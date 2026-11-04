//! Scanning and converting the synthetic fixtures ([`super::synth`]): what each export pattern is
//! classified as, the names it gets, and what the scan warns about. Conversion needs dcm2niix;
//! without it those checks are skipped (CI installs it).

use std::path::Path;
use std::sync::atomic::AtomicUsize;
use std::sync::{mpsc, Arc};

use super::convert::{self, ConvertMessage};
use super::synth::{self, write_fixture};
use super::*;

const PREFIX: &str = "sub-synth01_ses-20240101";

fn scan(name: &str) -> (tempfile::TempDir, DicomSession) {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(name, dir.path());
    let session = scan_dicom_directory(dir.path(), Arc::new(AtomicUsize::new(0))).unwrap();
    (dir, session)
}

/// Every file under `root`, relative, sorted.
fn tree(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                out.push(p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    walk(root, root, &mut out);
    out.sort();
    out
}

struct Converted {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    /// The images written (`.nii.gz`), relative to the dataset root.
    images: Vec<String>,
    errors: Vec<String>,
    log: Vec<String>,
}

/// Convert a session, if dcm2niix is available.
fn convert_to(session: &DicomSession, out: &Path) -> Option<(Vec<String>, Vec<String>)> {
    if convert::find_dcm2niix().is_none() {
        eprintln!("dcm2niix not found: skipping the conversion checks");
        return None;
    }
    let (tx, rx) = mpsc::channel();
    convert::convert_session_streaming(session, out, &tx);
    drop(tx);
    let (mut errors, mut log) = (Vec::new(), Vec::new());
    for m in rx {
        match m {
            ConvertMessage::Error(e) => errors.push(e),
            ConvertMessage::Log(l) => log.push(l),
            ConvertMessage::Done { .. } => {}
        }
    }
    Some((errors, log))
}

fn convert(session: &DicomSession) -> Option<Converted> {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bids");
    let (errors, log) = convert_to(session, &root)?;
    let images = tree(&root).into_iter().filter(|f| f.ends_with(".nii.gz")).collect();
    Some(Converted { _dir: dir, root, images, errors, log })
}

fn anat(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| format!("sub-synth01/ses-20240101/anat/{PREFIX}_{n}.nii.gz")).collect()
}

/// `(description, type)` of every series.
fn types(s: &DicomSession) -> Vec<(String, SeriesType)> {
    s.flat_series().iter().map(|r| {
        let x = s.series_ref(r);
        (x.description.clone(), x.series_type)
    }).collect()
}

fn assert_images(c: &Converted, want: &[String]) {
    let got: Vec<&String> = c.images.iter().filter(|f| f.contains("/anat/")).collect();
    let want: Vec<&String> = want.iter().collect();
    assert_eq!(got, want, "errors: {:?}\nlog:\n{}", c.errors, c.log.join("\n"));
    assert!(c.errors.is_empty(), "{:?}", c.errors);
}

#[test]
fn swi_qsm_and_magnitude() {
    let (_d, s) = scan("swi_qsm_and_magnitude");
    assert_eq!(types(&s).iter().map(|t| t.1).collect::<Vec<_>>(), vec![
        SeriesType::Magnitude, SeriesType::Phase, SeriesType::ScannerQsm, SeriesType::MinIp, SeriesType::Swi,
    ]);
    let Some(c) = convert(&s) else { return };
    assert_images(&c, &anat(&[
        "acq-t2starSWIandQSM_part-mag_T2starw",
        "acq-t2starSWIandQSM_part-phase_T2starw",
        "acq-t2starSWIandQSM_rec-mip_T2starw",
        "acq-t2starSWIandQSM_rec-scanner_Chimap",
        "acq-t2starSWIandQSM_rec-swi_part-mag_T2starw",
    ]));
    // BIDS requires Units for phase.
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
        c.root.join(format!("sub-synth01/ses-20240101/anat/{PREFIX}_acq-t2starSWIandQSM_part-phase_T2starw.json"))).unwrap()).unwrap();
    assert_eq!(json["Units"], "arbitrary");
    assert!((json["EchoTime"].as_f64().unwrap() - 0.02).abs() < 1e-9);
    // Dataset files.
    assert!(c.root.join("dataset_description.json").is_file());
    let ignore = std::fs::read_to_string(c.root.join(".bidsignore")).unwrap();
    assert!(ignore.lines().any(|l| l == "extra_data"), "{ignore}");
}

#[test]
fn duplicate_identical() {
    let (_d, s) = scan("duplicate_identical");
    let t = types(&s);
    assert_eq!(t[2], ("Mag_Images".to_string(), SeriesType::Skip));
    assert!(s.report.notes.iter().any(|n| n.contains("has the same images as series 9")), "{:?}", s.report);
    let Some(c) = convert(&s) else { return };
    assert_images(&c, &anat(&["acq-greqsm_part-mag_T2starw", "acq-greqsm_part-phase_T2starw"]));
}

