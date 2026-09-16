//! Turn a downloaded QSM-CI `inputs/` archive into a BIDS `sub-01/ses-*/anat/` tree.
//!
//! The archive holds 4D magnitude and phase (echoes already sorted by ascending TE,
//! phase already in radians) plus a `params.json` of recovered acquisition parameters.
//! BIDS wants one 3D file per echo with a JSON sidecar, so this splits each 4D volume
//! and writes the sidecars from `params.json`.
//!
//! Every acquisition in the registry is the *same* subject, so they all materialize as
//! `sub-01` and are told apart by their `ses-`(scanner) / `acq-`(protocol) / `run-`
//! entities. That is what makes several of them shareable in one dataset.

use std::fs;
use std::io::Read;
use std::path::Path;

use serde::Deserialize;

use super::{Example, DATASET_URL};
use crate::error::QsmxtError;

const BIDS_VERSION: &str = "1.10.0";

/// Gyromagnetic ratio of the proton, MHz/T — relates `f0` to the field strength recorded
/// in `params.json`, and lets a reader check one against the other.
const GAMMA_MHZ_PER_T: f64 = 42.576384;

/// Acquisition parameters as packed by QSM-CI's `pack_harmonization.py`.
#[derive(Debug, Deserialize)]
struct Params {
    /// Echo times in seconds, ascending — one per volume of the 4D files.
    #[serde(rename = "TE")]
    te: Vec<f64>,
    /// Field strength in tesla, derived from the scanner's carrier frequency. This is
    /// the physically accurate value (~2.89 T on a nominal 3 T system) and the one QSM
    /// scaling needs, so it is what lands in `MagneticFieldStrength`.
    #[serde(rename = "B0")]
    b0: f64,
    #[serde(rename = "B0_dir")]
    b0_dir: Vec<f64>,
    #[serde(rename = "TR")]
    tr: Option<f64>,
    #[serde(rename = "flip_angle")]
    flip_angle: Option<f64>,
    #[serde(rename = "f0_MHz")]
    f0_mhz: Option<f64>,
    scanner: Option<String>,
    sequence: Option<String>,
}

/// What [`materialize`] did for one acquisition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Files were written; carries the number of echoes.
    Written(usize),
    /// The acquisition was already present and `force` was not set.
    Skipped,
}

/// Write `example`'s BIDS tree into `bids_dir`, creating the dataset if it does not
/// exist and adding to it if it does.
///
/// `on_step` receives short human-readable progress lines. Returns [`Outcome::Skipped`]
/// without touching anything if this acquisition is already in the dataset and `force`
/// is false.
pub fn materialize(
    example: &Example,
    zip_path: &Path,
    bids_dir: &Path,
    force: bool,
    on_step: &mut dyn FnMut(String),
) -> crate::Result<Outcome> {
    let anat_dir = bids_dir
        .join("sub-01")
        .join(format!("ses-{}", example.scanner))
        .join("anat");

    // One representative output decides whether this acquisition is already here.
    let sentinel = anat_dir.join(format!("{}_echo-1_part-phase_MEGRE.nii.gz", prefix(example)));
    if sentinel.exists() && !force {
        return Ok(Outcome::Skipped);
    }

    let file = fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| QsmxtError::Example(format!("{}: not a readable zip: {e}", zip_path.display())))?;

    let params: Params = serde_json::from_slice(&read_entry(&mut archive, "params.json")?)
        .map_err(|e| QsmxtError::Example(format!("{}: bad params.json: {e}", zip_path.display())))?;

    if params.te.is_empty() {
        return Err(QsmxtError::Example(format!(
            "{}: params.json lists no echo times",
            zip_path.display()
        )));
    }

    fs::create_dir_all(&anat_dir)?;
    write_dataset_files(bids_dir)?;

    let mut echoes = 0usize;
    for (entry, part) in [("magnitude.nii.gz", "mag"), ("phase.nii.gz", "phase")] {
        on_step(format!("extracting {} {}", example.id, part));
        let bytes = read_entry(&mut archive, entry)?;

        let (data, (nx, ny, nz, nt), voxel_size, affine) =
            qsm_core::io::load_nifti_4d(&bytes).map_err(QsmxtError::NiftiIo)?;
        drop(bytes);

        if nt != params.te.len() {
            return Err(QsmxtError::Example(format!(
                "{}: {entry} has {nt} volumes but params.json lists {} echo times",
                example.id,
                params.te.len()
            )));
        }

        let vol = nx * ny * nz;
        for echo in 0..nt {
            let name = format!("{}_echo-{}_part-{}_MEGRE", prefix(example), echo + 1, part);
            let nii = anat_dir.join(format!("{name}.nii.gz"));
            qsm_core::io::save_nifti_to_file(
                &nii,
                &data[echo * vol..(echo + 1) * vol],
                (nx, ny, nz),
                voxel_size,
                &affine,
            )
            .map_err(QsmxtError::NiftiIo)?;
            write_sidecar(&anat_dir.join(format!("{name}.json")), example, &params, echo, part)?;
        }
        echoes = nt;
        on_step(format!("wrote {nt} {part} echoes"));
    }

    Ok(Outcome::Written(echoes))
}

