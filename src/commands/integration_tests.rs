use crate::cli::*;
use crate::testutils;
use std::path::{Path, PathBuf};

// The topic files call the commands as `super::<command>`, as this file does.
use super::{
    aggregate, bgremove, common, homogeneity, init, invert, mask, qsmart, quality_map, r2prime, r2star,
    recommend, resample, run, segment, separate, slurm, swi, t2star, unwrap, validate,
};

mod mask_tests;
mod separate_tests;
mod unwrap_tests;
mod bgremove_tests;
mod invert_tests;
mod standalone_tests;
mod validate_tests;
mod run_tests;
mod slurm_tests;
mod recommendations_tests;

fn default_run_args(bids_dir: PathBuf, output_dir: PathBuf) -> RunArgs {
    RunArgs {
        bids_dir,
        output_dir: Some(output_dir),
        config: None,
        include: None,
        exclude: None,
        num_echoes: None,
        n_procs: Some(1),
        source_dicom: None,
        dicom_outputs: None,
        dry: true,
        debug: false,
        mem_limit_gb: None,
        no_mem_limit: false,
        force: false,
        clean_intermediates: false,
        no_dataset_steps: false,
        accept_recommendations: false,
        accept_reference_settings: false,
        pipeline: Default::default(),
    }
}

fn common_mask(input: PathBuf, output: PathBuf) -> MaskCommonArgs {
    MaskCommonArgs { input, output, ops: vec![] }
}

fn common_bgremove(input: PathBuf, mask: PathBuf, output: PathBuf) -> BgremoveCommonArgs {
    BgremoveCommonArgs {
        input, mask, output,
        b0_direction: vec![0.0, 0.0, 1.0],
        output_mask: None,
    }
}

fn common_invert(input: PathBuf, mask: PathBuf, output: PathBuf) -> InvertCommonArgs {
    InvertCommonArgs {
        input, mask, output,
        b0_direction: vec![0.0, 0.0, 1.0],
        tiling_params: Default::default(),
    }
}

fn scan(echo_times: &[f64]) -> crate::cli::ScanParamArgs {
    crate::cli::ScanParamArgs { field_strength: 3.0, echo_times: echo_times.to_vec() }
}

fn common_unwrap(input: PathBuf, mask: PathBuf, output: PathBuf) -> UnwrapCommonArgs {
    UnwrapCommonArgs { input, mask, output }
}

/// The cheapest pipeline: what is under test is which runs were processed, not how.
fn cheap_pipeline() -> crate::cli_settings::SettingsArgs {
    crate::cli_settings::SettingsArgs::from_flags(&["--qsm-algorithm", "tkd", "--unwrapping-algorithm", "laplacian"])
}

fn aggregate_args(bids: PathBuf, out: PathBuf, include: Vec<String>) -> AggregateArgs {
    AggregateArgs {
        bids_dir: bids, output_dir: Some(out), config: None, include: Some(include), exclude: None,
        num_echoes: None, pipeline: cheap_pipeline(), debug: false, force: false,
        clean_intermediates: false, command_line: None,
    }
}

