//! What `qsmxt run` publishes for each output, without file I/O: where the file goes under the
//! derivatives directory, and what its JSON sidecar says.
//!
//! `qsmxt run` names its published files and writes their sidecars with these functions, and a
//! front end that saves the same outputs (qsmbly's downloads) calls them too, so the two cannot
//! name or describe an output differently.
//!
//! A sidecar is built in two parts. While the run goes, the steps record what they found about an
//! output (the reference a χ map was referenced to, the units of the total field), which `qsmxt
//! run` keeps in the sidecar file and a front end keeps beside the volume ([`Store::annotate`]).
//! Once the run is over, [`sidecar`] adds what describes the output as a whole: whether it is
//! skull-stripped, the acquisition's field strength (and echo time, when it still applies), the
//! step that made it, and the software versions.
//!
//! [`Store::annotate`]: crate::store::Store::annotate

use serde_json::{Map, Value};

use crate::bids::entities::AcquisitionKey;
use crate::bids::naming::{self, Tail};
use crate::store::{Contrast, Item};

/// The tail of an item's published file and its extension, for an item `qsmxt run` publishes;
/// `None` for anything it keeps in a workflow directory, or not as a file. `scan_suffix` is the
/// run's own suffix (`MEGRE`), which the combined echoes of a per-coil run keep.
pub fn published_tail(item: &Item, scan_suffix: &str) -> Option<(Tail, &'static str)> {
    let nii = |t: Tail| Some((t, "nii"));
    match *item {
        Item::Chi => nii(naming::chimap()),
        Item::EchoChi(e) => nii(naming::chimap_desc(&format!("echo{e}"))),
        Item::ChiSupport => nii(naming::mask_desc("qsm")),
        Item::SinglePassChi => nii(naming::chimap_desc("singlepass")),
        Item::BrainMask => nii(naming::mask()),
        Item::ReliableMask => nii(naming::mask_desc("reliable")),
        Item::CoverageMask => nii(naming::mask_desc("motioncoverage")),
        Item::CombinedMagnitude => nii(naming::magnitude()),
        Item::TotalFieldHz => nii(naming::total_fieldmap()),
        Item::TotalFieldMagnitude => nii(naming::total_fieldmap_magnitude()),
        Item::Swi => nii(naming::swi()),
        Item::SwiMip => nii(naming::swi_minip()),
        Item::T2star => nii(naming::t2starmap()),
        Item::R2star => nii(naming::r2starmap()),
        Item::R2 => nii(naming::r2map()),
        Item::R2prime => nii(naming::r2primemap()),
        Item::R2primeCoverage => nii(naming::mask_desc("r2prime")),
        Item::SeparatedTotalChi => nii(naming::chi_total()),
        Item::SeparatedChi(Contrast::Paramagnetic) => nii(naming::chi_paramagnetic()),
        Item::SeparatedChi(Contrast::Diamagnetic) => nii(naming::chi_diamagnetic()),
        Item::Segmentation => nii(naming::dseg()),
        Item::SegmentationLabels => Some((naming::dseg(), "tsv")),
        Item::RegionStats => Some((Tail::new("stats").with("desc", "segmentation"), "tsv")),
        Item::Smwi(c) => nii(naming::smwi(c.name())),
        Item::SmwiMip(c) => nii(naming::smwi_minip(c.name())),
        Item::CombinedEchoPhase(e) => nii(naming::combined_echo(e, "phase", scan_suffix)),
        Item::CombinedEchoMagnitude(e) => nii(naming::combined_echo(e, "mag", scan_suffix)),
        _ => None,
    }
}

/// A run's directory of one BIDS datatype (`anat`, `fmap`) under the derivatives directory:
/// `sub-01/ses-pre/anat`.
pub fn datatype_dir(key: &AcquisitionKey, datatype: &str) -> String {
    match &key.session {
        Some(ses) => format!("sub-{}/ses-{ses}/{datatype}", key.subject),
        None => format!("sub-{}/{datatype}", key.subject),
    }
}

/// Where `qsmxt run` publishes `item` of the run `key`, relative to the derivatives directory
/// (`sub-01/anat/sub-01_Chimap.nii`), with `space` the label of a run left on its working grid
/// (`--output-space working`). `None` for an item it does not publish.
///
/// The total field and its magnitude go to `fmap/`, under the run's name with its `rec-` and
/// `inv-` folded into `acq-` ([`naming::fmap_key`]); everything else to `anat/`. The per-structure
/// statistics are one table per run, whatever its space.
pub fn published_path(item: &Item, key: &AcquisitionKey, space: Option<&str>) -> Option<String> {
    let (tail, ext) = published_tail(item, &key.suffix)?;
    if *item == Item::RegionStats {
        return Some(format!("{}/{}.{ext}", datatype_dir(key, "anat"), naming::compose(&key.basename(), &tail)));
    }
    let tail = tail.in_space(space);
    if naming::FMAP_SUFFIXES.contains(&tail.suffix()) {
        let fmap = naming::fmap_key(key);
        return Some(format!("{}/{}.{ext}", datatype_dir(key, "fmap"), naming::compose(&fmap.basename(), &tail)));
    }
    Some(format!("{}/{}.{ext}", datatype_dir(key, "anat"), naming::compose(&key.basename(), &tail)))
}

