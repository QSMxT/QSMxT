//! Grouping, classification and naming of whole scans, from per-file metadata.

use super::*;

const MR: &str = "1.2.840.10008.5.1.4.1.1.4";

/// One file of a series. `t` is the acquisition time (s), `te` the echo time (ms).
#[allow(clippy::too_many_arguments)]
fn file(uid: &str, num: i32, desc: &str, prot: &str, it: &[&str], te: f64, t: f64, inst: i32) -> FileMeta {
    FileMeta {
        path: PathBuf::from(format!("{uid}/{inst:04}.dcm")),
        patient_id: "p1".into(),
        study_uid: "study1".into(),
        study_date: "20240101".into(),
        series_uid: uid.into(),
        series_description: desc.into(),
        protocol_name: prot.into(),
        series_number: num,
        instance_number: Some(inst),
        echo_times: vec![te],
        image_type: it.iter().map(|s| s.to_string()).collect(),
        scanning_sequence: vec!["GR".into()],
        repetition_time: Some(30.0),
        flip_angle: Some(15.0),
        rows: Some(8),
        columns: Some(8),
        slice_thickness: Some(1.0),
        pixel_spacing: vec![1.0, 1.0],
        manufacturer: "SIEMENS".into(),
        acquisition_time: Some(t),
        sop_class_uid: MR.into(),
        frames: 1,
        ..Default::default()
    }
}

/// A series of `n` slices for each echo time.
#[allow(clippy::too_many_arguments)]
fn series(uid: &str, num: i32, desc: &str, prot: &str, it: &[&str], tes: &[f64], t: f64, n: i32) -> Vec<FileMeta> {
    let mut v = Vec::new();
    for (e, &te) in tes.iter().enumerate() {
        for k in 0..n {
            v.push(file(uid, num, desc, prot, it, te, t, e as i32 * n + k + 1));
        }
    }
    v
}

const M: &[&str] = &["ORIGINAL", "PRIMARY", "M", "ND", "NORM"];
const P: &[&str] = &["ORIGINAL", "PRIMARY", "P", "ND"];

fn never_identical(_: &DicomSeries, _: &DicomSeries) -> bool {
    false
}

/// `(description, type, preview name)` of every series, in session order.
fn names(session: &DicomSession) -> Vec<(String, SeriesType, String)> {
    plan_session(session)
        .into_iter()
        .map(|p| {
            let s = session.series_ref(&p.r);
            let name = match &p.target {
                Target::Bids(stem) => stem.preview(),
                Target::Extra { dir, prefix } => format!("{}/{}", dir.join("/"), prefix),
                Target::Skip => "-".into(),
            };
            (s.description.clone(), s.series_type, name)
        })
        .collect()
}

fn name_of(session: &DicomSession, desc: &str, t: SeriesType) -> String {
    names(session).into_iter().find(|(d, ty, _)| d == desc && *ty == t).unwrap_or_else(|| panic!("no {desc} {t:?}: {:#?}", names(session))).2
}

#[test]
fn swi_qsm_and_magnitude_of_one_protocol_get_distinct_names() {
    let p = "t2star_SWIandQSM";
    let mut files = series("u9", 9, p, p, M, &[20.0], 100.0, 4);
    files.extend(series("u10", 10, p, p, P, &[20.0], 100.0, 4));
    files.extend(series("u13", 13, "t2star_SWIandQSM_QSM", p, &["ORIGINAL", "PRIMARY", "QSM", "ND"], &[20.0], 100.0, 4));
    files.extend(series("u14", 14, "mIP_Images(SW)", p, &["ORIGINAL", "PRIMARY", "MNIP", "ND", "NORM"], &[20.0], 100.0, 3));
    files.extend(series("u15", 15, "SWI_Images", p, &["ORIGINAL", "PRIMARY", "M", "SWI", "ND", "NORM"], &[20.0], 100.0, 4));
    let s = build_session(files, never_identical);
    let n = names(&s);
    let all: Vec<&str> = n.iter().map(|x| x.2.as_str()).collect();
    assert_eq!(all, vec![
        "sub-p1_ses-20240101_acq-t2starSWIandQSM_part-mag_T2starw",
        "sub-p1_ses-20240101_acq-t2starSWIandQSM_part-phase_T2starw",
        "sub-p1_ses-20240101_acq-t2starSWIandQSM_rec-scanner_Chimap",
        "sub-p1_ses-20240101_acq-t2starSWIandQSM_rec-mip_T2starw",
        "sub-p1_ses-20240101_acq-t2starSWIandQSM_rec-swi_part-mag_T2starw",
    ]);
}

