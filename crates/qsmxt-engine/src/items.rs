//! What each [`Item`] is, for a front end to show it: its label, units, kind and the BIDS name
//! `qsmxt run` gives its file.
//!
//! A front end lists a run's outputs by step, labels them, puts their units on a colour bar and
//! chooses how to display them by kind (a χ map, a field, a mask...). All of that comes from
//! here, so no front end restates it per output.

use serde::{Deserialize, Serialize};

use crate::plan::Pass;
use crate::stages::qsmart::outputs as qsmart;
use crate::store::{Contrast, Item};

/// What an item holds, for choosing how to display it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    /// A susceptibility map (ppm).
    Chi,
    /// A field map: total or local (ppm or Hz).
    Field,
    /// A magnitude image.
    Magnitude,
    /// A phase image (or, for a scan given as real and imaginary parts, one of those parts).
    Phase,
    /// A relaxation map: T2*, R2*, R2 or R2′.
    Relaxometry,
    /// A binary mask.
    Mask,
    /// A label image (FreeSurfer ids).
    Segmentation,
    /// A susceptibility-weighted image (SWI, SMWI) or its minimum-intensity projection.
    Swi,
    /// A table (text).
    Table,
    /// A report (a JSON document).
    Report,
}

/// What [`Item::meta`] says about an item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ItemMeta {
    /// What it is called in a front end, without its echo or coil ("Phase", "Local field").
    pub label: String,
    /// Its units, as `qsmxt run` writes them (`ppm`, `Hz`, `s`, `s-1`, `rad`), when it has any.
    pub units: Option<&'static str>,
    pub kind: ItemKind,
    /// The entities and suffix `qsmxt run` names its file with, after the run's own entities
    /// (`desc-qsm_mask`, `Chimap`); none for an input, for what `qsmxt` keeps in its state rather
    /// than a file, and for an echo's image, whose suffix is the input's.
    pub bids_name: Option<String>,
    /// `qsmxt run` publishes it among the run's outputs, rather than keeping it as an
    /// intermediate in its step's workflow directory (or not at all).
    pub published: bool,
    /// The echo it belongs to (1-based), for a per-echo item.
    pub echo: Option<usize>,
    /// The receive coil it belongs to (1-based), for an uncombined coil's image.
    pub coil: Option<usize>,
}

/// " (reliable pass)" for the second pass of a two-pass run.
fn pass_suffix(pass: Pass) -> &'static str {
    match pass {
        Pass::Main => "",
        Pass::Reliable => " (reliable pass)",
    }
}

