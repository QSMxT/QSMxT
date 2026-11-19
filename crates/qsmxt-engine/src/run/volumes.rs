//! What the steps of a run read: which item holds each echo, mask and map in this run, and the
//! volumes several steps take, read from the store.

use super::{Echoes, Run, field_mask_written};
use crate::plan::Pass;
use crate::record::{Choice, Decision, Topic};
use crate::scan::RunMetadata;
use crate::stages;
use crate::store::{Item, StageStore};

impl<S: StageStore + ?Sized> Run<'_, S> {
    /// The box the FFT-based stages reconstruct in around `mask` ([`stages::crop::reconstruction_box`]),
    /// when the run crops or pads.
    pub(crate) fn reconstruction_box(&self, mask: &[u8]) -> Option<Decision> {
        let p = &self.config.pipeline;
        if !p.crop_to_mask && !p.fft_padding {
            return None;
        }
        let b = stages::crop::reconstruction_box(p.crop_to_mask, p.fft_padding, mask, self.meta.dims,
                                                 self.meta.voxel_size, p.crop_margin_mm);
        let (d, f) = (b.dims, b.full_dims);
        let size = format!("{}x{}x{} voxels, of {}x{}x{}", d.0, d.1, d.2, f.0, f.1, f.2);
        Some(if d == f {
            Decision::new(Topic::ReconstructionBox, Choice::Full,
                          "Reconstructed on the whole grid, which the box would not have changed.")
        } else if p.crop_to_mask {
            Decision::new(Topic::ReconstructionBox, Choice::Cropped, "Reconstructed in a box around the mask.")
        } else {
            Decision::new(Topic::ReconstructionBox, Choice::Padded,
                          "Reconstructed on the grid zero-padded to an FFT-efficient size.")
        }.with_detail(size))
    }

    pub(crate) fn n_echoes(&self) -> usize {
        self.meta.n_echoes
    }

    /// Whether the run reconstructs a QSM per echo ([`crate::bridge::per_echo`]).
    pub(crate) fn per_echo(&self) -> bool {
        crate::bridge::per_echo(self.config, self.n_echoes())
    }

    /// The run's metadata for echo `e` (1-based) alone: each step of a per-echo QSM sees the echo
    /// as a single-echo scan's (qsm-core `ScanMetadata::echo`).
    pub(crate) fn echo_meta(&self, e: usize) -> RunMetadata {
        RunMetadata { n_echoes: 1, echo_times: vec![self.meta.echo_times[e - 1]], ..self.meta.clone() }
    }

    /// Echo `e`'s magnitude, when the run has one: every magnitude a per-echo QSM's steps take
    /// (a single echo's combined magnitude, and its echoes' stack, are that echo's).
    pub(crate) fn one_echo_magnitude(&mut self, e: usize) -> Result<Option<Vec<f64>>, S::Error> {
        let item = self.echo_magnitude(e);
        if self.store.has(&item) { self.store.real(&item).map(Some) } else { Ok(None) }
    }

    /// The mask the stages after motion correction work in: the brain mask, or with motion
    /// correction the part of it every echo still covers.
    pub(crate) fn mask_item(&self) -> Item {
        if self.plan.motion_corrected { Item::CoverageMask } else { Item::BrainMask }
    }

    /// The mask a reconstruction pass runs on: the main pass's is where the total field is
    /// defined ([`Item::FieldMask`]) when the run maps one field from every echo, the run's mask
    /// otherwise; the reliable pass's is made inside that.
    pub(crate) fn pass_mask(&self, pass: Pass) -> Item {
        match pass {
            Pass::Main if field_mask_written(self.plan, self.config, self.n_echoes()) => Item::FieldMask,
            Pass::Main => self.mask_item(),
            Pass::Reliable => Item::ReliableMask,
        }
    }

    /// Where a pass's susceptibility map is defined: the mask background removal left, or the
    /// pass's mask when the inversion did its own background removal.
    pub(crate) fn support(&self, pass: Pass) -> Item {
        if crate::bridge::skips_bgremove(self.config) { self.pass_mask(pass) } else { Item::LocalFieldMask(pass) }
    }

    /// Echo `e`'s phase as the stages after motion correction read it.
    pub(crate) fn echo_phase(&self, e: usize) -> Item {
        if self.plan.motion_corrected { Item::CorrectedPhase(e) } else { Item::Phase(e) }
    }

    pub(crate) fn echo_magnitude(&self, e: usize) -> Item {
        if self.plan.motion_corrected { Item::CorrectedMagnitude(e) } else { Item::Magnitude(e) }
    }

    /// Every echo's phase and, when there are any, magnitude, as the stages after motion
    /// correction read them: what the phase-domain networks (iQSM, iQFM) take.
    pub(crate) fn phase_echoes(&mut self) -> Result<Echoes, S::Error> {
        let n = self.n_echoes();
        let phases = (1..=n).map(|e| { let i = self.echo_phase(e); self.store.real(&i) }).collect::<Result<Vec<_>, _>>()?;
        let mut magnitudes = Vec::new();
        for e in 1..=n {
            let item = self.echo_magnitude(e);
            if self.store.has(&item) {
                magnitudes.push(self.store.real(&item)?);
            }
        }
        Ok((phases, magnitudes))
    }

    /// Echo `e`'s magnitude as the stages after motion correction read it, falling back to the
    /// acquired echo when there is no working-grid copy (only for a run that was not resampled,
    /// where the two are the same grid).
    pub(crate) fn working_magnitude(&mut self, e: usize) -> Item {
        let item = self.echo_magnitude(e);
        if !self.store.has(&item) && self.store.has(&Item::AcquiredMagnitude(e)) { Item::AcquiredMagnitude(e) } else { item }
    }

    /// The segmentation regions are measured on: the run's own, or the one supplied with the scan
    /// (`--use-custom-dseg` without segmenting).
    pub(crate) fn segmentation_item(&mut self) -> Option<Item> {
        [Item::Segmentation, Item::SuppliedSegmentation].into_iter().find(|i| self.store.has(i))
    }

    /// Every echo's magnitude as the stages after motion correction read it, stacked with the
    /// echo slowest: what MEDI, TFI and PDF take. `None` for a run without magnitude.
    pub(crate) fn echo_magnitude_stack(&mut self) -> Result<Option<Vec<f64>>, S::Error> {
        let n = self.n_echoes();
        let mut stack = Vec::new();
        for e in 1..=n {
            let item = self.echo_magnitude(e);
            if !self.store.has(&item) {
                return Ok(None);
            }
            stack.extend(self.store.real(&item)?);
        }
        Ok(Some(stack))
    }

    pub(crate) fn combined_magnitude(&mut self) -> Result<Option<Vec<f64>>, S::Error> {
        if self.store.has(&Item::CombinedMagnitude) { self.store.real(&Item::CombinedMagnitude).map(Some) } else { Ok(None) }
    }
}