/// The sidecar's name beside a NIfTI file's (`…_Chimap.nii` or `.nii.gz` → `…_Chimap.json`);
/// `None` for a file that is not NIfTI.
pub fn sidecar_name(nifti: &str) -> Option<String> {
    let stem = nifti.strip_suffix(".nii.gz").or_else(|| nifti.strip_suffix(".nii"))?;
    Some(format!("{stem}.json"))
}

/// Which acquisition metadata is still true of an output, and goes into its sidecar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Forward {
    /// Nothing: a mask or label image.
    Nothing,
    /// `MagneticFieldStrength` only: a map fitted across echoes, or from another acquisition.
    FieldStrength,
    /// `MagneticFieldStrength`, and `EchoTime` when the run had a single echo.
    FieldAndEcho,
}

/// How a published image is described in its sidecar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Description {
    pub forward: Forward,
    /// `SkullStripped`, or `None` where the key does not apply (masks and label images, per the
    /// BIDS derivatives schema).
    pub skull_stripped: Option<bool>,
}

/// A mask or label image: nothing forwarded, no `SkullStripped`.
pub const MASK: Description = Description { forward: Forward::Nothing, skull_stripped: None };

/// A multi-orientation group's outputs (COSMOS, STI): fitted across orientations, inside the brain.
pub const MULTI_ORIENTATION: Description = Description { forward: Forward::FieldStrength, skull_stripped: Some(true) };

/// How the sidecar of a published image describes it; `None` for an item that is not a published
/// image.
///
/// `SkullStripped` (required for derivative anat images except `mask`) says whether the output is
/// confined to the brain mask:
/// - the χ map is referenced within the mask, and T2*/R2* are masked explicitly, so they are
///   skull-stripped; SMWI is the magnitude weighted by a χ-derived mask, so it is confined the way
///   the χ map is;
/// - the combined magnitude (T2starw) and SWI/minIP keep the full field of view.
pub fn description(item: &Item) -> Option<Description> {
    use Forward::*;
    let image = |forward, ss| Some(Description { forward, skull_stripped: Some(ss) });
    match *item {
        // The combined echoes of a per-coil run: one echo each, full field of view.
        Item::CombinedEchoPhase(_) | Item::CombinedEchoMagnitude(_) => image(FieldStrength, false),
        Item::Chi | Item::SinglePassChi => image(FieldAndEcho, true),
        // One echo's map: its echo time is recorded by the step that made it.
        Item::EchoChi(_) => image(FieldStrength, true),
        Item::BrainMask | Item::ChiSupport | Item::ReliableMask | Item::Segmentation
        | Item::R2primeCoverage | Item::CoverageMask => Some(MASK),
        Item::CombinedMagnitude | Item::Swi | Item::SwiMip => image(FieldAndEcho, false),
        Item::Smwi(_) | Item::SmwiMip(_) => image(FieldAndEcho, true),
        // R2 comes from the multi-echo spin echo, so the GRE's echo time says nothing about it.
        Item::T2star | Item::R2star | Item::R2 | Item::R2prime => image(FieldStrength, true),
        Item::SeparatedChi(_) | Item::SeparatedTotalChi => image(FieldAndEcho, true),
        // The field map is computed inside the brain mask and is 0 outside it; its magnitude is
        // the first echo as acquired.
        Item::TotalFieldHz => image(FieldStrength, true),
        Item::TotalFieldMagnitude => image(FieldStrength, false),
        _ => None,
    }
}