impl Item {
    /// What the item is: label, units, kind and BIDS name.
    pub fn meta(&self) -> ItemMeta {
        use ItemKind as K;
        let (label, units, kind, bids_name): (String, Option<&'static str>, ItemKind, Option<String>) = match *self {
            Item::AcquiredPhase(_) => ("Phase (as acquired)".into(), None, K::Phase, None),
            Item::AcquiredMagnitude(_) => ("Magnitude (as acquired)".into(), None, K::Magnitude, None),
            Item::AcquiredReal(_) => ("Real part (as acquired)".into(), None, K::Phase, None),
            Item::AcquiredImaginary(_) => ("Imaginary part (as acquired)".into(), None, K::Phase, None),
            Item::CoilPhase(..) => ("Coil phase".into(), None, K::Phase, None),
            Item::CoilMagnitude(..) => ("Coil magnitude".into(), None, K::Magnitude, None),
            Item::CombinedEchoPhase(_) => ("Coil-combined phase".into(), Some("rad"), K::Phase, None),
            Item::CombinedEchoMagnitude(_) => ("Coil-combined magnitude".into(), None, K::Magnitude, None),
            Item::CoilCombinationMask => ("Coil-combination mask".into(), None, K::Mask, Some("desc-mcpc3ds_mask".into())),
            Item::Phase(_) => ("Phase".into(), Some("rad"), K::Phase, None),
            Item::Magnitude(_) => ("Magnitude".into(), None, K::Magnitude, None),
            Item::CorrectedPhase(_) => ("Motion-corrected phase".into(), Some("rad"), K::Phase, None),
            Item::CorrectedMagnitude(_) => ("Motion-corrected magnitude".into(), None, K::Magnitude, None),
            Item::CombinedMagnitude => ("Combined magnitude".into(), None, K::Magnitude, Some("part-mag_T2starw".into())),
            Item::BrainMask => ("Brain mask".into(), None, K::Mask, Some("mask".into())),
            Item::SuppliedMask => ("Supplied brain mask".into(), None, K::Mask, None),
            Item::CoverageMask => ("Motion coverage mask".into(), None, K::Mask, Some("desc-motioncoverage_mask".into())),
            Item::FieldPpm => ("Total field".into(), Some("ppm"), K::Field, Some("desc-totalppm_fieldmap".into())),
            Item::TotalFieldHz => ("Total field".into(), Some("Hz"), K::Field, Some("desc-total_fieldmap".into())),
            Item::TotalFieldMagnitude => ("Field map magnitude".into(), None, K::Magnitude, Some("desc-total_magnitude".into())),
            Item::RobustFitReport => ("Robust echo fit report".into(), None, K::Report, Some("desc-totalppm_fieldmap_robustfit".into())),
            Item::FieldReliability => ("Echo-fit reliability mask".into(), None, K::Mask, Some("desc-echofit_mask".into())),
            Item::LocalField(p) => (format!("Local field{}", pass_suffix(p)), Some("ppm"), K::Field, Some("localfield".into())),
            Item::LocalFieldMask(p) => (format!("Local field mask{}", pass_suffix(p)), None, K::Mask, Some("bgmask".into())),
            Item::RawChi(p) => (format!("Unreferenced susceptibility{}", pass_suffix(p)), Some("ppm"), K::Chi,
                                Some("desc-unreferenced_Chimap".into())),
            Item::ReliableMask => ("Reliable-phase mask".into(), None, K::Mask, Some("desc-reliable_mask".into())),
            Item::QsmartIntermediate(p, name) => {
                let (label, units, kind, bids) = match name {
                    crate::stages::qsmart::VESSEL_MAGNITUDE => ("QSMART vessel magnitude", None, K::Magnitude, "part-mag_desc-vessels_T2starw"),
                    qsmart::VESSEL_MASK => ("QSMART vessel mask", None, K::Mask, "desc-vessels_mask"),
                    qsmart::LOCAL_FIELD_STAGE1 => ("QSMART local field, stage 1", Some("ppm"), K::Field, "desc-stage1_localfield"),
                    qsmart::CHI_STAGE1 => ("QSMART susceptibility, stage 1", Some("ppm"), K::Chi, "desc-stage1_Chimap"),
                    qsmart::LOCAL_FIELD_STAGE2 => ("QSMART local field, stage 2", Some("ppm"), K::Field, "desc-stage2_localfield"),
                    qsmart::CHI_STAGE2 => ("QSMART susceptibility, stage 2", Some("ppm"), K::Chi, "desc-stage2_Chimap"),
                    // An intermediate QSMxT has no name for is not saved (`qsmxt run` warns).
                    other => return ItemMeta {
                        label: format!("QSMART {other}{}", pass_suffix(p)), units: None, kind: K::Field,
                        bids_name: None, published: false, echo: None, coil: None,
                    },
                };
                (format!("{label}{}", pass_suffix(p)), units, kind, Some(bids.into()))
            }
            Item::CombinedRawChi => ("Combined unreferenced susceptibility".into(), Some("ppm"), K::Chi,
                                     Some("desc-unreferenced_Chimap".into())),
            Item::Chi => ("Susceptibility".into(), Some("ppm"), K::Chi, Some("Chimap".into())),
            Item::ChiSupport => ("Susceptibility support".into(), None, K::Mask, Some("desc-qsm_mask".into())),
            Item::SinglePassChi => ("Single-pass susceptibility".into(), Some("ppm"), K::Chi, Some("desc-singlepass_Chimap".into())),
            Item::Segmentation => ("Segmentation".into(), None, K::Segmentation, Some("dseg".into())),
            Item::SuppliedSegmentation => ("Supplied segmentation".into(), None, K::Segmentation, None),
            Item::SegmentationLabels => ("Segmentation labels".into(), None, K::Table, Some("dseg".into())),
            Item::SegmentationVolumes => ("Structure volumes".into(), Some("mm3"), K::Report, None),
            Item::RegionStats => ("Region statistics".into(), None, K::Table, Some("desc-segmentation_stats".into())),
            Item::MotionReport => ("Motion report".into(), None, K::Report, Some("motion".into())),
            Item::Swi => ("SWI".into(), None, K::Swi, Some("swi".into())),
            Item::SwiMip => ("SWI minimum-intensity projection".into(), None, K::Swi, Some("minIP".into())),
            Item::Smwi(c) => (format!("SMWI ({})", c.name()), None, K::Swi, Some(format!("desc-{}_smwi", c.name()))),
            Item::SmwiMip(c) => (format!("SMWI minimum-intensity projection ({})", c.name()), None, K::Swi,
                                 Some(format!("desc-{}smwi_minIP", c.name()))),
            Item::R2star => ("R2*".into(), Some("s-1"), K::Relaxometry, Some("R2starmap".into())),
            Item::T2star => ("T2*".into(), Some("s"), K::Relaxometry, Some("T2starmap".into())),
            Item::MeseMagnitude(_) => ("Spin-echo magnitude".into(), None, K::Magnitude, None),
            Item::SuppliedR2 => ("Supplied R2".into(), Some("s-1"), K::Relaxometry, None),
            Item::R2 => ("R2".into(), Some("s-1"), K::Relaxometry, Some("R2map".into())),
            Item::SuppliedR2prime => ("Supplied R2′".into(), Some("s-1"), K::Relaxometry, None),
            Item::R2prime => ("R2′".into(), Some("s-1"), K::Relaxometry, Some("R2primemap".into())),
            Item::R2primeCoverage => ("R2′ coverage".into(), None, K::Mask, Some("desc-r2prime_mask".into())),
            Item::SuppliedChi => ("Supplied susceptibility".into(), Some("ppm"), K::Chi, None),
            Item::SeparatedChi(Contrast::Paramagnetic) => ("Paramagnetic susceptibility".into(), Some("ppm"), K::Chi,
                                                           Some("desc-paramagnetic_Chimap".into())),
            Item::SeparatedChi(Contrast::Diamagnetic) => ("Diamagnetic susceptibility".into(), Some("ppm"), K::Chi,
                                                          Some("desc-diamagnetic_Chimap".into())),
            Item::SeparatedTotalChi => ("χ-separation total susceptibility".into(), Some("ppm"), K::Chi,
                                        Some("desc-total_Chimap".into())),
        };
        let published = matches!(self, Item::RegionStats | Item::SegmentationLabels)
            || crate::stages::output_space::published(self.echo().unwrap_or(0)).iter().any(|(i, _)| i == self);
        let coil = match *self {
            Item::CoilPhase(c, _) | Item::CoilMagnitude(c, _) => Some(c),
            _ => None,
        };
        ItemMeta { label, units, kind, bids_name, published, echo: self.echo(), coil }
    }

