pub mod common;
pub mod bgremove;
pub mod dicom;
pub mod homogeneity;
pub mod init;
pub mod invert;
pub mod mask;
pub mod qsmart;
pub mod quality_map;
pub mod r2star;
pub mod resample;
pub mod run;
pub mod slurm;
pub mod swi;
pub mod t2star;
pub mod unwrap;
pub mod update;
pub mod validate;

#[cfg(test)]
mod integration_tests {
    use crate::cli::*;
    use crate::testutils;
    use std::path::PathBuf;

    fn default_run_args(bids_dir: PathBuf, output_dir: PathBuf) -> RunArgs {
        RunArgs {
            bids_dir,
            output_dir: Some(output_dir),
            config: None,
            include: None,
            exclude: None,
            num_echoes: None,
            qsm_algorithm: None,
            unwrapping_algorithm: None,
            bf_algorithm: None,
            masking_algorithm: None,
            masking_input: None,
            phase_offset_removal: None,
            phase_offset_sigma: None,
            bipolar_correction: false,
            romeo_individual: false,
            no_romeo_individual: false,
            no_romeo_correct_global: false,
            romeo_template: None,
            b0_estimation: None,
            b0_weight_type: None,
            bet_fractional_intensity: None,
            bet_smoothness: None,
            bet_gradient_threshold: None,
            bet_iterations: None,
            bet_subdivisions: None,
            qsm_reference: None,
            mask_erosions: None,
            rts_params: Default::default(),
            tv_params: Default::default(),
            tkd_params: Default::default(),
            tsvd_params: Default::default(),
            tgv_params: Default::default(),
            tikhonov_params: Default::default(),
            nltv_params: Default::default(),
            medi_params: Default::default(),
            ilsqr_params: Default::default(),
            qsmart_params: Default::default(),
            vsharp_params: Default::default(),
            pdf_params: Default::default(),
            lbv_params: Default::default(),
            ismv_params: Default::default(),
            sharp_params: Default::default(),
            resharp_params: Default::default(),
            harperella_params: Default::default(),
            iharperella_params: Default::default(),
            romeo_params: Default::default(),
            swi_params: Default::default(),
            n_procs: Some(1),
            homogeneity_sigma_mm: None,
            homogeneity_nbox: None,
            linear_fit_reliability_threshold: None,
            no_qsm: false,
            do_swi: false,
            do_t2starmap: false,
            do_r2starmap: false,
            export_dicom: false,
            source_dicom: None,
            dicom_outputs: None,
            inhomogeneity_correction: false,
            no_inhomogeneity_correction: false,
            obliquity_threshold: None,
            mask_preset: None,
            mask_sections_cli: None,
            dry: true,
            debug: false,
            mem_limit_gb: None,
            no_mem_limit: false,
            force: false,
            clean_intermediates: false,
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
        }
    }

    fn common_unwrap(input: PathBuf, mask: PathBuf, output: PathBuf) -> UnwrapCommonArgs {
        UnwrapCommonArgs { input, mask, output }
    }

    // --- Mask ---

    #[test]
    fn test_mask_otsu() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mag.nii");
        let output = dir.path().join("mask.nii");
        testutils::write_magnitude(&input);