#[test]
fn duplicate_different() {
    let (_d, s) = scan("duplicate_different");
    assert_eq!(types(&s)[2], ("Mag_Images".to_string(), SeriesType::Extra));
    assert!(s.report.warnings.iter().any(|w| w.contains("both original magnitude images")), "{:?}", s.report);
    let Some(c) = convert(&s) else { return };
    assert_images(&c, &anat(&["acq-greqsm_part-mag_T2starw", "acq-greqsm_part-phase_T2starw"]));
    assert!(c.images.contains(&"sub-synth01/ses-20240101/extra_data/11_MagImages.nii.gz".to_string()), "{:?}", c.images);
    let readme = std::fs::read_to_string(c.root.join("README")).unwrap();
    assert!(readme.contains("extra_data/"), "{readme}");
}

#[test]
fn repeat_same_settings() {
    let (_d, s) = scan("repeat_same_settings");
    let Some(c) = convert(&s) else { return };
    let mut want = Vec::new();
    for run in 1..=2 {
        for e in 1..=2 {
            for part in ["mag", "phase"] {
                want.push(format!("acq-greqsm_run-{run}_echo-{e}_part-{part}_MEGRE"));
            }
        }
    }
    want.sort();
    assert_images(&c, &anat(&want.iter().map(String::as_str).collect::<Vec<_>>()));
}

#[test]
fn repeat_other_echoes() {
    let (_d, s) = scan("repeat_other_echoes");
    let names: Vec<&str> = s.subjects[0].studies[0].acquisitions.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["greqsm", "greqsm2"]);
    let Some(c) = convert(&s) else { return };
    let mut want = vec!["acq-greqsm_part-mag_T2starw".to_string(), "acq-greqsm_part-phase_T2starw".to_string()];
    for e in 1..=3 {
        for part in ["mag", "phase"] {
            want.push(format!("acq-greqsm2_echo-{e}_part-{part}_MEGRE"));
        }
    }
    want.sort();
    assert_images(&c, &anat(&want.iter().map(String::as_str).collect::<Vec<_>>()));
}

#[test]
fn clinical_series() {
    let (_d, s) = scan("clinical_series");
    let t = types(&s);
    for (d, ty) in &t {
        let want = match d.as_str() {
            "t1_mprage_sag" => SeriesType::T1w,
            "gre_qsm" => *ty,
            _ => SeriesType::Extra,
        };
        assert_eq!(*ty, want, "{d}");
    }
    let Some(c) = convert(&s) else { return };
    assert_images(&c, &anat(&["acq-greqsm_part-mag_T2starw", "acq-greqsm_part-phase_T2starw", "acq-t1mpragesag_T1w"]));
    let extra: Vec<&String> = c.images.iter().filter(|f| f.contains("/extra_data/")).collect();
    assert_eq!(extra.len(), 4, "{extra:?}");
}

#[test]
fn scanner_map_at_last_echo() {
    let (_d, s) = scan("scanner_map_at_last_echo");
    let Some(c) = convert(&s) else { return };
    let maps: Vec<&String> = c.images.iter().filter(|f| f.ends_with("R2starmap.nii.gz")).collect();
    assert_eq!(maps.len(), 1, "{:?}\n{}", c.images, c.log.join("\n"));
    assert!(maps[0].contains("_rec-scanner_R2starmap"), "{maps:?}");
    assert!(c.errors.is_empty(), "{:?}", c.errors);
}

#[test]
fn bruker_fit_of_extra() {
    let (_d, s) = scan("bruker_fit_of_extra");
    let plans = plan_session(&s);
    let target = |n: i32| plans.iter().find(|p| s.series_ref(&p.r).series_number == n).map(|p| p.target.clone()).unwrap();
    assert!(matches!(target(90002), Target::Bids(_)));
    assert!(matches!(target(90003), Target::Extra { .. }));
    assert!(matches!(target(90004), Target::Extra { .. }));
    let Some(c) = convert(&s) else { return };
    let maps: Vec<&String> = c.images.iter().filter(|f| f.contains("/anat/") && f.ends_with("T2starmap.nii.gz")).collect();
    assert_eq!(maps.len(), 1, "only the fit of the magnitude: {:?}\n{}", c.images, c.log.join("\n"));
    assert!(c.images.iter().any(|f| f.contains("/extra_data/90004")), "{:?}", c.images);
    assert!(c.log.iter().any(|l| l.contains("series 90004") && l.contains("kept there too")), "{}", c.log.join("\n"));
    assert!(c.errors.is_empty(), "{:?}", c.errors);
}