    /// One of every item, each variant at least once (both passes and contrasts, and every QSMART
    /// intermediate QSMxT names). `meta` matches on the variant without a wildcard, so a new
    /// variant cannot compile without its metadata; this list is what its tests check it on.
    pub fn one_of_each() -> Vec<Item> {
        let mut items = vec![
            Item::AcquiredPhase(1), Item::AcquiredMagnitude(1), Item::AcquiredReal(1), Item::AcquiredImaginary(1),
            Item::CoilPhase(1, 1), Item::CoilMagnitude(1, 1), Item::CombinedEchoPhase(1),
            Item::CombinedEchoMagnitude(1), Item::CoilCombinationMask, Item::Phase(1), Item::Magnitude(1),
            Item::CorrectedPhase(1), Item::CorrectedMagnitude(1), Item::CombinedMagnitude, Item::BrainMask,
            Item::SuppliedMask, Item::CoverageMask, Item::FieldPpm, Item::TotalFieldHz,
            Item::TotalFieldMagnitude, Item::RobustFitReport, Item::FieldReliability, Item::ReliableMask, Item::CombinedRawChi,
            Item::Chi, Item::ChiSupport, Item::SinglePassChi, Item::Segmentation,
            Item::SuppliedSegmentation, Item::SegmentationLabels, Item::SegmentationVolumes,
            Item::RegionStats, Item::MotionReport, Item::Swi, Item::SwiMip, Item::R2star, Item::T2star,
            Item::MeseMagnitude(1), Item::SuppliedR2, Item::R2, Item::SuppliedR2prime, Item::R2prime,
            Item::R2primeCoverage, Item::SuppliedChi, Item::SeparatedTotalChi,
        ];
        for p in [Pass::Main, Pass::Reliable] {
            items.extend([Item::LocalField(p), Item::LocalFieldMask(p), Item::RawChi(p)]);
            for name in [crate::stages::qsmart::VESSEL_MAGNITUDE, qsmart::VESSEL_MASK, qsmart::LOCAL_FIELD_STAGE1,
                         qsmart::CHI_STAGE1, qsmart::LOCAL_FIELD_STAGE2, qsmart::CHI_STAGE2] {
                items.push(Item::QsmartIntermediate(p, name));
            }
        }
        for c in Contrast::BOTH {
            items.extend([Item::Smwi(c), Item::SmwiMip(c), Item::SeparatedChi(c)]);
        }
        items
    }

