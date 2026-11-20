use crate::cli::*;
use qsmxt_io::testutils::multiorient as mo;
use std::path::{Path, PathBuf};

/// Three orientations spanning a realistic in-vivo range (0–30°).
const DIRS: [(f64, f64, f64); 3] = [
    (0.0, 0.0, 1.0),
    (0.0, 0.5, 0.866_025_4),
    (0.5, 0.0, 0.866_025_4),
];

/// Six orientations that span all three axes, as STI requires.
const SIX_DIRS: [(f64, f64, f64); 6] = [
    (0.0, 0.0, 1.0),
    (0.0, 0.5, 0.866_025_4),
    (0.5, 0.0, 0.866_025_4),
    (0.4, 0.4, 0.824_620_9),
    (-0.4, 0.3, 0.866_025_4),
    (0.3, -0.45, 0.840_000_0),
];

/// Write the forward fields for `dirs` and return their paths.
fn write_fields(dir: &Path, chi: &[f64], dirs: &[(f64, f64, f64)]) -> Vec<PathBuf> {
    dirs.iter()
        .enumerate()
        .map(|(i, &b)| {
            let p = dir.join(format!("field{i}.nii"));
            mo::write(&p, &mo::forward_field(chi, b));
            p
        })
        .collect()
}

fn flat_dirs(dirs: &[(f64, f64, f64)]) -> Vec<f64> {
    dirs.iter().flat_map(|d| [d.0, d.1, d.2]).collect()
}

fn common(
    inputs: Vec<PathBuf>,
    mask: PathBuf,
    b0_direction: Vec<f64>,
) -> MultiOrientCommonArgs {
    MultiOrientCommonArgs { inputs, b0_direction, b0_dirs: None, mask, force: false }
}

fn read(path: &Path) -> Vec<f64> {
    qsm_core::io::read_nifti_file(path).unwrap().data
}

/// `got` is `want` as written to a float32 NIfTI.
fn assert_same_map(got: &[f64], want: &[f64], what: &str) {
    assert_eq!(got.len(), want.len(), "{what}");
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert!((g - w).abs() <= 1e-5 * (1.0 + w.abs()), "{what} differs at {i}: {g} vs {w}");
    }
}

/// qsm-core's COSMOS on the phantom's fields for `dirs`, with `lambda`.
fn core_cosmos(chi: &[f64], dirs: &[(f64, f64, f64)], lambda: Option<f64>) -> Vec<f64> {
    let fields: Vec<Vec<f64>> = dirs.iter().map(|&b| mo::forward_field(chi, b)).collect();
    let d = qsm_core::inversion::CosmosParams::default();
    let params = qsm_core::inversion::CosmosParams { lambda: lambda.unwrap_or(d.lambda), ..d };
    qsm_core::inversion::cosmos(&fields, dirs, &mo::mask(), &mo::grid(), &params)
}

/// Every orientation's field reaches qsm-core with its own direction, and the mask with them.
#[test]
fn cosmos_gives_qsm_core_every_orientation_with_its_direction() {
    let dir = tempfile::tempdir().unwrap();
    let chi = mo::chi_phantom();
    let inputs = write_fields(dir.path(), &chi, &DIRS);
    let mask_path = dir.path().join("mask.nii");
    mo::write_mask(&mask_path);
    let out = dir.path().join("cosmos.nii");

    super::invert::execute(InvertCommand::Cosmos(InvertCosmosArgs {
        common: common(inputs, mask_path, flat_dirs(&DIRS)),
        output: out.clone(),
        lambda: None,
        magnitudes: vec![],
        tol: None,
        max_iter: None,
    }))
    .unwrap();

    assert_same_map(&read(&out), &core_cosmos(&chi, &DIRS, None), "COSMOS");
}