#[test]
fn bruker_te_suffix() {
    let (_d, s) = scan("bruker_te_suffix");
    assert_eq!(types(&s).iter().map(|t| t.1).collect::<Vec<_>>(), vec![SeriesType::Magnitude, SeriesType::T2starMap]);
    let Some(c) = convert(&s) else { return };
    let mag: Vec<&String> = c.images.iter().filter(|f| f.contains("part-mag")).collect();
    assert_eq!(mag.len(), 3, "every echo its own file: {:?}\n{}", c.images, c.log.join("\n"));
    for e in 1..=3 {
        assert!(c.images.iter().any(|f| f.ends_with(&format!("acq-T2starmapMGE_echo-{e}_part-mag_MEGRE.nii.gz"))), "{:?}", c.images);
    }
    assert!(c.images.iter().any(|f| f.contains("_rec-scanner_T2starmap")), "{:?}", c.images);
    assert!(c.errors.is_empty(), "{:?}", c.errors);
}

#[test]
fn headerless() {
    let (_d, s) = scan("headerless");
    assert_eq!(types(&s).iter().map(|t| t.1).collect::<Vec<_>>(), vec![SeriesType::Magnitude, SeriesType::Phase]);
    assert_eq!(s.report.not_dicom, 0);
    let Some(c) = convert(&s) else { return };
    assert_images(&c, &anat(&["acq-greqsm_part-mag_T2starw", "acq-greqsm_part-phase_T2starw"]));
}

#[test]
fn not_dicom_files_are_counted() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture("headerless", dir.path());
    std::fs::write(dir.path().join("notes.txt"), "hello").unwrap();
    std::fs::write(dir.path().join("image.nii"), [0u8; 400]).unwrap();
    let s = scan_dicom_directory(dir.path(), Arc::new(AtomicUsize::new(0))).unwrap();
    assert_eq!(s.report.not_dicom, 2);
    let empty = tempfile::tempdir().unwrap();
    std::fs::write(empty.path().join("notes.txt"), "hello").unwrap();
    let err = scan_dicom_directory(empty.path(), Arc::new(AtomicUsize::new(0))).unwrap_err();
    assert!(err.contains("No DICOM files found") && err.contains("1 files were not DICOM"), "{err}");
}

#[test]
fn incomplete_export() {
    let (_d, s) = scan("incomplete_export");
    let w = &s.report.warnings;
    assert!(w.iter().any(|w| w.contains("1 empty (0-byte) file was found")), "{w:?}");
    assert!(w.iter().any(|w| w.contains("the magnitude has 8 images but the phase has 7")), "{w:?}");
    assert!(w.iter().any(|w| w.contains("instance numbers 1–8, 1 missing")), "{w:?}");
    let Some(c) = convert(&s) else { return };
    assert!(c.log.iter().any(|l| l.starts_with("Warning: ") && l.contains("the phase has 7")), "{:?}", c.log);
}

#[test]
fn phoenix_report() {
    let (_d, s) = scan("phoenix_report");
    assert_eq!(s.total_series(), 2);
    assert_eq!(s.report.skipped, vec!["series 99 'PhoenixZIPReport' (structured report, 1 file)".to_string()]);
    let Some(c) = convert(&s) else { return };
    assert_images(&c, &anat(&["acq-greqsm_part-mag_T2starw", "acq-greqsm_part-phase_T2starw"]));
}

#[test]
fn split_patient() {
    let (_d, s) = scan("split_patient");
    assert_eq!(s.subjects.len(), 1);
    assert_eq!(s.subjects[0].patient_id, "synth01");
    assert!(s.report.warnings.iter().any(|w| w.contains("1 of its 4 files name a different patient") && w.contains("(other01)")), "{:?}", s.report.warnings);
    let Some(c) = convert(&s) else { return };
    assert_images(&c, &anat(&["acq-greqsm_part-mag_T2starw", "acq-greqsm_part-phase_T2starw"]));
}

#[test]
fn ge_private_type() {
    let (_d, s) = scan("ge_private_type");
    assert_eq!(types(&s).iter().map(|t| t.1).collect::<Vec<_>>(), vec![SeriesType::Magnitude, SeriesType::Real, SeriesType::Imaginary]);
    let Some(c) = convert(&s) else { return };
    let mut want = Vec::new();
    for e in 1..=2 {
        for part in ["imag", "mag", "real"] {
            want.push(format!("acq-SWAN_echo-{e}_part-{part}_MEGRE"));
        }
    }
    assert_images(&c, &anat(&want.iter().map(String::as_str).collect::<Vec<_>>()));
}

#[test]
fn mp2rage() {
    let (_d, s) = scan("mp2rage");
    let Some(c) = convert(&s) else { return };
    assert_images(&c, &anat(&[
        "acq-MP2RAGE0p75mm_T1map",
        "acq-MP2RAGE0p75mm_T1w",
        "acq-MP2RAGE0p75mm_inv-1_MP2RAGE",
        "acq-MP2RAGE0p75mm_inv-2_MP2RAGE",
        "acq-MP2RAGE0p75mm_rec-uniden_T1w",
    ]));
}