        super::mask::execute(MaskCommand::Otsu(MaskOtsuArgs {
            common: common_mask(input, output.clone()),
        })).unwrap();
        assert!(output.exists());
    }

    #[test]
    fn test_mask_value_threshold() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mag.nii");
        let output = dir.path().join("mask.nii");
        testutils::write_magnitude(&input);

        let mut c = common_mask(input, output.clone());
        c.ops = vec!["erode:1".to_string()];
        super::mask::execute(MaskCommand::Value(MaskValueArgs {
            common: c,
            threshold: 500.0,
        })).unwrap();
        assert!(output.exists());
    }

    #[test]
    fn test_mask_bet() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mag.nii");
        let output = dir.path().join("mask.nii");
        testutils::write_magnitude(&input);

        super::mask::execute(MaskCommand::Bet(MaskBetArgs {
            common: common_mask(input, output.clone()),
            fractional_intensity: 0.5,
        })).unwrap();
        assert!(output.exists());
    }

    // --- Mask: percentile, robust, erode ---

    #[test]
    fn test_mask_percentile() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mag.nii");
        let output = dir.path().join("mask.nii");
        testutils::write_magnitude(&input);

        super::mask::execute(MaskCommand::Percentile(MaskPercentileArgs {
            common: common_mask(input, output.clone()),
            percentile: 50.0,
        })).unwrap();
        assert!(output.exists());
    }

    #[test]
    fn test_mask_robust() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mag.nii");
        let output = dir.path().join("mask.nii");
        testutils::write_magnitude(&input);

        super::mask::execute(MaskCommand::Robust(MaskRobustArgs {
            input, output: output.clone(),
        })).unwrap();
        assert!(output.exists());
    }

    #[test]
    fn test_mask_otsu_with_ops() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mag.nii");
        let output = dir.path().join("mask.nii");
        testutils::write_magnitude(&input);

        let mut c = common_mask(input, output.clone());
        c.ops = vec!["dilate:1".to_string(), "fill-holes:0".to_string(), "erode:1".to_string()];
        super::mask::execute(MaskCommand::Otsu(MaskOtsuArgs { common: c })).unwrap();
        assert!(output.exists());
    }

    #[test]
    fn test_mask_erode() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mask.nii");
        let output = dir.path().join("eroded.nii");
        testutils::write_mask(&input);

        super::mask::execute(MaskCommand::Erode(MaskErodeArgs {
            input, output: output.clone(), iterations: 1,
        })).unwrap();
        assert!(output.exists());
    }

    // --- Mask morphological operations ---

    #[test]
    fn test_mask_dilate() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mask.nii");
        let output = dir.path().join("dilated.nii");
        testutils::write_mask(&input);

        super::mask::execute(MaskCommand::Dilate(MaskDilateArgs {
            input, output: output.clone(), iterations: 1,
        })).unwrap();
        assert!(output.exists());
    }

    #[test]
    fn test_mask_close() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mask.nii");
        let output = dir.path().join("closed.nii");
        testutils::write_mask(&input);

        super::mask::execute(MaskCommand::Close(MaskCloseArgs {
            input, output: output.clone(), radius: 1,
        })).unwrap();
        assert!(output.exists());
    }

    #[test]
    fn test_mask_fill_holes() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mask.nii");
        let output = dir.path().join("filled.nii");
        testutils::write_mask(&input);

        super::mask::execute(MaskCommand::FillHoles(MaskFillHolesArgs {
            input, output: output.clone(), max_size: 1000,
        })).unwrap();
        assert!(output.exists());
    }

    #[test]
    fn test_mask_smooth() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mask.nii");
        let output = dir.path().join("smoothed.nii");
        testutils::write_mask(&input);

        super::mask::execute(MaskCommand::Smooth(MaskSmoothArgs {
            input, output: output.clone(), sigma: 2.0,
        })).unwrap();
        assert!(output.exists());
    }

    // --- Unwrap ---

    #[test]
    fn test_unwrap_laplacian() {
        let dir = tempfile::tempdir().unwrap();
        let phase = dir.path().join("phase.nii");
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("unwrapped.nii");
        testutils::write_phase(&phase);
        testutils::write_mask(&mask);

        super::unwrap::execute(UnwrapCommand::Laplacian(UnwrapLaplacianArgs {
            common: common_unwrap(phase, mask, output.clone()),
        })).unwrap();
        assert!(output.exists());
    }

    #[test]
    fn test_unwrap_romeo() {
        let dir = tempfile::tempdir().unwrap();
        let phase = dir.path().join("phase.nii");
        let mask = dir.path().join("mask.nii");
        let mag = dir.path().join("mag.nii");
        let output = dir.path().join("unwrapped.nii");
        testutils::write_phase(&phase);
        testutils::write_mask(&mask);
        testutils::write_magnitude(&mag);

        super::unwrap::execute(UnwrapCommand::Romeo(UnwrapRomeoArgs {
            common: common_unwrap(phase, mask, output.clone()),
            magnitude: Some(mag),
            no_phase_gradient_coherence: false,
            no_mag_coherence: false,
            no_mag_weight: false,
        })).unwrap();
        assert!(output.exists());
    }

    // --- Background Removal ---

    #[test]
    fn test_bgremove_vsharp() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("field.nii");
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("local.nii");
        let output_mask = dir.path().join("bgmask.nii");
        testutils::write_field(&input);
        testutils::write_mask(&mask);

        let mut c = common_bgremove(input, mask, output.clone());
        c.output_mask = Some(output_mask.clone());
        super::bgremove::execute(BgremoveCommand::Vsharp(BgremoveVsharpArgs {
            common: c,
            threshold: None, max_radius_factor: None, min_radius_factor: None,
        })).unwrap();
        assert!(output.exists());
        assert!(output_mask.exists());
    }

    #[test]
    fn test_bgremove_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("field.nii");
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("local.nii");
        testutils::write_field(&input);
        testutils::write_mask(&mask);

        super::bgremove::execute(BgremoveCommand::Pdf(BgremovePdfArgs {
            common: common_bgremove(input, mask, output.clone()),
            tol: None,
        })).unwrap();
        assert!(output.exists());
    }

    // --- Dipole Inversion ---

    #[test]
    fn test_invert_tkd() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("field.nii");
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("chi.nii");
        testutils::write_field(&input);
        testutils::write_mask(&mask);

        super::invert::execute(InvertCommand::Tkd(InvertTkdArgs {
            common: common_invert(input, mask, output.clone()),
            threshold: None,
        })).unwrap();
        assert!(output.exists());
    }

    #[test]
    fn test_invert_tgv_requires_field_strength() {
        // TGV has required field_strength and echo_time — this test verifies
        // the struct requires them (they're not Option)
        // Since they're required args in InvertTgvArgs, this is enforced at parse time.
        // We just test that TGV runs with valid params.
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("field.nii");
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("chi.nii");
        testutils::write_field(&input);
        testutils::write_mask(&mask);

        super::invert::execute(InvertCommand::Tgv(InvertTgvArgs {
            common: common_invert(input, mask, output.clone()),
            field_strength: 3.0,
            echo_time: 0.02,
            iterations: Some(5),
            erosions: Some(0),
            alpha1: None, alpha0: None,
            step_size: None, tol: None,
        })).unwrap();
        assert!(output.exists());
    }

    // --- Homogeneity ---

    #[test]
    fn test_homogeneity() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mag.nii");
        let output = dir.path().join("corrected.nii");
        testutils::write_magnitude(&input);

        super::homogeneity::execute(HomogeneityArgs {
            input, output: output.clone(), sigma: 4.0, nbox: 2,
        }).unwrap();
        assert!(output.exists());
    }

    // --- Resample ---

    #[test]
    fn test_resample() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("vol.nii");
        let output = dir.path().join("resampled.nii");
        testutils::write_magnitude(&input);

        super::resample::execute(ResampleArgs {
            input, output: output.clone(),
        }).unwrap();
        assert!(output.exists());
    }

    // --- Quality Map ---

    #[test]
    fn test_quality_map() {
        let dir = tempfile::tempdir().unwrap();
        let phase = dir.path().join("phase.nii");
        let output = dir.path().join("quality.nii");
        testutils::write_phase(&phase);

        super::quality_map::execute(QualityMapArgs {
            phase, output: output.clone(),
            magnitude: None, phase2: None, te1: 0.02, te2: 0.04,
        }).unwrap();
        assert!(output.exists());
    }

    // --- SWI ---

    #[test]
    fn test_swi() {
        let dir = tempfile::tempdir().unwrap();
        let phase = dir.path().join("phase.nii");
        let mag = dir.path().join("mag.nii");
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("swi.nii");
        testutils::write_phase(&phase);
        testutils::write_magnitude(&mag);
        testutils::write_mask(&mask);

        super::swi::execute(SwiArgs {
            phase, magnitude: mag, mask,
            output: output.clone(),
            mip: false, mip_output: None,
            swi_params: Default::default(),
        }).unwrap();
        assert!(output.exists());
    }

    // --- R2* ---

    #[test]
    fn test_r2star() {
        let dir = tempfile::tempdir().unwrap();
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("r2star.nii");
        testutils::write_mask(&mask);

        let mut inputs = Vec::new();
        for i in 1..=3 {
            let p = dir.path().join(format!("echo{}.nii", i));
            testutils::write_magnitude(&p);
            inputs.push(p);
        }

        super::r2star::execute(R2starArgs {
            inputs, mask, output: output.clone(),
            echo_times: vec![0.004, 0.008, 0.012],
        }).unwrap();
        assert!(output.exists());
    }

    #[test]
    fn test_r2star_mismatched_inputs() {
        let result = super::r2star::execute(R2starArgs {
            inputs: vec![PathBuf::from("a.nii"), PathBuf::from("b.nii"), PathBuf::from("c.nii")],
            mask: PathBuf::from("mask.nii"),
            output: PathBuf::from("out.nii"),
            echo_times: vec![0.004, 0.008],
        });
        assert!(result.is_err());
    }

    // --- T2* ---

    #[test]
    fn test_t2star() {
        let dir = tempfile::tempdir().unwrap();
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("t2star.nii");
        testutils::write_mask(&mask);

        let mut inputs = Vec::new();
        for i in 1..=3 {
            let p = dir.path().join(format!("echo{}.nii", i));
            testutils::write_magnitude(&p);
            inputs.push(p);
        }

        super::t2star::execute(T2starArgs {
            inputs, mask, output: output.clone(),
            echo_times: vec![0.004, 0.008, 0.012],
        }).unwrap();
        assert!(output.exists());
    }

    // --- Init ---

    #[test]
    fn test_init_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("config.toml");
        super::init::execute(InitArgs { output: Some(output.clone()) }).unwrap();
        assert!(output.exists());
        let content = std::fs::read_to_string(&output).unwrap();
        assert!(content.contains("[inversion]"));
    }

    #[test]
    fn test_init_to_stdout() {
        super::init::execute(InitArgs { output: None }).unwrap();
    }

    // --- Validate ---

    #[test]
    fn test_validate_single_echo() {
        let dir = tempfile::tempdir().unwrap();
        testutils::create_single_echo_bids(dir.path());
        super::validate::execute(ValidateArgs {
            bids_dir: dir.path().to_path_buf(), include: None, exclude: None,
        }).unwrap();
    }

    #[test]
    fn test_validate_multi_echo() {
        let dir = tempfile::tempdir().unwrap();
        testutils::create_multi_echo_bids(dir.path());
        super::validate::execute(ValidateArgs {
            bids_dir: dir.path().to_path_buf(), include: None, exclude: None,
        }).unwrap();
    }

    #[test]
    fn test_validate_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        super::validate::execute(ValidateArgs {
            bids_dir: dir.path().to_path_buf(), include: None, exclude: None,
        }).unwrap();
    }

    // --- Run (dry) ---

    #[test]
    fn test_run_dry_single_echo() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_single_echo_bids(&bids);

        let mut args = default_run_args(bids, out);
        args.mem_limit_gb = Some(4.0);
        super::run::execute(args).unwrap();
    }

    #[test]
    fn test_run_dry_multi_echo() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_multi_echo_bids(&bids);

        let mut args = default_run_args(bids, out);
        args.qsm_algorithm = Some(QsmAlgorithmArg::Tkd);
        args.no_mem_limit = true;
        super::run::execute(args).unwrap();
    }

    #[test]
    fn test_run_dry_empty_bids() {
        let dir = tempfile::tempdir().unwrap();
        let args = default_run_args(dir.path().to_path_buf(), dir.path().join("out"));
        super::run::execute(args).unwrap();
    }

    // --- Run (actual execution) ---

    #[test]
    fn test_run_single_echo_tkd() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_single_echo_bids(&bids);

        let mut args = default_run_args(bids, out.clone());
        args.qsm_algorithm = Some(QsmAlgorithmArg::Tkd);
        args.unwrapping_algorithm = Some(UnwrapAlgorithmArg::Laplacian);
        args.bf_algorithm = Some(BfAlgorithmArg::Vsharp);
        args.masking_algorithm = Some(MaskAlgorithmArg::Threshold);
        args.masking_input = Some(MaskInputArg::MagnitudeFirst);
        args.mask_erosions = Some(vec![1]);
        args.dry = false;
        args.no_mem_limit = true;
        super::run::execute(args).unwrap();

        let deriv = out.join("derivatives/qsmxt.rs");
        assert!(deriv.join("sub-1/anat/sub-1_Chimap.nii").exists());
        assert!(deriv.join("sub-1/anat/sub-1_mask.nii").exists());
        assert!(deriv.join("sub-1/anat/sub-1_magnitude.nii").exists());
        assert!(deriv.join("pipeline_config.toml").exists());
    }

    #[test]
    fn test_run_caching() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_single_echo_bids(&bids);

        let make_args = || {
            let mut args = default_run_args(bids.clone(), out.clone());
            args.qsm_algorithm = Some(QsmAlgorithmArg::Tkd);
            args.unwrapping_algorithm = Some(UnwrapAlgorithmArg::Laplacian);
            args.bf_algorithm = Some(BfAlgorithmArg::Vsharp);
            args.masking_algorithm = Some(MaskAlgorithmArg::Threshold);
            args.masking_input = Some(MaskInputArg::MagnitudeFirst);
            args.mask_erosions = Some(vec![1]);
            args.dry = false;
            args.no_mem_limit = true;
            args
        };

        // First run: produces all outputs
        super::run::execute(make_args()).unwrap();

        let deriv = out.join("derivatives/qsmxt.rs");
        let qsm = deriv.join("sub-1/anat/sub-1_Chimap.nii");
        let mask = deriv.join("sub-1/anat/sub-1_mask.nii");
        let mag = deriv.join("sub-1/anat/sub-1_magnitude.nii");
        assert!(qsm.exists());
        assert!(mask.exists());
        assert!(mag.exists());

        // Record modification times
        let qsm_mtime = std::fs::metadata(&qsm).unwrap().modified().unwrap();
        let mask_mtime = std::fs::metadata(&mask).unwrap().modified().unwrap();

        // Brief pause so any re-written files would have a different mtime
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Second run: should skip all cached steps
        super::run::execute(make_args()).unwrap();

        // Outputs should still exist
        assert!(qsm.exists());
        assert!(mask.exists());
        assert!(mag.exists());

        // Files should NOT have been rewritten (mtimes unchanged)
        assert_eq!(std::fs::metadata(&qsm).unwrap().modified().unwrap(), qsm_mtime);
        assert_eq!(std::fs::metadata(&mask).unwrap().modified().unwrap(), mask_mtime);
    }

    #[test]
    fn test_run_caching_survives_unrelated_config_change() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_multi_echo_bids(&bids);

        let make_args = |do_t2star: bool| {
            let mut args = default_run_args(bids.clone(), out.clone());
            args.qsm_algorithm = Some(QsmAlgorithmArg::Tkd);
            args.unwrapping_algorithm = Some(UnwrapAlgorithmArg::Laplacian);
            args.bf_algorithm = Some(BfAlgorithmArg::Vsharp);
            args.masking_algorithm = Some(MaskAlgorithmArg::Threshold);
            args.masking_input = Some(MaskInputArg::Magnitude);
            args.mask_erosions = Some(vec![1]);
            args.dry = false;
            args.no_mem_limit = true;
            args.do_t2starmap = do_t2star;
            args
        };

        // First run: QSM only (no T2*)
        super::run::execute(make_args(false)).unwrap();

        let deriv = out.join("derivatives/qsmxt.rs");
        let qsm = deriv.join("sub-1/anat/sub-1_Chimap.nii");
        assert!(qsm.exists());
        let qsm_mtime = std::fs::metadata(&qsm).unwrap().modified().unwrap();

        std::thread::sleep(std::time::Duration::from_millis(50));

        // Second run: add --do-t2starmap — should reuse cached QSM steps
        super::run::execute(make_args(true)).unwrap();

        // QSM should not have been rewritten
        assert_eq!(std::fs::metadata(&qsm).unwrap().modified().unwrap(), qsm_mtime);
        // T2* should now exist
        assert!(deriv.join("sub-1/anat/sub-1_T2starmap.nii").exists());
    }

    #[test]
    fn test_run_multi_echo_with_extras() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_multi_echo_bids(&bids);

        let mut args = default_run_args(bids, out.clone());
        args.qsm_algorithm = Some(QsmAlgorithmArg::Tkd);
        args.unwrapping_algorithm = Some(UnwrapAlgorithmArg::Laplacian);
        args.bf_algorithm = Some(BfAlgorithmArg::Vsharp);
        args.masking_algorithm = Some(MaskAlgorithmArg::Threshold);
        args.masking_input = Some(MaskInputArg::Magnitude);
        args.phase_offset_removal = Some(true);
        args.mask_erosions = Some(vec![1]);
        args.dry = false;
        args.no_mem_limit = true;
        args.do_swi = true;
        args.do_t2starmap = true;
        args.do_r2starmap = true;
        super::run::execute(args).unwrap();

        let deriv = out.join("derivatives/qsmxt.rs");
        assert!(deriv.join("sub-1/anat/sub-1_Chimap.nii").exists());
        assert!(deriv.join("sub-1/anat/sub-1_swi.nii").exists());
        assert!(deriv.join("sub-1/anat/sub-1_T2starmap.nii").exists());
        assert!(deriv.join("sub-1/anat/sub-1_R2starmap.nii").exists());
    }

    #[test]
    fn test_run_single_echo_tgv() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_single_echo_bids(&bids);

        let mut args = default_run_args(bids, out.clone());
        args.qsm_algorithm = Some(QsmAlgorithmArg::Tgv);
        args.masking_input = Some(MaskInputArg::MagnitudeFirst);
        args.mask_erosions = Some(vec![0]);
        args.tgv_params.tgv_iterations = Some(5);
        args.tgv_params.tgv_erosions = Some(0);
        args.dry = false;
        args.no_mem_limit = true;
        args.inhomogeneity_correction = true;
        args.mask_sections_cli = Some(vec!["phase-quality,threshold:otsu".to_string()]);
        super::run::execute(args).unwrap();

        assert!(out.join("derivatives/qsmxt.rs/sub-1/anat/sub-1_Chimap.nii").exists());
    }

    #[test]
    fn test_run_with_mask_ops() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_single_echo_bids(&bids);

        let mut args = default_run_args(bids, out.clone());
        args.qsm_algorithm = Some(QsmAlgorithmArg::Tkd);
        args.unwrapping_algorithm = Some(UnwrapAlgorithmArg::Laplacian);
        args.bf_algorithm = Some(BfAlgorithmArg::Vsharp);
        args.dry = false;
        args.no_mem_limit = true;
        args.clean_intermediates = true;
        args.mask_sections_cli = Some(vec!["phase-quality,threshold:otsu,dilate:1,erode:1".to_string()]);
        super::run::execute(args).unwrap();

        assert!(out.join("derivatives/qsmxt.rs/sub-1/anat/sub-1_Chimap.nii").exists());
    }

    // --- Invert algorithms ---

    #[test]
    fn test_invert_rts() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("field.nii");
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("chi.nii");
        testutils::write_field(&input);
        testutils::write_mask(&mask);

        super::invert::execute(InvertCommand::Rts(InvertRtsArgs {
            common: common_invert(input, mask, output.clone()),
            delta: None, mu: None,
            tol: Some(0.5), // loose tolerance for speed
            rho: None, max_iter: None, lsmr_iter: None,
        })).unwrap();
        assert!(output.exists());
    }

    #[test]
    fn test_invert_tv() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("field.nii");
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("chi.nii");
        testutils::write_field(&input);
        testutils::write_mask(&mask);

        super::invert::execute(InvertCommand::Tv(InvertTvArgs {
            common: common_invert(input, mask, output.clone()),
            lambda: None, rho: None, tol: None, max_iter: None,
        })).unwrap();
        assert!(output.exists());
    }

    #[test]
    fn test_invert_tgv() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("field.nii");
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("chi.nii");
        testutils::write_field(&input);
        testutils::write_mask(&mask);

        super::invert::execute(InvertCommand::Tgv(InvertTgvArgs {
            common: common_invert(input, mask, output.clone()),
            field_strength: 3.0,
            echo_time: 0.02,
            iterations: Some(5),
            erosions: Some(0),
            alpha1: None, alpha0: None,
            step_size: None, tol: None,
        })).unwrap();
        assert!(output.exists());
    }

    // --- SWI with MIP ---

    #[test]
    fn test_swi_with_mip() {
        let dir = tempfile::tempdir().unwrap();
        let phase = dir.path().join("phase.nii");
        let mag = dir.path().join("mag.nii");
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("swi.nii");
        let mip = dir.path().join("mip.nii");
        testutils::write_phase(&phase);
        testutils::write_magnitude(&mag);
        testutils::write_mask(&mask);

        super::swi::execute(SwiArgs {
            phase, magnitude: mag, mask,
            output: output.clone(),
            mip: true, mip_output: Some(mip.clone()),
            swi_params: Default::default(),
        }).unwrap();
        assert!(output.exists());
        assert!(mip.exists());
    }

    // --- Quality map with all optional inputs ---

    #[test]
    fn test_quality_map_with_magnitude_and_phase2() {
        let dir = tempfile::tempdir().unwrap();
        let phase = dir.path().join("phase.nii");
        let mag = dir.path().join("mag.nii");
        let phase2 = dir.path().join("phase2.nii");
        let output = dir.path().join("quality.nii");
        testutils::write_phase(&phase);
        testutils::write_magnitude(&mag);
        testutils::write_phase(&phase2);

        super::quality_map::execute(QualityMapArgs {
            phase, output: output.clone(),
            magnitude: Some(mag), phase2: Some(phase2),
            te1: 0.004, te2: 0.008,
        }).unwrap();
        assert!(output.exists());
    }

    // --- Bgremove remaining algorithms ---

    #[test]
    fn test_bgremove_lbv() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("field.nii");
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("local.nii");
        testutils::write_field(&input);
        testutils::write_mask(&mask);

        super::bgremove::execute(BgremoveCommand::Lbv(BgremoveLbvArgs {
            common: common_bgremove(input, mask, output.clone()),
            tol: None,
        })).unwrap();
        assert!(output.exists());
    }

    #[test]
    fn test_bgremove_ismv() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("field.nii");
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("local.nii");
        testutils::write_field(&input);
        testutils::write_mask(&mask);

        super::bgremove::execute(BgremoveCommand::Ismv(BgremoveIsmvArgs {
            common: common_bgremove(input, mask, output.clone()),
            tol: None, max_iter: None, radius_factor: None,
        })).unwrap();
        assert!(output.exists());
    }

    #[test]
    fn test_bgremove_sharp() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("field.nii");
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("local.nii");
        testutils::write_field(&input);
        testutils::write_mask(&mask);

        super::bgremove::execute(BgremoveCommand::Sharp(BgremoveSharpArgs {
            common: common_bgremove(input, mask, output.clone()),
            threshold: None, radius_factor: None,
        })).unwrap();
        assert!(output.exists());
    }

    // --- Slurm command ---

    #[test]
    fn test_slurm_command() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_single_echo_bids(&bids);

        super::slurm::execute(SlurmArgs {
            bids_dir: bids,
            output_dir: Some(out.clone()),
            account: "testacct".to_string(),
            partition: Some("gpu".to_string()),
            config: None,
            time: "01:00:00".to_string(),
            mem: 16, cpus_per_task: 2, submit: false,
            include: None, exclude: None, num_echoes: None,
        }).unwrap();

        assert!(out.join("derivatives/qsmxt.rs/slurm").exists());
    }

    #[test]
    fn test_validate_multi_session() {
        let dir = tempfile::tempdir().unwrap();
        testutils::create_multi_session_bids(dir.path());
        super::validate::execute(ValidateArgs {
            bids_dir: dir.path().to_path_buf(), include: None, exclude: None,
        }).unwrap();
    }
}