/// What the step that wrote an output found about it, beyond the volume, which its sidecar
/// records: for the total field, its units and what it is (the BIDS `fieldmap` takes Hz, not
/// ppm) and, for its magnitude, the echo it is.
pub fn step_fields(item: &Item, first_echo_time: f64) -> Option<Map<String, Value>> {
    let fields = match *item {
        Item::TotalFieldHz => vec![
            ("Units", Value::from("Hz")),
            ("Description", Value::from(
                "Total (unwrapped, echo-combined) field: the B0 field offset from the scanner frequency, \
                 before background-field removal")),
        ],
        Item::TotalFieldMagnitude => vec![
            ("Description", Value::from(
                "Magnitude of the first echo, the anatomical reference for desc-total_fieldmap")),
            ("EchoTime", serde_json::json!(first_echo_time)),
        ],
        _ => return None,
    };
    Some(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// The BIDS-Prov activity of `step` on the run named `basename` (`bids::prov/#reference-sub-01`):
/// what a sidecar's `GeneratedBy` names.
pub fn activity_iri(step: &str, basename: &str) -> String {
    format!("bids::prov/#{step}-{basename}")
}

/// What describes a run's outputs as a whole.
#[derive(Debug, Clone, PartialEq)]
pub struct SidecarFacts<'a> {
    /// The acquisition's field strength (T).
    pub field_strength: f64,
    /// The acquisition's echo times (s).
    pub echo_times: &'a [f64],
    /// The QSMxT and QSM.rs versions that made the outputs.
    pub qsmxt_version: &'a str,
    pub qsm_rs_version: &'a str,
}

/// Add to `fields` (what is recorded about an output so far) what describes it as a whole:
/// `SkullStripped`, `MagneticFieldStrength`, `EchoTime` where `description` says they apply,
/// `GeneratedBy` (the activity that last wrote it, [`activity_iri`]) and the software versions.
pub fn describe(fields: &mut Map<String, Value>, description: Description, generated_by: Option<&str>, facts: &SidecarFacts) {
    if let Some(ss) = description.skull_stripped {
        fields.insert("SkullStripped".to_string(), Value::Bool(ss));
    }
    if description.forward != Forward::Nothing {
        fields.insert("MagneticFieldStrength".to_string(), serde_json::json!(facts.field_strength));
    }
    // EchoTime remains valid only when a single echo was used.
    if description.forward == Forward::FieldAndEcho && facts.echo_times.len() == 1 {
        fields.insert("EchoTime".to_string(), serde_json::json!(facts.echo_times[0]));
    }
    if let Some(activity) = generated_by {
        fields.insert("GeneratedBy".to_string(), Value::String(activity.to_string()));
    }
    fields.insert("QSMxTVersion".to_string(), Value::String(facts.qsmxt_version.to_string()));
    fields.insert("QSMrsVersion".to_string(), Value::String(facts.qsm_rs_version.to_string()));
}

/// The sidecar of a published image: what the steps recorded about it (`recorded`, in the order
/// they recorded it), then what describes it ([`describe`]). `generated_by` is the activity of the
/// step that last wrote it. `None` for an item that is not a published image.
pub fn sidecar(item: &Item, recorded: Map<String, Value>, generated_by: Option<&str>, facts: &SidecarFacts) -> Option<Map<String, Value>> {
    let description = description(item)?;
    let mut fields = recorded;
    describe(&mut fields, description, generated_by, facts);
    Some(fields)
}

/// The sidecar as `qsmxt run` writes it: pretty JSON, keys in the order they were added.
pub fn sidecar_json(fields: &Map<String, Value>) -> String {
    serde_json::to_string_pretty(fields).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::Pass;

    fn key() -> AcquisitionKey {
        AcquisitionKey {
            subject: "01".into(), session: Some("pre".into()), acquisition: Some("gre".into()),
            reconstruction: None, inversion: None, run: Some("1".into()), suffix: "MEGRE".into(),
        }
    }

    /// Each echo's map of a per-echo QSM is published beside the QSM as `desc-echo<n>` (BIDS gives a
    /// `Chimap` no `echo-` entity); its echo time is recorded by the step that made it.
    #[test]
    fn an_echos_map_is_published_with_its_echo() {
        assert_eq!(published_path(&Item::EchoChi(2), &key(), None).unwrap(),
                   "sub-01/ses-pre/anat/sub-01_ses-pre_acq-gre_run-1_desc-echo2_Chimap.nii");
        assert_eq!(description(&Item::EchoChi(2)).unwrap().forward, Forward::FieldStrength);
        assert_eq!(published_path(&Item::EchoRawChi(2), &key(), None), None);
    }

    /// Every item the engine calls published has a published file, under the BIDS name the engine
    /// gives it, and nothing else has one.
    #[test]
    fn published_items_and_only_they_have_a_published_file() {
        for item in Item::one_of_each() {
            let meta = item.meta();
            let path = published_path(&item, &key(), None);
            assert_eq!(path.is_some(), meta.published, "{item:?}: {path:?}");
            let Some(path) = path else { continue };
            if let Some(bids) = meta.bids_name {
                let name = path.rsplit('/').next().unwrap();
                assert!(name.split('.').next().unwrap().ends_with(&format!("_{bids}")), "{item:?}: {name} is not {bids}");
            }
            // An image has a description for its sidecar; a table has none.
            assert_eq!(description(&item).is_some(), path.ends_with(".nii"), "{item:?}");
        }
    }

    #[test]
    fn names_follow_the_bids_layout() {
        let k = key();
        assert_eq!(published_path(&Item::Chi, &k, None).unwrap(), "sub-01/ses-pre/anat/sub-01_ses-pre_acq-gre_run-1_Chimap.nii");
        assert_eq!(published_path(&Item::Chi, &k, Some("axial")).unwrap(),
                   "sub-01/ses-pre/anat/sub-01_ses-pre_acq-gre_run-1_space-axial_Chimap.nii");
        assert_eq!(published_path(&Item::CombinedMagnitude, &k, None).unwrap(),
                   "sub-01/ses-pre/anat/sub-01_ses-pre_acq-gre_run-1_part-mag_T2starw.nii");
        assert_eq!(published_path(&Item::RegionStats, &k, Some("axial")).unwrap(),
                   "sub-01/ses-pre/anat/sub-01_ses-pre_acq-gre_run-1_desc-segmentation_stats.tsv");
        assert_eq!(published_path(&Item::SegmentationLabels, &k, None).unwrap(),
                   "sub-01/ses-pre/anat/sub-01_ses-pre_acq-gre_run-1_dseg.tsv");
        // The field map goes to fmap/, its run's rec- folded into acq-.
        let rec = AcquisitionKey { reconstruction: Some("uncombined".into()), session: None, ..k.clone() };
        assert_eq!(published_path(&Item::TotalFieldHz, &rec, None).unwrap(),
                   "sub-01/fmap/sub-01_acq-grerecuncombined_run-1_desc-total_fieldmap.nii");
        assert_eq!(published_path(&Item::CombinedEchoPhase(2), &rec, None).unwrap(),
                   "sub-01/anat/sub-01_acq-gre_rec-mcpc3ds_run-1_echo-2_part-phase_MEGRE.nii");
        assert_eq!(published_path(&Item::RawChi(Pass::Main), &k, None), None);
        assert_eq!(sidecar_name("a/sub-01_Chimap.nii").as_deref(), Some("a/sub-01_Chimap.json"));
        assert_eq!(sidecar_name("sub-01_Chimap.nii.gz").as_deref(), Some("sub-01_Chimap.json"));
        assert_eq!(sidecar_name("sub-01_dseg.tsv"), None);
    }

    #[test]
    fn a_sidecar_keeps_what_the_steps_recorded_then_describes_the_output() {
        let facts = SidecarFacts { field_strength: 3.0, echo_times: &[0.02], qsmxt_version: "10.1.0", qsm_rs_version: "v0.40.0" };
        let mut recorded = Map::new();
        recorded.insert("QsmReference".into(), "mean".into());
        let s = sidecar(&Item::Chi, recorded, Some(&activity_iri("reference", "sub-01")), &facts).unwrap();
        assert_eq!(sidecar_json(&s), r#"{
  "QsmReference": "mean",
  "SkullStripped": true,
  "MagneticFieldStrength": 3.0,
  "EchoTime": 0.02,
  "GeneratedBy": "bids::prov/#reference-sub-01",
  "QSMxTVersion": "10.1.0",
  "QSMrsVersion": "v0.40.0"
}"#);
        // A mask: no acquisition metadata; several echoes: no EchoTime.
        let multi = SidecarFacts { echo_times: &[0.01, 0.02], ..facts.clone() };
        let mask = sidecar(&Item::BrainMask, Map::new(), None, &multi).unwrap();
        assert_eq!(mask.keys().collect::<Vec<_>>(), ["QSMxTVersion", "QSMrsVersion"]);
        let chi = sidecar(&Item::Chi, Map::new(), None, &multi).unwrap();
        assert!(!chi.contains_key("EchoTime") && chi.contains_key("MagneticFieldStrength"));
        assert!(sidecar(&Item::RegionStats, Map::new(), None, &facts).is_none());
        // A field already recorded keeps its place.
        let mut again = s.clone();
        describe(&mut again, description(&Item::Chi).unwrap(), Some("bids::prov/#output_space-sub-01"), &facts);
        assert_eq!(again.keys().collect::<Vec<_>>(), s.keys().collect::<Vec<_>>());
    }

    #[test]
    fn the_total_field_says_its_units() {
        assert_eq!(step_fields(&Item::TotalFieldHz, 0.004).unwrap()["Units"], "Hz");
        assert_eq!(step_fields(&Item::TotalFieldMagnitude, 0.004).unwrap()["EchoTime"], 0.004);
        assert!(step_fields(&Item::Chi, 0.004).is_none());
    }
}