#[test]
fn uncombined_coils() {
    let (_d, s) = scan("uncombined_coils");
    let Some(c) = convert(&s) else { return };
    let mut want = Vec::new();
    for e in 1..=2 {
        want.push(format!("acq-SWI3mm_echo-{e}_part-mag_MEGRE"));
        want.push(format!("acq-SWI3mm_echo-{e}_part-phase_MEGRE"));
        want.push(format!("acq-SWI3mm_rec-filtered_echo-{e}_part-mag_MEGRE"));
        for coil in 1..=2 {
            for part in ["mag", "phase"] {
                want.push(format!("acq-SWI3mm_rec-uncombined_coil-{coil:02}_echo-{e}_part-{part}_MEGRE"));
            }
        }
    }
    want.sort();
    assert_images(&c, &anat(&want.iter().map(String::as_str).collect::<Vec<_>>()));
}

#[test]
fn same_echo_tags() {
    let (_d, s) = scan("same_echo_tags");
    let Some(c) = convert(&s) else { return };
    let mut want = Vec::new();
    for e in 1..=3 {
        want.push(format!("acq-fl3dvibeQSM_echo-{e}_part-mag_MEGRE"));
        want.push(format!("acq-fl3dvibeQSM_echo-{e}_part-phase_MEGRE"));
    }
    // The scanner QSM's echoes cannot be told apart by their tags: one file per volume.
    for v in 1..=3 {
        want.push(format!("acq-fl3dvibeQSM_rec-scannervol{v}_Chimap"));
    }
    want.sort();
    assert_images(&c, &anat(&want.iter().map(String::as_str).collect::<Vec<_>>()));
}

#[test]
fn philips_radian_phase() {
    let (_d, s) = scan("philips_radian_phase");
    let Some(c) = convert(&s) else { return };
    let units = |name: &str| -> String {
        let p = c.root.join(format!("sub-synth01/ses-20240101/anat/{PREFIX}_acq-WIPQSM2e_echo-1_part-{name}_MEGRE.json"));
        let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        j["Units"].as_str().unwrap_or("").to_string()
    };
    assert_eq!(units("phase"), "rad", "{}", c.log.join("\n"));
    assert_eq!(units("mag"), "");
}

#[test]
fn a_name_two_series_would_share_is_an_error_and_neither_is_written() {
    let (_d, mut s) = scan("duplicate_different");
    // Relabel the extra magnitude back to Magnitude: it now clashes with series 9.
    let r = s.flat_series().into_iter().find(|r| s.series_ref(r).description == "Mag_Images").unwrap();
    s.series_mut(&r).series_type = SeriesType::Magnitude;
    s.series_mut(&r).original = false;
    s.series_mut(&r).description = "gre_qsm".into();
    let Some(c) = convert(&s) else { return };
    // Labels keep them apart (the second gets a series-number label), so nothing collides…
    assert!(c.errors.is_empty(), "{:?}", c.errors);
    assert!(c.images.iter().any(|f| f.contains("_rec-s11_part-mag_T2starw")), "{:?}", c.images);
}

#[test]
fn convert_command_dry_run_lists_names() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture("swi_qsm_and_magnitude", dir.path());
    let s = scan_dicom_directory(dir.path(), Arc::new(AtomicUsize::new(0))).unwrap();
    let plans = plan_session(&s);
    let lines = crate::commands::dicom::classification_lines(&s, &s.unique_series(), &plans);
    assert!(lines.iter().any(|l| l.contains("[Scanner QSM]") && l.contains("rec-scanner_Chimap")), "{lines:#?}");
}

/// Writes the BIDS `qsmxt dicom-convert` makes from every fixture into
/// `$QSMXT_DICOM_FIXTURE_OUT/<fixture>/`, for CI's BIDS validator job.
#[test]
#[ignore]
fn dicom_bids_validator_fixture() {
    let root = std::path::PathBuf::from(std::env::var("QSMXT_DICOM_FIXTURE_OUT").expect("set QSMXT_DICOM_FIXTURE_OUT"));
    for name in synth::FIXTURES {
        let src = tempfile::tempdir().unwrap();
        write_fixture(name, src.path());
        let session = scan_dicom_directory(src.path(), Arc::new(AtomicUsize::new(0))).unwrap();
        let out = root.join(name);
        let _ = std::fs::remove_dir_all(&out);
        let (errors, log) = convert_to(&session, &out).expect("dcm2niix is needed");
        assert!(errors.is_empty(), "{name}: {errors:?}\n{}", log.join("\n"));
    }
}