/// The BIDS filename prefix for an acquisition, up to (not including) the `echo-` entity.
fn prefix(example: &Example) -> String {
    format!(
        "sub-01_ses-{}_acq-{}_run-{}",
        example.scanner, example.acq, example.run
    )
}

/// Read one archive entry by its trailing filename (the archive nests them under
/// `inputs/`, but matching on the leaf keeps this robust to a repack).
fn read_entry<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    filename: &str,
) -> crate::Result<Vec<u8>> {
    let name = archive
        .file_names()
        .find(|n| n.rsplit('/').next() == Some(filename))
        .map(str::to_string)
        .ok_or_else(|| QsmxtError::Example(format!("archive has no '{filename}'")))?;

    let mut entry = archive
        .by_name(&name)
        .map_err(|e| QsmxtError::Example(format!("reading '{filename}': {e}")))?;
    let mut buf = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut buf)?;
    Ok(buf)
}

/// Write the JSON sidecar for one echo.
fn write_sidecar(
    path: &Path,
    example: &Example,
    params: &Params,
    echo: usize,
    part: &str,
) -> crate::Result<()> {
    let mut map = serde_json::Map::new();
    map.insert("EchoTime".into(), params.te[echo].into());
    map.insert("EchoNumber".into(), (echo as u64 + 1).into());
    // The accurate field strength, not the nominal 3 T: QSM scales ppm by this, and
    // ImagingFrequency below is what it was derived from.
    map.insert("MagneticFieldStrength".into(), params.b0.into());
    if let Some(f0) = params.f0_mhz {
        map.insert("ImagingFrequency".into(), f0.into());
    }
    if let Some(tr) = params.tr {
        map.insert("RepetitionTime".into(), tr.into());
    }
    if let Some(fa) = params.flip_angle {
        map.insert("FlipAngle".into(), fa.into());
    }
    map.insert("Manufacturer".into(), "Siemens".into());
    if let Some(scanner) = &params.scanner {
        map.insert("ManufacturersModelName".into(), scanner.clone().into());
    }
    if let Some(sequence) = &params.sequence {
        map.insert("PulseSequenceDetails".into(), sequence.clone().into());
    }
    if !params.b0_dir.is_empty() {
        map.insert(
            "B0_dir".into(),
            serde_json::Value::Array(params.b0_dir.iter().map(|&v| v.into()).collect()),
        );
    }
    if part == "phase" {
        map.insert("Units".into(), "rad".into());
    }
    map.insert(
        "Sources".into(),
        serde_json::Value::Array(vec![format!("{DATASET_URL} ({})", example.id).into()]),
    );

    let json = serde_json::to_string_pretty(&map)
        .map_err(|e| QsmxtError::Example(format!("serializing sidecar: {e}")))?;
    fs::write(path, json + "\n")?;
    Ok(())
}

/// Write `dataset_description.json` and `README` at the dataset root, leaving any
/// existing copy alone so that adding an acquisition never clobbers an edited dataset.
fn write_dataset_files(bids_dir: &Path) -> crate::Result<()> {
    let desc = bids_dir.join("dataset_description.json");
    if !desc.exists() {
        let value = serde_json::json!({
            "Name": "QSMxT example dataset (QSM harmonization, MGH bays 4/5, 2026-08-20)",
            "BIDSVersion": BIDS_VERSION,
            "DatasetType": "raw",
            "ReferencesAndLinks": [DATASET_URL],
        });
        fs::write(&desc, serde_json::to_string_pretty(&value).unwrap_or_default() + "\n")?;
    }

    let readme = bids_dir.join("README");
    if !readme.exists() {
        fs::write(&readme, readme_text())?;
    }
    Ok(())
}