#[test]
fn duplicate_exports_are_skipped_and_different_ones_kept_in_extra() {
    let p = "gre";
    let mk = || {
        let mut files = series("a", 9, p, p, M, &[20.0], 100.0, 4);
        files.extend(series("b", 10, p, p, P, &[20.0], 100.0, 4));
        files.extend(series("c", 11, "Mag_Images", p, M, &[20.0], 100.0, 4));
        files
    };
    let same = build_session(mk(), |_, _| true);
    assert_eq!(name_of(&same, "Mag_Images", SeriesType::Skip), "-");
    assert!(same.report.notes.iter().any(|n| n.contains("series 11 'Mag_Images' has the same images as series 9")), "{:?}", same.report.notes);
    assert!(same.report.warnings.is_empty(), "{:?}", same.report.warnings);

    let differ = build_session(mk(), never_identical);
    assert_eq!(name_of(&differ, "Mag_Images", SeriesType::Extra), "sub-p1/ses-20240101/extra_data/11_MagImages");
    assert_eq!(name_of(&differ, "gre", SeriesType::Magnitude), "sub-p1_ses-20240101_acq-gre_part-mag_T2starw");
    assert!(differ.report.warnings.iter().any(|w| w.contains("series 11 'Mag_Images' are both original magnitude")), "{:?}", differ.report.warnings);
}

#[test]
fn a_repeat_with_the_same_settings_is_run_2() {
    let p = "gre";
    let mut files = series("a", 1, p, p, M, &[5.0, 10.0], 100.0, 4);
    files.extend(series("b", 2, p, p, P, &[5.0, 10.0], 100.0, 4));
    files.extend(series("c", 3, p, p, M, &[5.0, 10.0], 900.0, 4));
    files.extend(series("d", 4, p, p, P, &[5.0, 10.0], 900.0, 4));
    let s = build_session(files, never_identical);
    let n: Vec<String> = names(&s).into_iter().map(|x| x.2).collect();
    assert_eq!(n, vec![
        "sub-p1_ses-20240101_acq-gre_run-1_echo-<N>_part-mag_MEGRE",
        "sub-p1_ses-20240101_acq-gre_run-1_echo-<N>_part-phase_MEGRE",
        "sub-p1_ses-20240101_acq-gre_run-2_echo-<N>_part-mag_MEGRE",
        "sub-p1_ses-20240101_acq-gre_run-2_echo-<N>_part-phase_MEGRE",
    ]);
}

#[test]
fn a_repeat_with_other_echoes_is_another_acquisition() {
    let p = "fl3d_vibe";
    let mut files = series("a", 11, p, p, M, &[20.0], 100.0, 4);
    files.extend(series("b", 12, p, p, P, &[20.0], 100.0, 4));
    files.extend(series("c", 20, p, p, M, &[2.0, 4.0, 6.0], 1200.0, 4));
    files.extend(series("d", 21, p, p, P, &[2.0, 4.0, 6.0], 1200.0, 4));
    let s = build_session(files, never_identical);
    let n: Vec<String> = names(&s).into_iter().map(|x| x.2).collect();
    assert_eq!(n, vec![
        "sub-p1_ses-20240101_acq-fl3dvibe_part-mag_T2starw",
        "sub-p1_ses-20240101_acq-fl3dvibe_part-phase_T2starw",
        "sub-p1_ses-20240101_acq-fl3dvibe2_echo-<N>_part-mag_MEGRE",
        "sub-p1_ses-20240101_acq-fl3dvibe2_echo-<N>_part-phase_MEGRE",
    ]);
    assert!(s.report.notes.iter().any(|n| n.contains("acq-fl3dvibe2")), "{:?}", s.report.notes);
}