/// The degenerate set that this feature exists to refuse: same direction N times.
#[test]
fn refuses_identical_directions() {
    let dir = tempfile::tempdir().unwrap();
    let chi = mo::chi_phantom();
    let same = [DIRS[0], DIRS[0], DIRS[0]];
    let inputs = write_fields(dir.path(), &chi, &same);
    let mask_path = dir.path().join("mask.nii");
    mo::write_mask(&mask_path);

    let err = super::invert::execute(InvertCommand::Cosmos(InvertCosmosArgs {
        common: common(inputs, mask_path, flat_dirs(&same)),
        output: dir.path().join("out.nii"),
        lambda: None,
        magnitudes: vec![],
        tol: None,
        max_iter: None,
    }))
    .unwrap_err();
    assert!(format!("{err}").contains("one orientation measured 3 times"), "{err}");
}

/// ...and reconstructs it anyway under --force, because refusing is a guard rail, not a veto.
#[test]
fn force_overrides_the_degeneracy_check() {
    let dir = tempfile::tempdir().unwrap();
    let chi = mo::chi_phantom();
    let same = [DIRS[0], DIRS[0]];
    let inputs = write_fields(dir.path(), &chi, &same);
    let mask_path = dir.path().join("mask.nii");
    mo::write_mask(&mask_path);
    let out = dir.path().join("out.nii");

    let mut c = common(inputs, mask_path, flat_dirs(&same));
    c.force = true;
    super::invert::execute(InvertCommand::Cosmos(InvertCosmosArgs {
        common: c,
        output: out.clone(),
        lambda: Some(1e-2),
        magnitudes: vec![],
        tol: None,
        max_iter: None,
    }))
    .unwrap();
    // ...with `--lambda` reaching qsm-core.
    assert_same_map(&read(&out), &core_cosmos(&chi, &same, Some(1e-2)), "forced COSMOS");
}

#[test]
fn rejects_a_direction_count_that_does_not_match_the_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let chi = mo::chi_phantom();
    let inputs = write_fields(dir.path(), &chi, &DIRS);
    let mask_path = dir.path().join("mask.nii");
    mo::write_mask(&mask_path);

    let err = super::invert::execute(InvertCommand::Cosmos(InvertCosmosArgs {
        common: common(inputs, mask_path, flat_dirs(&DIRS[..2])),
        output: dir.path().join("out.nii"),
        lambda: None,
        magnitudes: vec![],
        tol: None,
        max_iter: None,
    }))
    .unwrap_err();
    assert!(format!("{err}").contains("2 B0 direction(s) for 3 input(s)"), "{err}");
}

/// Same dimensions, different affine: the volumes are not on a common grid, and combining
/// them voxel-for-voxel would silently mix different physical space.
#[test]
fn rejects_inputs_whose_affines_disagree() {
    let dir = tempfile::tempdir().unwrap();
    let chi = mo::chi_phantom();
    let inputs = write_fields(dir.path(), &chi, &DIRS);
    // Rewrite the second input with a shifted affine.
    let mut shifted = mo::MO_AFFINE;
    shifted[3] = 10.0;
    qsm_core::io::save_nifti_to_file(
        &inputs[1],
        &mo::forward_field(&chi, DIRS[1]),
        mo::MO_DIMS,
        mo::MO_VOXEL,
        &shifted,
    )
    .unwrap();
    let mask_path = dir.path().join("mask.nii");
    mo::write_mask(&mask_path);

    let err = super::invert::execute(InvertCommand::Cosmos(InvertCosmosArgs {
        common: common(inputs, mask_path, flat_dirs(&DIRS)),
        output: dir.path().join("out.nii"),
        lambda: None,
        magnitudes: vec![],
        tol: None,
        max_iter: None,
    }))
    .unwrap_err();
    assert!(format!("{err}").contains("not on a common grid"), "{err}");
}

#[test]
fn cosmos_reads_directions_from_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let chi = mo::chi_phantom();
    let inputs = write_fields(dir.path(), &chi, &DIRS);
    let mask_path = dir.path().join("mask.nii");
    mo::write_mask(&mask_path);
    let dirs_file = dir.path().join("dirs.txt");
    std::fs::write(
        &dirs_file,
        "# B0 directions, one per orientation\n0 0 1\n0, 0.5, 0.8660254\n0.5 0 0.8660254\n",
    )
    .unwrap();
    let out = dir.path().join("cosmos.nii");

    let mut c = common(inputs, mask_path, vec![]);
    c.b0_dirs = Some(dirs_file);
    super::invert::execute(InvertCommand::Cosmos(InvertCosmosArgs {
        common: c,
        output: out.clone(),
        lambda: None,
        magnitudes: vec![],
        tol: None,
        max_iter: None,
    }))
    .unwrap();
    assert_same_map(&read(&out), &core_cosmos(&chi, &DIRS, None), "COSMOS with --b0-dirs");
}