/// #275: derivatives carrying every name QSMxT publishes, for the BIDS validator. Not a test
/// on its own (`--ignored`): CI's `bids-validator` job runs it with `QSMXT_BIDS_FIXTURE_OUT`
/// set and validates each `<case>/derivatives/qsmxt` it leaves there, with only the
/// documented `.bidsignore` entries.
#[test]
#[ignore]
fn bids_validator_fixture() {
    let root = PathBuf::from(std::env::var("QSMXT_BIDS_FIXTURE_OUT")
        .expect("set QSMXT_BIDS_FIXTURE_OUT to the directory to write the derivatives to"));
    let scratch = tempfile::tempdir().unwrap();
    let run = |case: &str, bids: &Path, set: &dyn Fn(&mut RunArgs)| {
        let mut args = default_run_args(bids.to_path_buf(), root.join(case));
        args.dry = false;
        args.no_mem_limit = true;
        args.pipeline.add(&["--masking-input", "magnitude"]);
        args.pipeline.add(&["--unwrapping-algorithm", "laplacian"]);
        // Output names and sidecars do not depend on the inversion, and the default (iLSQR,
        // with STI's 64 mm padding) takes minutes per run in a debug build.
        args.pipeline.add(&["--qsm-algorithm", "tkd"]);
        set(&mut args);
        super::run::execute(args).unwrap();
    };

    // Every output a multi-echo run can make: two-pass χ, the total field, SWI/SMWI, T2*/R2*,
    // R2/R2' from a MESE, chi-separation, motion correction, a (supplied) segmentation and its
    // statistics, and DICOM export — plus a GRE twin differing only by suffix.
    let bids = scratch.path().join("full");
    write_full_case(&bids);
    run("full", &bids, &|a| {
        let p = &mut a.pipeline;
        p.add(&["--two-pass"]);
        p.add(&["--do-swi"]);
        p.add(&["--swi-filtered-phase"]);
        p.add(&["--do-smwi"]);
        p.add(&["--do-t2starmap"]);
        p.add(&["--do-r2starmap"]);
        p.add(&["--do-r2map"]);
        p.add(&["--do-r2primemap"]);
        p.add(&["--r2prime-strategy", "mese"]);
        p.add(&["--do-chisep"]);
        p.add(&["--chisep", "r2star-qsm"]);
        p.add(&["--do-analysis"]);
        p.add(&["--use-custom-dseg", "mydseg"]);
        p.add(&["--motion-correction"]);
        p.add(&["--export-dicom"]);
    });

    // A per-echo QSM: each echo's map beside the mean, and DECOMPOSE from them.
    let bids = scratch.path().join("perecho");
    testutils::create_multi_echo_bids(&bids);
    run("perecho", &bids, &|a| {
        a.pipeline.add(&["--qsm-per-echo", "true"]);
        a.pipeline.add(&["--do-chisep"]);
        a.pipeline.add(&["--chisep", "decompose"]);
        a.pipeline.add(&["--decompose-n-inner", "1"]);
        a.pipeline.add(&["--decompose-max-iterations", "5"]);
    });

    // Per-coil data: the MCPC-3D-S combined echoes.
    let bids = scratch.path().join("coils");
    testutils::create_multi_coil_bids(&bids, 2);
    run("coils", &bids, &|_| {});

    // COSMOS over three orientations.
    let bids = scratch.path().join("cosmos");
    testutils::create_multi_orientation_bids(&bids);
    run("cosmos", &bids, &|a| {
        a.include = Some(vec!["*acq-dir*".to_string()]);
        a.pipeline.add(&["--orientation-group", "acq"]);
    });

    // STI over six: the 4D tensor and eigenvector.
    let bids = scratch.path().join("sti");
    write_sti_case(&bids);
    run("sti", &bids, &|a| {
        a.pipeline.add(&["--orientation-group", "acq"]);
        a.pipeline.add(&["--multi-orientation-algorithm", "sti"]);
        a.pipeline.add(&["--multi-orientation-force"]);
    });

    // An oblique run left on its axial working grid: `space-axial` and `SpatialReference`.
    let bids = scratch.path().join("oblique");
    write_oblique_case(&bids);
    run("oblique", &bids, &|a| {
        a.pipeline.add(&["--axial-resampling"]);
        a.pipeline.add(&["--obliquity-threshold", "5"]);
        a.pipeline.add(&["--output-space", "working"]);
        a.pipeline.add(&["--do-swi"]);
        a.pipeline.add(&["--swi-filtered-phase"]);
    });

    for case in ["full", "coils", "cosmos", "sti", "oblique"] {
        assert!(root.join(case).join("derivatives/qsmxt/dataset_description.json").exists(), "{case}");
    }
    let full = root.join("full/derivatives/qsmxt/sub-1/anat");
    for name in ["sub-1_acq-GRE_Chimap.nii", "sub-1_acq-MEGRE_desc-paramagneticsmwi_minIP.nii",
                 "sub-1_acq-MEGRE_desc-swi_phase.nii"] {
        assert!(full.join(name).exists(), "{name}");
    }
    let sidecar: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(full.join("sub-1_acq-MEGRE_desc-swi_phase.json")).unwrap()).unwrap();
    assert_eq!(sidecar["Units"], "Hz", "{sidecar}");
    for name in ["sub-1_acq-MEGRE_desc-total_fieldmap.nii", "sub-1_acq-MEGRE_desc-total_magnitude.nii"] {
        assert!(full.with_file_name("fmap").join(name).exists(), "{name}");
    }
    assert!(root.join("sti/derivatives/qsmxt/sub-1/anat/sub-1_desc-sti_Chimap.nii").exists());
    assert!(root.join("oblique/derivatives/qsmxt/sub-1/anat/sub-1_space-axial_Chimap.nii").exists());
}

