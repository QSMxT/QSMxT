//! What a step reads and writes: [`Item`], and [`Contrast`].

use crate::plan::Pass;

/// A volume (or report) a step reads or writes, by what it is. A store keeps each under its own
/// name for it (`qsmxt`: a path).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum Item {
    /// Echo `n`'s phase (1-based), as acquired.
    AcquiredPhase(usize),
    /// Echo `n`'s magnitude, as acquired, when there is one.
    AcquiredMagnitude(usize),
    /// Echo `n`'s real part, as acquired, for a scan given as real and imaginary parts.
    AcquiredReal(usize),
    /// Echo `n`'s imaginary part, as acquired, for a scan given as real and imaginary parts.
    AcquiredImaginary(usize),
    /// Uncombined receive coil `c`'s (1-based, in [`StageStore::coils`] order) echo `n` phase, as
    /// acquired.
    CoilPhase(usize, usize),
    /// Coil `c`'s echo `n` magnitude, as acquired.
    CoilMagnitude(usize, usize),
    /// Echo `n`'s phase (radians) combined from the uncombined coils, published.
    CombinedEchoPhase(usize),
    /// Echo `n`'s magnitude combined from the uncombined coils, published.
    CombinedEchoMagnitude(usize),
    /// The mask the coil combination estimated each coil's phase offset in, on the acquired grid.
    CoilCombinationMask,
    /// Echo `n`'s phase in radians on the acquired grid, from phase scaling, when the Tukey window
    /// or the resampling to an axial grid comes after it (otherwise phase scaling writes
    /// [`Item::Phase`] itself).
    ScaledPhase(usize),
    /// Echo `n`'s magnitude on the acquired grid, from phase scaling, when phase scaling makes one
    /// (from real and imaginary parts, or combined from uncombined coils) and another step comes
    /// after it.
    ScaledMagnitude(usize),
    /// Echo `n`'s phase after the k-space Tukey window, on the acquired grid, when the run is
    /// resampled to an axial grid after it.
    WindowedPhase(usize),
    /// Echo `n`'s magnitude after the k-space Tukey window, on the acquired grid, when the run is
    /// resampled after it.
    WindowedMagnitude(usize),
    /// Echo `n`'s phase in radians, on the working grid.
    Phase(usize),
    /// Echo `n`'s magnitude on the working grid.
    Magnitude(usize),
    /// Echo `n`'s phase after motion correction.
    CorrectedPhase(usize),
    /// Echo `n`'s magnitude after motion correction.
    CorrectedMagnitude(usize),
    /// Echo `n`'s magnitude corrected for inhomogeneity (the `inhomog` step), as masking and the
    /// combined magnitude read it.
    HomogeneousMagnitude(usize),
    /// QSMART's vessel magnitude: the mean of every echo's magnitude, corrected with N4 as QSMART
    /// does (the `inhomog` step).
    VesselMagnitude,
    /// The RSS-combined magnitude.
    CombinedMagnitude,
    /// The brain mask.
    BrainMask,
    /// A brain mask supplied with the scan (`--use-custom-masks`), as found.
    SuppliedMask,
    /// The brain mask restricted to the voxels every echo covers after motion correction.
    CoverageMask,
    /// The total field in ppm, zero outside the mask.
    FieldPpm,
    /// The total field in Hz, as it is published.
    TotalFieldHz,
    /// Echo 1's magnitude, published beside the total field.
    TotalFieldMagnitude,
    /// The robust echo fit's report (a JSON document).
    RobustFitReport,
    /// QSMART's reliability map `R_0` from its echo fit (1 = reliable), over the whole volume:
    /// where QSMART reconstructs, within the brain mask.
    FieldReliability,
    /// The run's mask restricted to where the total field is defined: phase-offset removal leaves
    /// no field where it could not estimate the phase offset (qsm-core's
    /// `FieldMappingResult::valid_mask`). Background removal and the reconstructions of the total
    /// field run inside it.
    FieldMask,
    /// A pass's local field in ppm.
    LocalField(Pass),
    /// Where a pass's local field is defined.
    LocalFieldMask(Pass),
    /// A pass's raw (unreferenced) susceptibility map.
    RawChi(Pass),
    /// The reliable-phase mask of a two-pass run.
    ReliableMask,
    /// One of QSMART's intermediates, by its name in [`crate::stages::qsmart::outputs`].
    QsmartIntermediate(Pass, &'static str),
    /// The two passes' raw maps combined.
    CombinedRawChi,
    /// Echo `n`'s field in ppm, a per-echo QSM's (`qsm.per_echo`): its phase unwrapped and
    /// divided by its own echo time, zero outside the mask.
    EchoField(usize),
    /// Echo `n`'s local field in ppm, a per-echo QSM's.
    EchoLocalField(usize),
    /// Where echo `n`'s local field is defined.
    EchoLocalFieldMask(usize),
    /// Echo `n`'s raw (unreferenced) susceptibility map, a per-echo QSM's.
    EchoRawChi(usize),
    /// Echo `n`'s susceptibility map, referenced over its own support. The mean of the echoes'
    /// maps is [`Item::Chi`].
    EchoChi(usize),
    /// The final, referenced susceptibility map.
    Chi,
    /// Where the final map is defined.
    ChiSupport,
    /// A two-pass run's single-pass map, referenced.
    SinglePassChi,
    /// The run's segmentation (FreeSurfer label ids).
    Segmentation,
    /// A segmentation supplied with the scan (`--use-custom-dseg`), on whatever grid it came on.
    SuppliedSegmentation,
    /// The segmentation's lookup table: each label id and its name (text, a BIDS `dseg.tsv`).
    SegmentationLabels,
    /// SynthSeg's per-label volumes in mm³, in label-table order (a JSON array).
    SegmentationVolumes,
    /// Per-structure statistics of every map over the segmentation (text, a TSV table).
    RegionStats,
    /// The motion correction's per-echo report (a JSON document).
    MotionReport,
    /// The CLEAR-SWI image.
    Swi,
    /// The SWI's minimum-intensity projection, on its own grid ([`Data::OnGrid`]).
    SwiMip,
    /// CLEAR-SWI's high-pass filtered phase: Hz for several echoes, radians for one (its
    /// sidecar's `Units`).
    SwiPhase,
    /// A susceptibility map-weighted image, suppressing one sign of χ.
    Smwi(Contrast),
    /// An SMWI's minimum-intensity projection, on its own grid.
    SmwiMip(Contrast),
    /// The R2* map (Hz).
    R2star,
    /// The T2* map (s).
    T2star,
    /// Echo `n` (1-based) of a matched multi-echo spin-echo (MESE) acquisition, on its own grid
    /// ([`StageStore::mese`]).
    MeseMagnitude(usize),
    /// An R2 map supplied with the scan (`--use-custom-r2`).
    SuppliedR2,
    /// The R2 map (Hz).
    R2,
    /// An R2′ map supplied with the scan (`--use-custom-r2prime`).
    SuppliedR2prime,
    /// The R2′ map (Hz).
    R2prime,
    /// Where a measured R2′ is valid: the brain mask, narrowed to where R2 was measured.
    R2primeCoverage,
    /// A susceptibility map supplied with the scan (`--use-custom-qsm`), in place of the run's.
    SuppliedChi,
    /// One sign's χ-separation map, as a magnitude (both are positive).
    SeparatedChi(Contrast),
    /// The χ-separation net susceptibility, χ+ + χ−.
    SeparatedTotalChi,
}