#[test]
fn structural_and_clinical_series_outside_qsm_go_to_extra() {
    let mut files = series("mpr", 2, "AAHead_Scout_MPR_sag", "AAHead_Scout", &["DERIVED", "PRIMARY", "MPR", "ND", "NORM"], &[3.0], 50.0, 2);
    let mut flair = series("flair", 5, "t2_tirm_tra_dark-fluid", "t2_tirm_tra_dark-fluid", M, &[80.0], 200.0, 4);
    for f in &mut flair {
        f.scanning_sequence = vec!["SE".into(), "IR".into()];
    }
    files.extend(flair);
    files.extend(series("dwi", 6, "resolve_TRACEW", "resolve", &["DERIVED", "PRIMARY", "DIFFUSION", "TRACEW", "ND"], &[60.0], 300.0, 4));
    let s = build_session(files, never_identical);
    for (_, t, name) in names(&s) {
        assert_eq!(t, SeriesType::Extra);
        assert!(name.starts_with("sub-p1/ses-20240101/extra_data/"), "{name}");
    }
}

#[test]
fn bruker_magnitude_without_part_marker_and_its_t2star_map() {
    let mut files = series("mge", 90001, "", "T2star_map_MGE", &["ORIGINAL", "PRIMARY", "OTHER"], &[3.3, 6.6, 9.9], 100.0, 4);
    files.extend(series("map", 90002, "Parametric image for T2* relaxation", "", &["DERIVED", "SECONDARY", "OTHER"], &[3.3], 100.0, 4));
    for f in &mut files {
        f.scanning_sequence = vec!["RM".into()];
        f.manufacturer = "Bruker BioSpin MRI GmbH".into();
    }
    let s = build_session(files, never_identical);
    let n = names(&s);
    assert_eq!(n[0].1, SeriesType::Magnitude);
    assert_eq!(n[0].2, "sub-p1_ses-20240101_acq-T2starmapMGE_echo-<N>_part-mag_MEGRE");
    assert_eq!(n[1].1, SeriesType::T2starMap);
    // The map has no protocol name: it joins the acquisition it was made from.
    assert_eq!(n[1].2, "sub-p1_ses-20240101_acq-T2starmapMGE_rec-scanner_T2starmap");
}

#[test]
fn a_series_is_never_split_across_patients() {
    let mut files = series("a", 4, "QSM", "QSM", M, &[20.0], 100.0, 4);
    let mut pha = series("b", 5, "QSM", "QSM", P, &[20.0], 100.0, 4);
    pha[2].patient_id = "NACC1".into();
    pha[2].series_description = "Other name".into();
    pha[2].protocol_name = "Other protocol".into();
    files.extend(pha);
    let s = build_session(files, never_identical);
    assert_eq!(s.subjects.len(), 1);
    // The odd file does not rename the series: magnitude and phase stay one acquisition.
    assert_eq!(s.subjects[0].studies[0].acquisitions.len(), 1);
    assert_eq!(s.subjects[0].patient_id, "p1");
    assert_eq!(s.total_series(), 2);
    let w = &s.report.warnings;
    assert!(w.iter().any(|w| w.contains("1 of its 4 files name a different patient") && w.contains("b/0003.dcm (NACC1)") && w.contains("sub-p1")), "{w:?}");
}

#[test]
fn ge_private_image_type_splits_one_uid_into_parts() {
    let mut files = Vec::new();
    for (k, ge) in [0, 2, 3].into_iter().enumerate() {
        for i in 0..4 {
            let mut f = file("swan", 8, "Ax SWAN", "--STRONGER", &["ORIGINAL", "PRIMARY", "OTHER"], 20.0, 100.0, (k * 4 + i + 1) as i32);
            f.ge_image_type = Some(ge);
            f.manufacturer = "GE MEDICAL SYSTEMS".into();
            files.push(f);
        }
    }
    let s = build_session(files, never_identical);
    let n = names(&s);
    let types: Vec<SeriesType> = n.iter().map(|x| x.1).collect();
    assert_eq!(types, vec![SeriesType::Magnitude, SeriesType::Real, SeriesType::Imaginary]);
    assert_eq!(n[0].2, "sub-p1_ses-20240101_acq-STRONGER_part-mag_T2starw");
    assert_eq!(n[1].2, "sub-p1_ses-20240101_acq-STRONGER_part-real_T2starw");
    assert_eq!(n[2].2, "sub-p1_ses-20240101_acq-STRONGER_part-imag_T2starw");
}