/// The `full` case's dataset: three GRE echoes beside the multi-echo run, a four-echo MESE,
/// and a supplied segmentation.
fn write_full_case(bids: &Path) {
    testutils::create_multi_echo_bids(bids);
    let anat = bids.join("sub-1/anat");
    for (e, te) in [0.004f64, 0.008, 0.012].into_iter().enumerate() {
        for part in ["phase", "mag"] {
            let stem = format!("sub-1_echo-{}_part-{part}_GRE", e + 1);
            if part == "phase" { testutils::write_phase(&anat.join(format!("{stem}.nii"))); }
            else { testutils::write_magnitude_decayed(&anat.join(format!("{stem}.nii")), (-30.0 * te).exp()); }
            testutils::write_sidecar(&anat.join(format!("{stem}.json")), te, 3.0);
        }
    }
    for echo in 1..=4 {
        let te = 0.012 * echo as f64;
        let data = vec![1000.0 * (-15.0 * te).exp(); 512];
        qsm_core::io::save_nifti_to_file(&anat.join(format!("sub-1_echo-{echo}_MESE.nii")),
            &data, (8, 8, 8), (1.0, 1.0, 1.0), &crate::testutils::multiorient::MO_AFFINE).unwrap();
        testutils::write_sidecar(&anat.join(format!("sub-1_echo-{echo}_MESE.json")), te, 3.0);
    }
    testutils::write_dseg(&bids.join("derivatives/mydseg/sub-1/anat/sub-1_dseg.nii"));
}

/// The `sti` case's dataset: six orientations, one echo each.
fn write_sti_case(bids: &Path) {
    let anat = bids.join("sub-1/anat");
    std::fs::create_dir_all(&anat).unwrap();
    for (i, b0) in [(0.0, 0.0, 1.0), (0.0, 0.5, 0.866_025_4), (0.5, 0.0, 0.866_025_4),
                    (0.4, 0.4, 0.824_620_9), (-0.4, 0.3, 0.866_025_4), (0.3, -0.45, 0.84)]
        .into_iter().enumerate()
    {
        let stem = format!("sub-1_acq-dir{}_echo-1", i + 1);
        testutils::write_phase(&anat.join(format!("{stem}_part-phase_MEGRE.nii")));
        testutils::write_magnitude(&anat.join(format!("{stem}_part-mag_MEGRE.nii")));
        for part in ["phase", "mag"] {
            testutils::write_sidecar_with_b0(
                &anat.join(format!("{stem}_part-{part}_MEGRE.json")), 0.012, 3.0, Some(b0));
        }
    }
}

/// The `oblique` case's dataset: two echoes on a grid tilted 30° about x.
fn write_oblique_case(bids: &Path) {
    let anat = bids.join("sub-1/anat");
    std::fs::create_dir_all(&anat).unwrap();
    let (c, s) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
    let affine = [1.0, 0.0, 0.0, -10.0, 0.0, c, -s, -11.0, 0.0, s, c, -9.0, 0.0, 0.0, 0.0, 1.0];
    let dims = (20, 22, 18);
    for (e, te) in [0.004f64, 0.008].into_iter().enumerate() {
        let n = dims.0 * dims.1 * dims.2;
        let (mut mag, mut phase) = (vec![1.0; n], vec![0.0; n]);
        for v in 0..n {
            let (i, j, k) = (v % dims.0, (v / dims.0) % dims.1, v / (dims.0 * dims.1));
            let (x, y, z) = (i as f64 - 10.0, j as f64 - 11.0, k as f64 - 9.0);
            if (x * x + y * y + z * z).sqrt() < 7.0 { mag[v] = 100.0 * (-30.0 * te).exp() + x; }
            phase[v] = (0.05 * (e + 1) as f64 * (x + 0.5 * y)).sin() * 3.0;
        }
        for (part, data) in [("phase", &phase), ("mag", &mag)] {
            let stem = format!("sub-1_echo-{}_part-{part}_MEGRE", e + 1);
            qsm_core::io::save_nifti_to_file(&anat.join(format!("{stem}.nii")), data, dims,
                (1.0, 1.0, 1.0), &affine).unwrap();
            testutils::write_sidecar(&anat.join(format!("{stem}.json")), te, 3.0);
        }
    }
}