impl Item {
    /// Whether a store that keeps volumes in float32 (`qsmxt run`'s NIfTI files) keeps this one in
    /// double precision: the magnitude echoes QSMxT computes before masking (made from coils or
    /// real and imaginary parts, Tukey-windowed, resampled to an axial grid). The mask and every
    /// later step read them as computed, as the toolboxes, which keep double throughout, do: the
    /// χ-separation toolbox's BET rounds only the RSS of its windowed echoes to single, and the RSS
    /// of float32-rounded echoes differs from that in the last bit at about half of the head's
    /// voxels, which moves BET's surface (QSMxT/QSM-CI#421). A magnitude kept as it was given
    /// ([`Data::Same`]) is the input file's, already exact. The inhomogeneity correction's
    /// outputs, which masking and QSMART's vessel detection read, are kept in double for the same
    /// reason: those steps corrected the echoes themselves, in double, before the correction was a
    /// step of its own.
    pub fn kept_in_double(&self) -> bool {
        matches!(self, Item::ScaledMagnitude(_) | Item::WindowedMagnitude(_) | Item::Magnitude(_)
            | Item::HomogeneousMagnitude(_) | Item::VesselMagnitude)
    }

    /// The name a front end keeps the item under: as Rust writes it (`AcquiredPhase(1)`,
    /// `LocalField(Main)`, `CoilPhase(2, 1)`, `QsmartIntermediate(Main, "vessel_mask")`).
    /// [`Item::from_key`] reads it back.
    pub fn key(&self) -> String {
        format!("{self:?}")
    }