#[test]
fn mp2rage_series_get_t1w_inv_and_map_names() {
    let p = "MP2RAGE_0p75mm";
    let mk = |uid: &str, num: i32, desc: &str, it: &[&str], ti: f64| {
        let mut v = series(uid, num, desc, p, it, &[2.0], 100.0, 4);
        for f in &mut v {
            f.inversion_time = Some(ti);
            f.scanning_sequence = vec!["GR".into(), "IR".into()];
        }
        v
    };
    let mut files = mk("i1", 27, "MP2RAGE_0p75mm_INV1", &["ORIGINAL", "PRIMARY", "M", "DIS3D"], 840.0);
    files.extend(mk("i2", 28, "MP2RAGE_0p75mm_INV2", &["ORIGINAL", "PRIMARY", "M", "DIS3D"], 2370.0));
    files.extend(mk("t1", 29, "MP2RAGE_0p75mm_T1_Images", &["DERIVED", "PRIMARY", "T1 MAP", "DIS3D"], 0.0));
    files.extend(mk("u", 31, "MP2RAGE_0p75mm_UNI_Images", &["DERIVED", "PRIMARY", "M", "UNI", "DIS3D"], 0.0));
    files.extend(mk("ud", 32, "MP2RAGE_0p75mm_UNI-DEN", &["DERIVED", "PRIMARY", "M", "UNI", "DIS3D"], 0.0));
    let s = build_session(files, never_identical);
    let n: Vec<String> = names(&s).into_iter().map(|x| x.2).collect();
    assert_eq!(n, vec![
        "sub-p1_ses-20240101_acq-MP2RAGE0p75mm_inv-1_MP2RAGE",
        "sub-p1_ses-20240101_acq-MP2RAGE0p75mm_inv-2_MP2RAGE",
        "sub-p1_ses-20240101_acq-MP2RAGE0p75mm_T1map",
        "sub-p1_ses-20240101_acq-MP2RAGE0p75mm_T1w",
        "sub-p1_ses-20240101_acq-MP2RAGE0p75mm_rec-uniden_T1w",
    ]);
}

#[test]
fn uncombined_coils_need_two_coil_elements() {
    let p = "SWI_3mm";
    let mut files = Vec::new();
    for (uid, num, it) in [("mu", 15, M), ("pu", 18, P)] {
        for c in 1..=2 {
            for mut f in series(uid, num, p, p, it, &[9.4, 20.0], 100.0, 2) {
                f.coil_string = format!("H{c}");
                f.instance_number = f.instance_number.map(|i| i + 100 * c);
                f.path = PathBuf::from(format!("{uid}/c{c}_{}.dcm", f.instance_number.unwrap()));
                files.push(f);
            }
        }
    }
    for (uid, num, it) in [("mc", 16, &["ORIGINAL", "PRIMARY", "M", "ND"][..]), ("mf", 17, &["ORIGINAL", "PRIMARY", "M", "ND", "NORM", "FM", "FIL"][..]), ("pc", 19, P)] {
        for mut f in series(uid, num, p, p, it, &[9.4, 20.0], 100.0, 2) {
            f.coil_string = "HEA;HEP".into();
            files.push(f);
        }
    }
    let s = build_session(files, never_identical);
    let n: Vec<String> = names(&s).into_iter().map(|x| x.2).collect();
    assert_eq!(n, vec![
        "sub-p1_ses-20240101_acq-SWI3mm_rec-uncombined_coil-<NN>_echo-<N>_part-mag_MEGRE",
        "sub-p1_ses-20240101_acq-SWI3mm_echo-<N>_part-mag_MEGRE",
        "sub-p1_ses-20240101_acq-SWI3mm_rec-filtered_echo-<N>_part-mag_MEGRE",
        "sub-p1_ses-20240101_acq-SWI3mm_rec-uncombined_coil-<NN>_echo-<N>_part-phase_MEGRE",
        "sub-p1_ses-20240101_acq-SWI3mm_echo-<N>_part-phase_MEGRE",
    ]);
    let unc = s.subjects[0].studies[0].acquisitions[0].series.iter().find(|x| x.series_number == 15).unwrap();
    assert_eq!(unc.coil_type, CoilType::Uncombined);
    assert_eq!(unc.coil_groups.len(), 2);

    // One coil element name on its own (7T `A32`) is the combined image.
    let mut files = series("a", 1, "gre", "gre", M, &[20.0], 100.0, 4);
    for f in &mut files {
        f.coil_string = "A32".into();
    }
    let s = build_session(files, never_identical);
    assert_eq!(s.subjects[0].studies[0].acquisitions[0].series[0].coil_type, CoilType::Combined);
}