    /// The echo the item belongs to (1-based), for a per-echo item.
    pub fn echo(&self) -> Option<usize> {
        match *self {
            Item::AcquiredPhase(e) | Item::AcquiredMagnitude(e) | Item::AcquiredReal(e) | Item::AcquiredImaginary(e)
            | Item::CoilPhase(_, e) | Item::CoilMagnitude(_, e)
            | Item::CombinedEchoPhase(e) | Item::CombinedEchoMagnitude(e)
            | Item::Phase(e) | Item::Magnitude(e) | Item::CorrectedPhase(e) | Item::CorrectedMagnitude(e)
            | Item::MeseMagnitude(e) => Some(e),
            _ => None,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn every_item_has_a_label_and_the_two_passes_are_told_apart() {
        let items = Item::one_of_each();
        for item in &items {
            let m = item.meta();
            assert!(!m.label.is_empty(), "{item:?}");
            assert!(!m.label.contains("STI") && !m.label.contains("Biobank"), "{item:?}");
        }
        // Within a run, two items with the same label must differ in echo, coil or units (the
        // total field in ppm and in Hz), or be the same thing under another name.
        for (i, a) in items.iter().enumerate() {
            for b in &items[i + 1..] {
                let (ma, mb) = (a.meta(), b.meta());
                if ma.label == mb.label {
                    assert!(ma.units != mb.units, "{a:?} and {b:?} are both labelled {:?}", ma.label);
                }
            }
        }
        assert_eq!(Item::RawChi(Pass::Reliable).meta().label, "Unreferenced susceptibility (reliable pass)");
        assert_eq!(Item::Phase(3).meta().echo, Some(3));
        assert_eq!(Item::CoilMagnitude(2, 3).meta().coil, Some(2));
    }

    /// The units are the ones `qsmxt run` writes: the per-structure table's for every map it
    /// summarises, Hz for the published total field (its sidecar's `Units`), radians for phase on
    /// the working grid.
    #[test]
    fn units_match_what_qsmxt_writes() {
        for (item, name, unit) in crate::stages::analysis::MAPS {
            assert_eq!(item.meta().units, Some(unit), "{item:?}");
            assert_eq!(item.meta().bids_name.as_deref(), Some(name), "{item:?}");
        }
        assert_eq!(Item::TotalFieldHz.meta().units, Some("Hz"));
        assert_eq!(Item::FieldPpm.meta().units, Some("ppm"));
        assert_eq!(Item::Phase(1).meta().units, Some("rad"));
        assert_eq!(Item::Chi.meta().kind, ItemKind::Chi);
        assert_eq!(Item::BrainMask.meta().kind, ItemKind::Mask);
        for item in Item::one_of_each() {
            let m = item.meta();
            if m.kind == ItemKind::Mask || m.kind == ItemKind::Segmentation {
                assert_eq!(m.units, None, "{item:?}");
            }
        }
    }

    /// What `qsmxt run` publishes, and only that, is marked published.
    #[test]
    fn published_outputs_are_marked() {
        assert!(Item::Chi.meta().published && Item::TotalFieldHz.meta().published);
        assert!(Item::CombinedEchoPhase(4).meta().published);
        assert!(Item::RegionStats.meta().published);
        assert!(!Item::RawChi(Pass::Main).meta().published && !Item::FieldPpm.meta().published);
        assert!(!Item::Phase(1).meta().published && !Item::AcquiredPhase(1).meta().published);
    }
}