#[test]
fn sti_writes_the_tensor_and_its_derived_maps() {
    let dir = tempfile::tempdir().unwrap();
    let chi = mo::chi_phantom();
    let inputs = write_fields(dir.path(), &chi, &SIX_DIRS);
    let mask_path = dir.path().join("mask.nii");
    mo::write_mask(&mask_path);
    let prefix = dir.path().join("sti");

    super::invert::execute(InvertCommand::Sti(InvertStiArgs {
        common: common(inputs, mask_path, flat_dirs(&SIX_DIRS)),
        output: prefix.clone(),
        lambda: Some(1e-3),
    }))
    .unwrap();

    for name in ["desc-sti_Chimap", "desc-mms_Chimap", "desc-msa_Chimap", "desc-sti_evec"] {
        let p = dir.path().join(format!("sti_{name}.nii"));
        assert!(p.exists(), "missing {}", p.display());
    }
    // One 4D tensor (6 components, their order in the sidecar) and one 4D eigenvector.
    let bytes = std::fs::read(dir.path().join("sti_desc-sti_Chimap.nii")).unwrap();
    assert_eq!(qsm_core::io::load_nifti_4d(&bytes).unwrap().1 .3, 6);
    let sidecar: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join("sti_desc-sti_Chimap.json")).unwrap()).unwrap();
    assert_eq!(sidecar["TensorComponents"], serde_json::json!(["X11", "X12", "X13", "X22", "X23", "X33"]));
    let bytes = std::fs::read(dir.path().join("sti_desc-sti_evec.nii")).unwrap();
    assert_eq!(qsm_core::io::load_nifti_4d(&bytes).unwrap().1 .3, 3);

    // The maps are qsm-core's STI of every orientation with its direction, and `--lambda`.
    let fields: Vec<Vec<f64>> = SIX_DIRS.iter().map(|&b| mo::forward_field(&chi, b)).collect();
    let tensor = qsm_core::inversion::sti(&fields, &SIX_DIRS, &mo::mask(), &mo::grid(),
        &qsm_core::inversion::StiParams { lambda: 1e-3 });
    let maps = qsm_core::inversion::tensor_maps(&tensor, &mo::mask());
    assert_same_map(&read(&dir.path().join("sti_desc-mms_Chimap.nii")), &maps.mms, "STI MMS");
    assert_same_map(&read(&dir.path().join("sti_desc-msa_Chimap.nii")), &maps.msa, "STI MSA");
}