#[test]
fn aspire_magnitude_follows_the_aspire_phase() {
    let p = "ASPIRE_bipolar";
    let mut files = series("am", 23, "Aspire_M_ASPIRE_bipolar", p, &["ORIGINAL", "PRIMARY", "M", "ASPIRE", "DIS2D"], &[2.0, 4.0], 100.0, 2);
    files.extend(series("ap", 24, "Aspire_P_ASPIRE_bipolar", p, &["ORIGINAL", "PRIMARY", "P", "ASPIRE", "DIS2D"], &[2.0, 4.0], 100.0, 2));
    files.extend(series("m", 25, "ASPIRE_bipolar", p, &["ORIGINAL", "PRIMARY", "M", "DIS2D"], &[2.0, 4.0], 100.0, 2));
    files.extend(series("r2", 26, "R2star_ASPIRE_bipolar", p, &["ORIGINAL", "PRIMARY", "M", "DIS2D"], &[2.0], 100.0, 2));
    files.extend(series("cx", 22, "Aspire_magCxSum_ASPIRE_bipolar", p, &[], &[2.0, 4.0], 100.0, 2));
    files.extend(series("qm", 21, "Aspire_Qmetric_ASPIRE_bipolar", p, &[], &[2.0, 4.0], 100.0, 2));
    let s = build_session(files, never_identical);
    // The magnitude of the phase's reconstruction (ASPIRE) is the main one; the scanner's own
    // coil combination gets rec-scanner.
    assert_eq!(name_of(&s, "Aspire_M_ASPIRE_bipolar", SeriesType::Magnitude), "sub-p1_ses-20240101_acq-ASPIREbipolar_echo-<N>_part-mag_MEGRE");
    assert_eq!(name_of(&s, "ASPIRE_bipolar", SeriesType::Magnitude), "sub-p1_ses-20240101_acq-ASPIREbipolar_rec-scanner_echo-<N>_part-mag_MEGRE");
    assert_eq!(name_of(&s, "Aspire_P_ASPIRE_bipolar", SeriesType::Phase), "sub-p1_ses-20240101_acq-ASPIREbipolar_echo-<N>_part-phase_MEGRE");
    assert_eq!(name_of(&s, "Aspire_magCxSum_ASPIRE_bipolar", SeriesType::Magnitude), "sub-p1_ses-20240101_acq-ASPIREbipolar_rec-aspiremagcxsum_echo-<N>_part-mag_MEGRE");
    assert_eq!(name_of(&s, "R2star_ASPIRE_bipolar", SeriesType::R2starMap), "sub-p1_ses-20240101_acq-ASPIREbipolar_rec-scanner_R2starmap");
    assert_eq!(name_of(&s, "Aspire_Qmetric_ASPIRE_bipolar", SeriesType::Extra), "sub-p1/ses-20240101/extra_data/21_AspireQmetricASPIREbipolar");
}

#[test]
fn incomplete_exports_are_reported() {
    let mut files = series("m", 9, "gre", "gre", M, &[20.0], 100.0, 6);
    let mut pha = series("p", 10, "gre", "gre", P, &[20.0], 100.0, 6);
    pha.remove(3);
    files.extend(pha);
    let s = build_session(files, never_identical);
    let w = &s.report.warnings;
    assert!(w.iter().any(|w| w.contains("the magnitude has 6 images but the phase has 5")), "{w:?}");
    assert!(w.iter().any(|w| w.contains("series 10 'gre': 5 images with instance numbers 1–6, 1 missing")), "{w:?}");
}

#[test]
fn non_image_series_are_listed_not_converted() {
    let mut files = series("m", 9, "gre", "gre", M, &[20.0], 100.0, 2);
    let mut sr = series("sr", 99, "PhoenixZIPReport", "Phoenix Document", &["ORIGINAL", "PRIMARY", "OTHER", "CSA REPORT"], &[0.0], 100.0, 2);
    for f in &mut sr {
        f.sop_class_uid = "1.2.840.10008.5.1.4.1.1.88.22".into();
    }
    files.extend(sr);
    let s = build_session(files, never_identical);
    assert_eq!(s.total_series(), 1);
    assert_eq!(s.report.skipped, vec!["series 99 'PhoenixZIPReport' (structured report, 2 files)".to_string()]);
}