    /// The item [`Item::key`] wrote as `key`; `None` for one this engine does not have.
    pub fn from_key(key: &str) -> Option<Item> {
        let (name, args) = match key.split_once('(') {
            Some((name, rest)) => (name, Some(rest.strip_suffix(')')?)),
            None => (key, None),
        };
        let pass = |a: &str| match a.trim() {
            "Main" => Some(Pass::Main),
            "Reliable" => Some(Pass::Reliable),
            _ => None,
        };
        let contrast = |a: Option<&str>| {
            let a = a?.trim();
            Contrast::BOTH.into_iter().find(|c| format!("{c:?}") == a)
        };
        let n = |a: &str| a.trim().parse::<usize>().ok();
        let echo = |a: Option<&str>| n(a?);
        let pair = |a: Option<&str>| {
            let (c, e) = a?.split_once(',')?;
            Some((n(c)?, n(e)?))
        };
        use qsm_core::pipeline::qsmart::outputs as q;
        Some(match (name, args) {
            ("AcquiredPhase", a) => Item::AcquiredPhase(echo(a)?),
            ("AcquiredMagnitude", a) => Item::AcquiredMagnitude(echo(a)?),
            ("AcquiredReal", a) => Item::AcquiredReal(echo(a)?),
            ("AcquiredImaginary", a) => Item::AcquiredImaginary(echo(a)?),
            ("CoilPhase", a) => { let (c, e) = pair(a)?; Item::CoilPhase(c, e) }
            ("CoilMagnitude", a) => { let (c, e) = pair(a)?; Item::CoilMagnitude(c, e) }
            ("CombinedEchoPhase", a) => Item::CombinedEchoPhase(echo(a)?),
            ("CombinedEchoMagnitude", a) => Item::CombinedEchoMagnitude(echo(a)?),
            ("ScaledPhase", a) => Item::ScaledPhase(echo(a)?),
            ("ScaledMagnitude", a) => Item::ScaledMagnitude(echo(a)?),
            ("WindowedPhase", a) => Item::WindowedPhase(echo(a)?),
            ("WindowedMagnitude", a) => Item::WindowedMagnitude(echo(a)?),
            ("Phase", a) => Item::Phase(echo(a)?),
            ("Magnitude", a) => Item::Magnitude(echo(a)?),
            ("CorrectedPhase", a) => Item::CorrectedPhase(echo(a)?),
            ("CorrectedMagnitude", a) => Item::CorrectedMagnitude(echo(a)?),
            ("HomogeneousMagnitude", a) => Item::HomogeneousMagnitude(echo(a)?),
            ("MeseMagnitude", a) => Item::MeseMagnitude(echo(a)?),
            ("EchoField", a) => Item::EchoField(echo(a)?),
            ("EchoLocalField", a) => Item::EchoLocalField(echo(a)?),
            ("EchoLocalFieldMask", a) => Item::EchoLocalFieldMask(echo(a)?),
            ("EchoRawChi", a) => Item::EchoRawChi(echo(a)?),
            ("EchoChi", a) => Item::EchoChi(echo(a)?),
            ("LocalField", Some(a)) => Item::LocalField(pass(a)?),
            ("LocalFieldMask", Some(a)) => Item::LocalFieldMask(pass(a)?),
            ("RawChi", Some(a)) => Item::RawChi(pass(a)?),
            ("QsmartIntermediate", Some(a)) => {
                let (p, n) = a.split_once(',')?;
                let n = n.trim().strip_prefix('"')?.strip_suffix('"')?;
                let name = [crate::stages::qsmart::VESSEL_MAGNITUDE, q::VESSEL_MASK, q::LOCAL_FIELD_STAGE1, q::CHI_STAGE1,
                            q::LOCAL_FIELD_STAGE2, q::CHI_STAGE2]
                    .into_iter().find(|&known| known == n)?;
                Item::QsmartIntermediate(pass(p)?, name)
            }
            ("Smwi", a) => Item::Smwi(contrast(a)?),
            ("SmwiMip", a) => Item::SmwiMip(contrast(a)?),
            ("SeparatedChi", a) => Item::SeparatedChi(contrast(a)?),
            (name, None) => [
                Item::CoilCombinationMask, Item::VesselMagnitude, Item::CombinedMagnitude, Item::BrainMask, Item::SuppliedMask,
                Item::CoverageMask, Item::FieldPpm, Item::TotalFieldHz, Item::TotalFieldMagnitude,
                Item::RobustFitReport, Item::FieldReliability, Item::FieldMask, Item::ReliableMask, Item::CombinedRawChi,
                Item::Chi, Item::ChiSupport, Item::SinglePassChi, Item::Segmentation, Item::SuppliedSegmentation, Item::SegmentationLabels,
                Item::SegmentationVolumes, Item::RegionStats, Item::MotionReport, Item::Swi, Item::SwiMip,
                Item::SwiPhase, Item::R2star, Item::T2star, Item::SuppliedR2, Item::R2, Item::SuppliedR2prime, Item::R2prime,
                Item::R2primeCoverage, Item::SuppliedChi, Item::SeparatedTotalChi,
            ].into_iter().find(|item| item.key() == name)?,
            _ => return None,
        })
    }
}

/// Which sign of susceptibility an output is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum Contrast {
    /// Positive χ (iron, haemorrhage).
    Paramagnetic,
    /// Negative χ (calcification, myelin).
    Diamagnetic,
}

impl Contrast {
    pub const BOTH: [Contrast; 2] = [Contrast::Paramagnetic, Contrast::Diamagnetic];

    pub fn name(self) -> &'static str {
        match self {
            Contrast::Paramagnetic => "paramagnetic",
            Contrast::Diamagnetic => "diamagnetic",
        }
    }
}