/// #275: each standalone command, given a run's BIDS basename as its prefix, writes exactly
/// the files the pipeline would publish for that run — the same entities and suffixes.
#[test]
fn standalone_commands_name_their_outputs_as_the_pipeline_does() {
    use qsmxt_io::bids::derivatives::DerivativeOutputs;
    use qsmxt_io::bids::entities::AcquisitionKey;
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let key = AcquisitionKey {
        subject: "01".into(), session: Some("pre".into()), acquisition: None,
        reconstruction: None, inversion: None, run: Some("2".into()), suffix: "MEGRE".into(),
    };
    let pipeline = DerivativeOutputs::new(d);
    let name = |p: PathBuf| p.file_name().unwrap().to_string_lossy().into_owned();
    let prefix = d.join(key.basename());
    let exists = |n: String| assert!(d.join(&n).exists(), "standalone did not write {n}");

    // invert sti
    let chi = mo::chi_phantom();
    let fields = write_fields(d, &chi, &SIX_DIRS);
    let mask_path = d.join("mask.nii");
    mo::write_mask(&mask_path);
    super::invert::execute(InvertCommand::Sti(InvertStiArgs {
        common: common(fields, mask_path.clone(), flat_dirs(&SIX_DIRS)),
        output: prefix.clone(),
        lambda: Some(1e-3),
    })).unwrap();
    for p in [pipeline.sti_tensor_path(&key), pipeline.sti_mms_path(&key),
              pipeline.sti_msa_path(&key), pipeline.sti_eigenvector_path(&key)] {
        exists(name(p));
    }
    exists(name(pipeline.multiorient_sidecar_path(&key, "sti")));

    // separate
    let qsm = d.join("qsm.nii");
    let r2star = d.join("r2star.nii");
    mo::write(&qsm, &chi);
    mo::write(&r2star, &vec![20.0; chi.len()]);
    super::separate::execute(SeparateCommand::R2starQsm(SeparateR2starQsmArgs {
        common: SeparateCommonArgs {
            qsm, mask: mask_path.clone(), output: prefix.clone(), field_strength: 3.0,
            b0_direction: vec![0.0, 0.0, 1.0],
        },
        r2star: Some(r2star), magnitude: vec![], echo_times: vec![], settings: Default::default(),
    })).unwrap();
    for p in [pipeline.chi_para_path(&key), pipeline.chi_dia_path(&key), pipeline.chi_sep_total_path(&key)] {
        exists(name(p));
    }

    // swi --mip, with the projection left to its default name
    let swi = pipeline.swi_path(&key);
    let (phase, mag) = (d.join("phase.nii"), d.join("mag.nii"));
    qsmxt_io::testutils::write_phase(&phase);
    qsmxt_io::testutils::write_magnitude(&mag);
    let small_mask = d.join("small_mask.nii");
    qsmxt_io::testutils::write_mask(&small_mask);
    super::swi::execute(SwiArgs {
        phase: vec![phase.clone()], magnitude: vec![mag.clone()], tes: None, mask: Some(small_mask.clone()),
        output: d.join(name(swi.clone())),
        mip: true, mip_output: None, filtered_phase: true, filtered_phase_output: None,
        swi_params: Default::default(),
    }).unwrap();
    exists(name(swi));
    exists(name(pipeline.swi_mip_path(&key)));
    exists(name(pipeline.swi_phase_path(&key)));

    // combine mcpc3ds, on a per-coil run (`rec-uncombined`) like the pipeline's
    let unc = AcquisitionKey { reconstruction: Some("uncombined".into()), ..key.clone() };
    let coil_dir = d.join("coils");
    std::fs::create_dir_all(&coil_dir).unwrap();
    let (mut phases, mut mags) = (vec![], vec![]);
    for c in 1..=2 {
        for e in 1..=2 {
            let stem = format!("{}_coil-0{c}_echo-{e}", unc.basename());
            let p = coil_dir.join(format!("{stem}_part-phase_MEGRE.nii"));
            let m = coil_dir.join(format!("{stem}_part-mag_MEGRE.nii"));
            qsmxt_io::testutils::write_phase(&p);
            qsmxt_io::testutils::write_magnitude(&m);
            phases.push(p);
            mags.push(m);
        }
    }
    super::combine::execute(CombineCommand::Mcpc3ds(CombineMcpc3dsArgs {
        phase: phases, magnitude: mags, output: d.join(unc.basename()),
        tes: Some(vec![0.005, 0.010]), num_echoes: None, sigma: None,
        hip_unwrapping: UnwrapAlgorithmArg::Laplacian, prelude_params: Default::default(),
    })).unwrap();
    for e in 1..=2 {
        exists(name(pipeline.combined_phase_path(&unc, e)));
        exists(name(pipeline.combined_mag_path(&unc, e)));
    }
    exists(name(pipeline.combine_mask_path(&unc)));
}

#[test]
fn sti_refuses_fewer_than_six_orientations() {
    let dir = tempfile::tempdir().unwrap();
    let chi = mo::chi_phantom();
    let inputs = write_fields(dir.path(), &chi, &DIRS);
    let mask_path = dir.path().join("mask.nii");
    mo::write_mask(&mask_path);

    let err = super::invert::execute(InvertCommand::Sti(InvertStiArgs {
        common: common(inputs, mask_path, flat_dirs(&DIRS)),
        output: dir.path().join("sti"),
        lambda: None,
    }))
    .unwrap_err();
    assert!(format!("{err}").contains("at least 6"), "{err}");
}