#[test]
fn echoes_exported_one_series_each_form_one_row_with_every_te() {
    let p = "t2starME";
    let mut files = Vec::new();
    for (e, te) in [3.0, 6.0, 9.0].into_iter().enumerate() {
        files.extend(series(&format!("m{e}"), 1001 + e as i32, p, p, &["ORIGINAL", "PRIMARY", "M", "NORM", "DIS2D", "MFSPLIT"], &[te], 100.0, 2));
        files.extend(series(&format!("p{e}"), 2001 + e as i32, p, p, &["ORIGINAL", "PRIMARY", "P", "DIS2D", "MFSPLIT"], &[te], 100.0, 2));
    }
    let s = build_session(files, never_identical);
    let unique = s.unique_series();
    assert_eq!(unique.len(), 2);
    assert_eq!(unique[0].echo_times, vec![3.0, 6.0, 9.0]);
    let n: Vec<String> = names(&s).into_iter().map(|x| x.2).collect();
    assert_eq!(n[0], "sub-p1_ses-20240101_acq-t2starME_echo-1_part-mag_MEGRE");
    assert_eq!(n[1], "sub-p1_ses-20240101_acq-t2starME_echo-2_part-mag_MEGRE");
    assert_eq!(n[5], "sub-p1_ses-20240101_acq-t2starME_echo-3_part-phase_MEGRE");
}

#[test]
fn empty_protocol_and_description_fall_back_to_the_sequence_name() {
    let mut files = series("m", 9, "", "", M, &[20.0], 100.0, 2);
    for f in &mut files {
        f.sequence_name = "*fl3d1r".into();
    }
    let s = build_session(files, never_identical);
    assert_eq!(s.subjects[0].studies[0].acquisitions[0].name, "fl3d1r");
}

#[test]
fn relabelling_propagates_to_every_subject_and_keeps_its_row() {
    let mut files = series("a", 1, "gre", "gre", M, &[20.0], 100.0, 2);
    files.extend(series("b", 2, "gre", "gre", P, &[20.0], 100.0, 2));
    let mut other: Vec<FileMeta> = files.iter().cloned().map(|mut f| {
        f.patient_id = "p2".into();
        f.series_uid = format!("{}2", f.series_uid);
        f
    }).collect();
    files.append(&mut other);
    let mut s = build_session(files, never_identical);
    let groups = s.unique_series();
    assert_eq!(groups.len(), 2);
    assert!(groups.iter().all(|g| g.subject_count() == 2));
    let refs = groups[1].refs.clone();
    s.set_type_for_refs(&refs, SeriesType::Real);
    assert!(refs.iter().all(|r| s.series_ref(r).series_type == SeriesType::Real));
    assert_eq!(s.unique_series().len(), 2);
}

#[test]
fn series_type_cycles() {
    for &t in SeriesType::ALL {
        assert_eq!(t.next().prev(), t);
    }
    assert_eq!(SeriesType::Skip.next(), SeriesType::Magnitude);
}

#[test]
fn normalize_description() {
    assert_eq!(normalize_series_description("t2star_qsm_RR_RR"), "t2star_qsm");
    assert_eq!(clean_bids_label("p025_pre^^^^"), "p025pre");
}

#[test]
fn a_derived_series_without_protocol_joins_its_scan_by_bruker_numbering() {
    let mut files = series("mge", 90001, "", "T2star_map_MGE", &["ORIGINAL", "PRIMARY", "OTHER"], &[3.3, 6.6], 100.0, 2);
    files.extend(series("mge2", 120001, "", "T2star_map_MGE", &["ORIGINAL", "PRIMARY", "OTHER"], &[3.3, 6.6], 4000.0, 2));
    let mut map = series("map", 120002, "Parametric image for T2* relaxation", "", &["DERIVED", "SECONDARY", "OTHER"], &[3.3], 0.0, 2);
    for f in &mut map {
        f.acquisition_time = None;
    }
    files.extend(map);
    for f in &mut files {
        f.scanning_sequence = vec!["RM".into()];
    }
    let s = build_session(files, never_identical);
    let n = names(&s);
    let map = n.iter().find(|x| x.1 == SeriesType::T2starMap).unwrap();
    assert_eq!(map.2, "sub-p1_ses-20240101_acq-T2starmapMGE_rec-scanner_run-2_T2starmap");
    let s_map = s.flat_series().into_iter().map(|r| s.series_ref(&r).clone()).find(|x| x.series_number == 120002).unwrap();
    assert_eq!(s_map.note.as_deref(), Some("made from series 120001 'T2star_map_MGE'"));
}
