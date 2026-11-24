//! What each step reads and writes ([`Ctx::io`]).

use super::Ctx;
use crate::bridge::skips_bgremove;
use crate::enums::*;
use crate::plan::{Pass, Stage};
use crate::requirements;
use crate::store::{Contrast, Item};

impl Ctx<'_> {
    /// The items a per-echo QSM's `stage` reads and writes, for the stages that run per echo.
    fn io_per_echo(&self, stage: Stage) -> Option<(Vec<Item>, Vec<Item>)> {
        let c = self.config;
        let (mut reads, mut writes) = (Vec::new(), Vec::new());
        match stage {
            Stage::FieldMap => {
                reads.extend(self.echo_phases());
                reads.extend(self.echo_magnitudes());
                reads.push(self.mask_item());
                writes.extend(self.echoes(Item::EchoField));
            }
            Stage::BackgroundRemoval(p) => {
                reads.extend(self.echoes(Item::EchoField));
                if crate::stages::background::takes_echo_magnitudes(c) {
                    reads.extend(self.echo_magnitudes());
                }
                reads.push(self.pass_mask(p));
                for e in 1..=self.n() {
                    writes.extend([Item::EchoLocalField(e), Item::EchoLocalFieldMask(e)]);
                }
            }
            Stage::Inversion(p) => {
                if skips_bgremove(c) {
                    reads.extend(self.echoes(Item::EchoField));
                    reads.push(self.pass_mask(p));
                } else {
                    reads.extend(self.echoes(Item::EchoLocalField));
                    reads.extend(self.echoes(Item::EchoLocalFieldMask));
                }
                // Each echo's own magnitude, whichever magnitude the inversion takes.
                if crate::stages::inversion::magnitude_input(c).is_some() {
                    reads.extend(self.echo_magnitudes());
                }
                writes.extend(self.echoes(Item::EchoRawChi));
            }
            Stage::Reference => {
                reads.extend(self.echoes(Item::EchoRawChi));
                if skips_bgremove(c) {
                    reads.push(self.pass_mask(Pass::Main));
                } else {
                    reads.extend(self.echoes(Item::EchoLocalFieldMask));
                }
                if c.qsm.reference == QsmReference::Region {
                    reads.extend(self.segmentation());
                }
                writes.extend(self.echoes(Item::EchoChi));
                writes.extend([Item::Chi, Item::ChiSupport]);
            }
            _ => return None,
        }
        Some((reads, writes))
    }

    /// The items `stage` reads and writes, as the run reads and writes them.
    pub(super) fn io(&self, stage: Stage, written: &[Item]) -> (Vec<Item>, Vec<Item>) {
        if self.per_echo() {
            if let Some(io) = self.io_per_echo(stage) {
                return io;
            }
        }
        let mut reads = Vec::new();
        let mut writes = Vec::new();
        match stage {
            Stage::Load | Stage::ScalePhase | Stage::TukeyWindow | Stage::ResampleAxial =>
                self.io_echoes(stage, written, &mut reads, &mut writes),
            Stage::InhomogeneityCorrection | Stage::Magnitude | Stage::Mask(_) | Stage::Motion =>
                self.io_magnitudes_and_masks(stage, &mut reads, &mut writes),
            Stage::FieldMap | Stage::BackgroundRemoval(_) => self.io_fields(stage, &mut reads, &mut writes),
            Stage::Inversion(_) | Stage::Tgv(_) | Stage::Qsmart(_) | Stage::TwoPassCombination | Stage::Reference
            | Stage::SinglePassReference => self.io_reconstruction(stage, written, &mut reads, &mut writes),
            Stage::Swi | Stage::Smwi | Stage::Relaxometry | Stage::R2 | Stage::ChiSeparation =>
                self.io_other_maps(stage, written, &mut reads, &mut writes),
            Stage::Segmentation | Stage::Analysis | Stage::OutputSpace => self.io_regions(stage, written, &mut reads, &mut writes),
        }
        (reads, writes)
    }

    /// [`Self::io`] of loading, and the steps that make the echoes.
    fn io_echoes(&self, stage: Stage, written: &[Item], reads: &mut Vec<Item>, writes: &mut Vec<Item>) {
        let (c, f) = (self.config, self.facts);
        match stage {
            Stage::Load => {
                if f.n_coils > 1 {
                    for coil in 1..=f.n_coils {
                        for e in 1..=self.n() {
                            writes.extend([Item::CoilPhase(coil, e), Item::CoilMagnitude(coil, e)]);
                        }
                    }
                } else if f.real_imaginary {
                    writes.extend(self.echoes(Item::AcquiredReal));
                    writes.extend(self.echoes(Item::AcquiredImaginary));
                } else {
                    writes.extend(self.echoes(Item::AcquiredPhase));
                    if f.has_magnitude {
                        writes.extend(self.echoes(Item::AcquiredMagnitude));
                    }
                }
                let s = &c.separation;
                for (supplied, item) in [
                    (self.supplied_mask(), Item::SuppliedMask),
                    (self.supplied_dseg(), Item::SuppliedSegmentation),
                    (s.custom_r2_tool.is_some() && f.custom_r2, Item::SuppliedR2),
                    (s.custom_r2prime_tool.is_some() && f.custom_r2prime, Item::SuppliedR2prime),
                    (s.custom_qsm_tool.is_some() && f.custom_qsm, Item::SuppliedChi),
                ] {
                    if supplied {
                        writes.push(item);
                    }
                }
                writes.extend(self.mese());
            }
            Stage::ScalePhase => {
                let chain = self.chain();
                if f.n_coils > 1 {
                    reads.extend(written.iter().copied().filter(|i| matches!(i, Item::CoilPhase(..) | Item::CoilMagnitude(..))));
                } else if f.real_imaginary {
                    reads.extend(self.echoes(Item::AcquiredReal));
                    reads.extend(self.echoes(Item::AcquiredImaginary));
                } else {
                    // A magnitude it keeps as it was given is a copy of the scan's, not read
                    // ([`Ctx::copies`]).
                    reads.extend(self.echoes(Item::AcquiredPhase));
                }
                for e in 1..=self.n() {
                    writes.push(chain.scaled_phase(e));
                    writes.extend(chain.scaled_magnitude(e));
                    writes.extend(self.combined_echo(e, chain.scaling_is_last()));
                }
                if f.n_coils > 1 {
                    writes.push(Item::CoilCombinationMask);
                }
            }
            Stage::TukeyWindow => {
                let chain = self.chain();
                for e in 1..=self.n() {
                    reads.push(chain.scaled_phase(e));
                    reads.extend(chain.acquired_magnitude(e));
                }
                for e in 1..=self.n() {
                    let (p, m) = chain.windowed(e);
                    writes.extend([p, m]);
                    writes.extend(self.combined_echo(e, !chain.resampled));
                }
            }
            // Nothing when the run is known not to be resampled: the step is skipped.
            Stage::ResampleAxial if self.chain().resampled => {
                let chain = self.chain();
                for e in 1..=self.n() {
                    let (p, m) = chain.to_resample(e);
                    reads.push(p);
                    reads.extend(m);
                }
                for e in 1..=self.n() {
                    writes.push(Item::Phase(e));
                    if chain.to_resample(e).1.is_some() {
                        writes.push(Item::Magnitude(e));
                    }
                    writes.extend(self.combined_echo(e, true));
                }
            }
            Stage::ResampleAxial => {}
            _ => unreachable!(),
        }
    }

    /// [`Self::io`] of the inhomogeneity correction, the combined magnitude, the masks and motion correction.
    fn io_magnitudes_and_masks(&self, stage: Stage, reads: &mut Vec<Item>, writes: &mut Vec<Item>) {
        let c = self.config;
        match stage {
            Stage::InhomogeneityCorrection => {
                let t = &self.plan.homogeneity;
                if t.vessel {
                    reads.extend(self.magnitudes());
                } else {
                    reads.extend(t.echoes.iter().map(|&e| Item::Magnitude(e)));
                }
                writes.extend(t.echoes.iter().map(|&e| Item::HomogeneousMagnitude(e)));
                if t.vessel {
                    writes.push(Item::VesselMagnitude);
                }
            }
            Stage::Magnitude => {
                reads.extend(self.corrected_magnitudes(&(1..=self.n()).collect::<Vec<_>>()));
                writes.push(Item::CombinedMagnitude);
            }
            Stage::Mask(Pass::Main) => {
                if self.supplied_mask() {
                    reads.push(Item::SuppliedMask);
                } else {
                    reads.extend(self.echoes(Item::Phase));
                    reads.extend(self.mask_magnitudes(&c.masking.sections));
                }
                writes.push(Item::BrainMask);
            }
            Stage::Mask(Pass::Reliable) => {
                reads.extend(self.echoes(Item::Phase));
                let mut sections = c.masking.resolved_two_pass_sections();
                let local = c.masking.two_pass_mode == crate::enums::TwoPassMode::Local;
                if local {
                    // The holes are selected on the phase-quality map, whatever the recipe reads.
                    sections.extend(crate::masking::default_two_pass_sections());
                }
                reads.extend(self.mask_magnitudes(&sections));
                reads.push(self.pass_mask(Pass::Main));
                if local {
                    let support = self.support(Pass::Main);
                    if !reads.contains(&support) {
                        reads.push(support);
                    }
                }
                writes.push(Item::ReliableMask);
            }
            Stage::Motion => {
                reads.extend(self.echoes(Item::Phase));
                reads.extend(self.magnitudes());
                reads.push(Item::BrainMask);
                writes.extend(self.echoes(Item::CorrectedPhase));
                writes.extend(self.echoes(Item::CorrectedMagnitude));
                writes.extend([Item::CoverageMask, Item::MotionReport]);
            }
            _ => unreachable!(),
        }
    }

    /// [`Self::io`] of the field map and background-field removal.
    fn io_fields(&self, stage: Stage, reads: &mut Vec<Item>, writes: &mut Vec<Item>) {
        let (c, f) = (self.config, self.facts);
        match stage {
            Stage::FieldMap => {
                reads.extend(self.echo_phases());
                reads.extend(self.echo_magnitudes());
                reads.push(self.mask_item());
                writes.extend([Item::FieldPpm, Item::TotalFieldHz]);
                if !self.per_echo() {
                    writes.push(Item::FieldMask);
                }
                if f.has_magnitude {
                    writes.push(Item::TotalFieldMagnitude);
                }
                if c.field_mapping.b0_estimation == B0Estimation::RobustFit && self.n() > 1 {
                    writes.push(Item::RobustFitReport);
                }
                if c.field_mapping.b0_estimation == B0Estimation::QsmartEchofit {
                    writes.push(Item::FieldReliability);
                }
            }
            Stage::BackgroundRemoval(p) => {
                if c.bg_removal.algorithm == BfAlgorithm::Iqfm {
                    reads.extend(self.echo_phases());
                    reads.extend(self.echo_magnitudes());
                } else {
                    reads.push(Item::FieldPpm);
                    // PDF's noise map is made from every echo's magnitude.
                    if crate::stages::background::takes_echo_magnitudes(c) {
                        reads.extend(self.echo_magnitudes());
                    }
                }
                reads.push(self.pass_mask(p));
                // A `local` run fills the reliable pass's eroded rim inside the main pass's support.
                if p == Pass::Reliable && c.masking.two_pass_mode == crate::enums::TwoPassMode::Local
                    && c.bg_removal.algorithm != BfAlgorithm::Iqfm
                {
                    reads.push(Item::LocalFieldMask(Pass::Main));
                }
                writes.extend([Item::LocalField(p), Item::LocalFieldMask(p)]);
            }
            _ => unreachable!(),
        }
    }

    /// [`Self::io`] of the inversions, the two-pass combination and referencing.
    fn io_reconstruction(&self, stage: Stage, written: &[Item], reads: &mut Vec<Item>, writes: &mut Vec<Item>) {
        let (c, f) = (self.config, self.facts);
        let has = |i: &Item| written.contains(i);
        match stage {
            Stage::Inversion(p) => {
                if crate::stages::inversion::iqsm_algorithm(c).is_some() {
                    reads.extend(self.echo_phases());
                    reads.extend(self.echo_magnitudes());
                    reads.push(self.pass_mask(p));
                } else {
                    if skips_bgremove(c) {
                        reads.extend([Item::FieldPpm, self.pass_mask(p)]);
                    } else {
                        reads.extend([Item::LocalField(p), Item::LocalFieldMask(p)]);
                    }
                    // The magnitude the inversion weights by: the combined one, or every echo's.
                    use crate::stages::inversion::{magnitude_input, MagnitudeInput};
                    match magnitude_input(c) {
                        Some(MagnitudeInput::Combined) if has(&Item::CombinedMagnitude) => {
                            reads.push(Item::CombinedMagnitude);
                        }
                        Some(MagnitudeInput::Echoes | MagnitudeInput::Rss) => reads.extend(self.echo_magnitudes()),
                        _ => {}
                    }
                }
                writes.push(Item::RawChi(p));
            }
            Stage::Tgv(p) => {
                reads.push(self.pass_mask(p));
                if self.n() > 1 {
                    reads.push(Item::FieldPpm);
                } else {
                    reads.extend(self.echo_phases().into_iter().take(1));
                }
                writes.push(Item::RawChi(p));
            }
            Stage::Qsmart(p) => {
                reads.extend([Item::FieldPpm, self.pass_mask(p)]);
                if c.field_mapping.b0_estimation == B0Estimation::QsmartEchofit {
                    reads.push(Item::FieldReliability);
                }
                writes.push(Item::RawChi(p));
                // The magnitude vessels are detected on: QSMART's own, made from every echo's
                // (and kept), or the combined one.
                match c.inversion.qsmart.vessel_magnitude {
                    // Made by the inhomogeneity correction, for both passes.
                    QsmartVesselMagnitude::MeanN4 if self.plan.homogeneity.vessel => reads.push(Item::VesselMagnitude),
                    // From the motion-corrected echoes, in the step.
                    QsmartVesselMagnitude::MeanN4 if f.has_magnitude => {
                        reads.extend(self.echo_magnitudes());
                        writes.push(Item::QsmartIntermediate(p, crate::stages::qsmart::VESSEL_MAGNITUDE));
                    }
                    QsmartVesselMagnitude::MeanN4 => {}
                    QsmartVesselMagnitude::Combined => if has(&Item::CombinedMagnitude) {
                        reads.push(Item::CombinedMagnitude);
                    },
                }
                for name in crate::stages::qsmart::reported(c) {
                    writes.push(Item::QsmartIntermediate(p, name));
                }
            }
            Stage::TwoPassCombination => {
                reads.extend([Item::RawChi(Pass::Reliable), Item::RawChi(Pass::Main), self.support(Pass::Reliable),
                              self.pass_mask(Pass::Main)]);
                // A `local` run weighs the second pass by the distance to the reliable mask's
                // holes inside the main pass's support.
                if c.masking.two_pass_mode == crate::enums::TwoPassMode::Local {
                    for item in [Item::ReliableMask, self.support(Pass::Main)] {
                        if !reads.contains(&item) {
                            reads.push(item);
                        }
                    }
                }
                // HEIDI fills the combined map with the main pass's local field and mask.
                if c.inversion.heidi.enabled {
                    reads.extend([Item::LocalField(Pass::Main), Item::LocalFieldMask(Pass::Main)]);
                }
                writes.push(Item::CombinedRawChi);
            }
            Stage::Reference | Stage::SinglePassReference => {
                let single = stage == Stage::SinglePassReference;
                if single || !self.plan.two_pass {
                    reads.extend([Item::RawChi(Pass::Main), self.support(Pass::Main)]);
                } else {
                    reads.extend([Item::CombinedRawChi, self.support(Pass::Main), self.support(Pass::Reliable)]);
                }
                if c.qsm.reference == QsmReference::Region {
                    reads.extend(self.segmentation());
                }
                if single {
                    writes.push(Item::SinglePassChi);
                } else {
                    writes.extend([Item::Chi, Item::ChiSupport]);
                }
            }
            _ => unreachable!(),
        }
    }

    /// [`Self::io`] of SWI, SMWI, relaxometry, R2 and R2′, and χ-separation.
    fn io_other_maps(&self, stage: Stage, written: &[Item], reads: &mut Vec<Item>, writes: &mut Vec<Item>) {
        let (c, f) = (self.config, self.facts);
        let has = |i: &Item| written.contains(i);
        match stage {
            Stage::Swi => {
                reads.extend(self.echo_phases());
                reads.extend(self.echo_magnitudes());
                writes.extend([Item::Swi, Item::SwiMip]);
                if c.swi.filtered_phase {
                    writes.push(Item::SwiPhase);
                }
            }
            Stage::Smwi => {
                reads.extend([Item::Chi, Item::CombinedMagnitude]);
                reads.push(if has(&Item::ChiSupport) { Item::ChiSupport } else { self.mask_item() });
                for contrast in Contrast::BOTH {
                    writes.extend([Item::Smwi(contrast), Item::SmwiMip(contrast)]);
                }
            }
            Stage::Relaxometry => {
                reads.push(self.mask_item());
                reads.extend(self.echo_magnitudes());
                writes.extend([Item::R2star, Item::T2star]);
            }
            Stage::R2 => {
                let s = &c.separation;
                reads.push(self.mask_item());
                let r2 = if s.custom_r2_tool.is_some() {
                    has(&Item::SuppliedR2).then_some(Item::SuppliedR2)
                } else {
                    let mese = self.mese();
                    let usable = !mese.is_empty() && requirements::mese_problem(f).is_none();
                    reads.extend(mese);
                    usable.then_some(Item::R2)
                };
                if let Some(supplied @ Item::SuppliedR2) = r2 {
                    reads.push(supplied);
                }
                if r2.is_some() {
                    writes.push(Item::R2);
                }
                if c.pipeline.do_r2primemap {
                    if s.custom_r2prime_tool.is_some() {
                        if has(&Item::SuppliedR2prime) {
                            reads.push(Item::SuppliedR2prime);
                            writes.push(Item::R2prime);
                        }
                    } else if has(&Item::R2star) {
                        reads.push(Item::R2star);
                        let measured = r2.is_some() && s.r2prime_strategy != R2PrimeStrategy::R2primenet;
                        if measured {
                            writes.push(Item::R2primeCoverage);
                        }
                        if measured || s.r2prime_strategy != R2PrimeStrategy::Mese {
                            writes.push(Item::R2prime);
                        }
                    }
                }
            }
            Stage::ChiSeparation => {
                reads.push(self.mask_item());
                if crate::stages::separation::reconstructs_qsm(c) {
                    // Its QSM is made from the local field, inside that field's mask.
                    reads.push(Item::LocalFieldMask(Pass::Main));
                } else {
                    let chi = if c.separation.custom_qsm_tool.is_some() { Item::SuppliedChi } else { Item::Chi };
                    reads.push(chi);
                    if chi == Item::Chi && has(&Item::ChiSupport) {
                        reads.push(Item::ChiSupport);
                    }
                    // DECOMPOSE takes a per-echo QSM's echoes' maps.
                    if chi == Item::Chi && c.separation.algorithm == SeparationAlgorithm::Decompose && self.per_echo() {
                        reads.extend(self.echoes(Item::EchoChi));
                    }
                }
                reads.extend(self.echo_magnitudes());
                let r2prime = if c.separation.custom_r2prime_tool.is_some() { Item::SuppliedR2prime } else { Item::R2prime };
                // χ-sepnet's toolbox route makes its own R2′ unless it is given a measured one, so
                // an R2′ map predicted beside it is no input to it.
                let route_estimates = crate::config::chisepnet_route_can_estimate_r2prime(c)
                    && !(has(&Item::R2) && c.separation.r2prime_strategy != R2PrimeStrategy::R2primenet);
                for item in [Item::R2star, r2prime, Item::LocalField(Pass::Main)] {
                    if has(&item) && !(route_estimates && item == Item::R2prime) {
                        reads.push(item);
                    }
                }
                reads.extend(self.mese());
                writes.extend([Item::SeparatedChi(Contrast::Paramagnetic), Item::SeparatedChi(Contrast::Diamagnetic),
                               Item::SeparatedTotalChi]);
            }
            _ => unreachable!(),
        }
    }

    /// [`Self::io`] of the segmentation, the statistics and the return to the acquired grid.
    fn io_regions(&self, stage: Stage, written: &[Item], reads: &mut Vec<Item>, writes: &mut Vec<Item>) {
        let c = self.config;
        let has = |i: &Item| written.contains(i);
        match stage {
            Stage::Segmentation => {
                if self.supplied_dseg() {
                    reads.push(Item::SuppliedSegmentation);
                    writes.extend([Item::Segmentation, Item::SegmentationLabels]);
                } else {
                    reads.push(Item::CombinedMagnitude);
                    writes.extend([Item::Segmentation, Item::SegmentationLabels, Item::SegmentationVolumes]);
                }
            }
            Stage::Analysis => {
                reads.extend(self.segmentation());
                let mut maps: Vec<Item> = crate::stages::analysis::MAPS.iter().map(|(i, ..)| *i).filter(|i| has(i)).collect();
                if c.separation.custom_qsm_tool.is_some() && !maps.contains(&Item::Chi) && has(&Item::SuppliedChi) {
                    maps.insert(0, Item::SuppliedChi);
                }
                reads.extend(maps);
                if has(&Item::SegmentationVolumes) {
                    reads.push(Item::SegmentationVolumes);
                }
                writes.push(Item::RegionStats);
            }
            Stage::OutputSpace => {
                let published: Vec<Item> = crate::stages::output_space::published(self.n()).into_iter()
                    .map(|(i, _)| i).filter(|i| has(i) && i.published_in(c)).collect();
                reads.extend(published.iter().copied());
                writes.extend(published);
            }
            _ => unreachable!(),
        }
    }
}