fn readme_text() -> String {
    format!(
        "# QSMxT example dataset

One in-vivo subject from the 2026-08-20 QSM harmonization acquisition (MGH bays 4/5),
fetched with `qsmxt example` from the public OSF project at {DATASET_URL}.

The subject was scanned on two Siemens 3T scanners under four protocols, three runs
each. All of it is the same person, so everything here is `sub-01`; acquisitions are
distinguished by BIDS entities:

  ses-prisma / ses-cima            scanner (MAGNETOM Prisma Fit / MAGNETOM Cima.X)
  acq-bridge                       product GRE, GRAPPA R=2
  acq-local                        site-local protocol
  acq-pulseqonline                 Pulseq consensus sequence, scanner (ICE) recon
  acq-pulseqoffline                Pulseq consensus sequence, offline GRAPPA recon
  run-1 / run-2 / run-3            repeat

Only the acquisitions you asked for are present; re-run `qsmxt example` with another
`--name` pointed at this directory to add more.

## Notes on the data

Magnitude and phase are multi-echo 3D GRE (`MEGRE`), one 3D file per echo. Phase is in
radians and wrapped, as the pipeline expects.

`MagneticFieldStrength` is the physically accurate field derived from the scanner's
carrier frequency (`ImagingFrequency` / {GAMMA_MHZ_PER_T} MHz/T), not the nominal 3.0 T
printed on the magnet — roughly 2.89 T. QSM scales ppm by this value, so the accurate
one is the correct one to use.

Echo times were recovered from the exam-card PDFs shipped with the acquisition and,
for the Pulseq consensus sequence, from https://github.com/HarmonizedMRI/megre_label.
They are not taken from the DICOM sidecars, which carry placeholder timings for the
Pulseq protocols.

## Running it

    qsmxt run <this directory> <output directory>
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::example::find;

    #[test]
    fn prefix_orders_bids_entities_correctly() {
        let e = find("cima-pulseq-online-run3").unwrap();
        assert_eq!(prefix(e), "sub-01_ses-cima_acq-pulseqonline_run-3");
    }

    #[test]
    fn prefix_is_unique_per_example() {
        let mut seen = std::collections::HashSet::new();
        for e in crate::example::EXAMPLES {
            assert!(seen.insert(prefix(e)), "duplicate prefix for {}", e.id);
        }
    }

    #[test]
    fn generated_names_parse_as_bids_entities() {
        let e = find("prisma-bridge-run1").unwrap();
        let name = format!("{}_echo-2_part-phase_MEGRE.nii.gz", prefix(e));
        let parsed = crate::bids::entities::parse_entities(&name).expect("parses");
        assert_eq!(parsed.subject, "01");
        assert_eq!(parsed.session.as_deref(), Some("prisma"));
        assert_eq!(parsed.acquisition.as_deref(), Some("bridge"));
        assert_eq!(parsed.run.as_deref(), Some("1"));
        assert_eq!(parsed.echo, Some(2));
        assert_eq!(parsed.suffix, "MEGRE");
    }

    fn test_params() -> Params {
        Params {
            te: vec![0.005, 0.011],
            b0: 2.8946,
            b0_dir: vec![0.0, 0.0, 1.0],
            tr: Some(0.035),
            flip_angle: Some(15.0),
            f0_mhz: Some(123.243444),
            scanner: Some("MAGNETOM Prisma Fit (XR VA30A)".into()),
            sequence: Some("gre_bridge_1mm_psn_adapt".into()),
        }
    }

    #[test]
    fn sidecar_carries_the_fields_discovery_requires() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.json");
        write_sidecar(&path, find("prisma-bridge-run1").unwrap(), &test_params(), 1, "phase")
            .unwrap();

        // The real reader must accept it — that is the contract that matters.
        let parsed = crate::bids::sidecar::read_sidecar(&path).unwrap();
        assert_eq!(parsed.echo_time, 0.011);
        assert_eq!(parsed.magnetic_field_strength, 2.8946);
        assert_eq!(parsed.b0_dir, Some(vec![0.0, 0.0, 1.0]));

        let raw: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(raw["EchoNumber"], 2);
        assert_eq!(raw["Units"], "rad");
        assert_eq!(raw["ImagingFrequency"], 123.243444);
        assert_eq!(raw["Manufacturer"], "Siemens");
    }

    #[test]
    fn magnitude_sidecar_has_no_phase_units() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.json");
        write_sidecar(&path, find("prisma-bridge-run1").unwrap(), &test_params(), 0, "mag").unwrap();
        let raw: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(raw.get("Units").is_none());
    }

    #[test]
    fn dataset_files_are_written_once_and_never_clobbered() {
        let dir = tempfile::tempdir().unwrap();
        write_dataset_files(dir.path()).unwrap();
        let desc = dir.path().join("dataset_description.json");
        assert!(desc.exists());
        assert!(dir.path().join("README").exists());

        fs::write(&desc, "{\"Name\": \"edited by hand\"}").unwrap();
        write_dataset_files(dir.path()).unwrap();
        assert_eq!(fs::read_to_string(&desc).unwrap(), "{\"Name\": \"edited by hand\"}");
    }

    #[test]
    fn declared_field_strength_matches_the_carrier_frequency() {
        // Guards the claim the README makes about how B0 was derived.
        let p = test_params();
        let derived = p.f0_mhz.unwrap() / GAMMA_MHZ_PER_T;
        assert!((derived - p.b0).abs() < 1e-4, "{derived} vs {}", p.b0);
    }
}
