pub mod common;
pub mod bgremove;
pub mod combine;
pub mod dicom;
pub mod example;
pub mod fieldmap;
pub mod homogeneity;
pub mod init;
pub mod invert;
pub mod mask;
pub mod qsmart;
pub mod quality_map;
pub mod r2star;
pub mod r2;
pub mod r2prime;
pub mod separate;
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
    use std::path::{Path, PathBuf};

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
            pipeline: PipelineArgs {
                qsm_algorithm: None,
                unwrapping_algorithm: None,
                bf_algorithm: None,
                masking_input: None,
                phase_offset_removal: None,
                phase_offset_sigma: None,
                coil_combination_sigma: None,
                bipolar_correction: false,
                b0_estimation: None,
                b0_weight_type: None,
                bet_fractional_intensity: None,
                bet_smoothness: None,
                bet_gradient_threshold: None,
                bet_iterations: None,
                bet_subdivisions: None,
                qsm_reference: None,
                rts_params: Default::default(),
                tv_params: Default::default(),
                tkd_params: Default::default(),
                tsvd_params: Default::default(),
                tgv_params: Default::default(),
                tikhonov_params: Default::default(),
                nltv_params: Default::default(),
                medi_params: Default::default(),
                tfi_params: Default::default(),
                ilsqr_params: Default::default(),
                qsmart_params: Default::default(),
                ndi_params: Default::default(),
                fansi_params: Default::default(),
                l1qsm_params: Default::default(),
                whqsm_params: Default::default(),
                hdqsm_params: Default::default(),
                amp_pe_params: Default::default(),
                separation_params: Default::default(),
                vsharp_params: Default::default(),
                pdf_params: Default::default(),
                lbv_params: Default::default(),
                ismv_params: Default::default(),
                sharp_params: Default::default(),
                resharp_params: Default::default(),
                harperella_params: Default::default(),
                iharperella_params: Default::default(),
                msmv_params: Default::default(),
                romeo_params: Default::default(),
                swi_params: Default::default(),
                tiling_params: Default::default(),
                homogeneity_sigma_mm: None,
                homogeneity_nbox: None,
                linear_fit_reliability_threshold: None,
                linear_fit_estimate_offset: None,
                no_qsm: false,
                do_swi: false,
                do_t2starmap: false,
                do_r2starmap: false,
                do_r2map: false,
                do_r2primemap: false,
                do_chi_separation: false,
                chi_separation_algorithm: None,
                use_custom_qsm: None,
                use_custom_r2: None,
                use_custom_r2prime: None,
                export_dicom: false,
                inhomogeneity_correction: false,
                no_inhomogeneity_correction: false,
                obliquity_threshold: None,
                crop_to_mask: false,
                fft_padding: false,
                crop_margin_mm: None,
                output_space: None,
                mask_preset: None,
                use_custom_masks: None,
                mask_sections_cli: None,
                mask_combine: None,
                mask_refinements_cli: None,
                two_pass: false,
                two_pass_sections_cli: None,
            },
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

    #[test]
    fn test_mask_signal_erode_op() {
        // signal-erode refines a threshold mask using the (magnitude) input image.
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mag.nii");
        let output = dir.path().join("mask.nii");
        testutils::write_magnitude(&input);
        let mut c = common_mask(input, output.clone());
        c.ops = vec!["signal-erode:0.8:2:0:4:1".to_string()];
        super::mask::execute(MaskCommand::Otsu(MaskOtsuArgs { common: c })).unwrap();
        assert!(output.exists());
    }

    fn read_mask(path: &Path) -> Vec<u8> {
        super::common::load_mask(path).expect("read mask").0
    }

    /// `mask and` / `mask or` are set intersection and union, voxel for voxel.
    #[test]
    fn test_mask_and_or() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.nii"), dir.path().join("b.nii"));
        // Overlapping halves, so neither is a subset of the other.
        let n = testutils::N_VOXELS;
        testutils::write_mask_where(&a, |i| i < n * 2 / 3);
        testutils::write_mask_where(&b, |i| i >= n / 3);

        let combine = |mode: fn(crate::cli::MaskCombineCliArgs) -> MaskCommand, out: &Path, ops: Vec<String>| {
            let cmd = crate::cli::MaskCombineCliArgs {
                inputs: vec![a.clone(), b.clone()], output: out.to_path_buf(), ops, magnitude: None,
            };
            super::mask::execute(mode(cmd)).unwrap();
            read_mask(out)
        };

        let and = combine(MaskCommand::And, &dir.path().join("and.nii"), vec![]);
        let or = combine(MaskCommand::Or, &dir.path().join("or.nii"), vec![]);
        let (ma, mb) = (read_mask(&a), read_mask(&b));
        for i in 0..n {
            assert_eq!(and[i], ma[i] & mb[i], "voxel {i}");
            assert_eq!(or[i], ma[i] | mb[i], "voxel {i}");
        }
        let count = |m: &[u8]| m.iter().map(|&v| v as usize).sum::<usize>();
        assert_eq!(count(&and) + count(&or), count(&ma) + count(&mb), "inclusion-exclusion");

        // --op runs on the combined mask, not on the inputs.
        let eroded = combine(MaskCommand::And, &dir.path().join("e.nii"), vec!["erode:1".to_string()]);
        assert!(count(&eroded) < count(&and), "erode:1 did not shrink the combined mask");
    }

    /// Masks on different grids cannot be combined, and say so by name.
    #[test]
    fn test_mask_combine_rejects_mismatched_grids() {
        let dir = tempfile::tempdir().unwrap();
        let (a, small) = (dir.path().join("a.nii"), dir.path().join("small.nii"));
        testutils::write_mask_where(&a, |_| true);
        testutils::write_mismatched_volume(&small, 1.0);

        let err = super::mask::execute(MaskCommand::And(crate::cli::MaskCombineCliArgs {
            inputs: vec![a, small], output: dir.path().join("out.nii"), ops: vec![], magnitude: None,
        })).unwrap_err();
        assert!(format!("{err}").contains("same grid"), "{err}");
    }

    /// signal-erode after a combine needs a magnitude, and `--magnitude` is how it gets one.
    #[test]
    fn test_mask_combine_signal_erode_needs_magnitude() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.nii"), dir.path().join("b.nii"));
        testutils::write_mask_where(&a, |_| true);
        testutils::write_mask_where(&b, |_| true);
        let args = |magnitude| crate::cli::MaskCombineCliArgs {
            inputs: vec![a.clone(), b.clone()], output: dir.path().join("out.nii"),
            ops: vec!["signal-erode:0.8:2:0:4:1".to_string()], magnitude,
        };

        let err = super::mask::execute(MaskCommand::And(args(None))).unwrap_err();
        assert!(format!("{err}").contains("--magnitude"), "{err}");

        let mag = dir.path().join("mag.nii");
        testutils::write_magnitude(&mag);
        super::mask::execute(MaskCommand::And(args(Some(mag)))).unwrap();
        assert!(dir.path().join("out.nii").exists());
    }

    #[test]
    fn test_mask_generator_op_is_rejected() {
        // A generator passed as --op used to be silently ignored.
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mag.nii");
        testutils::write_magnitude(&input);
        for op in ["bet:0.5", "hd-bet", "threshold:otsu"] {
            let mut c = common_mask(input.clone(), dir.path().join("mask.nii"));
            c.ops = vec![op.to_string()];
            let err = super::mask::execute(MaskCommand::Otsu(MaskOtsuArgs { common: c })).unwrap_err();
            assert!(format!("{err}").contains("creates a mask"), "{op}: {err}");
        }
    }

    #[cfg(not(feature = "dl"))]
    #[test]
    fn test_mask_hd_bet_needs_dl() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mag.nii");
        testutils::write_magnitude(&input);
        let err = super::mask::execute(MaskCommand::HdBet(MaskHdBetArgs {
            common: common_mask(input, dir.path().join("mask.nii")),
            low_memory: false, patch: None, tta: false,
        })).unwrap_err();
        assert!(format!("{err}").contains("deep-learning"), "{err}");
    }

    /// Runs HD-BET for real (downloads the ~120 MB weights on first use).
    #[cfg(feature = "dl")]
    #[test]
    #[ignore]
    fn test_mask_hd_bet() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mag.nii");
        let output = dir.path().join("mask.nii");
        testutils::write_magnitude(&input);
        let mut c = common_mask(input, output.clone());
        c.ops = vec!["signal-erode".to_string()];
        super::mask::execute(MaskCommand::HdBet(MaskHdBetArgs {
            common: c, low_memory: true, patch: None, tta: false, tile_step: None,
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
            common: common_mask(input.clone(), output.clone()),
        })).unwrap();
        assert!(output.exists());

        // `robust` is `preset robust-threshold`, not a third copy of the recipe.
        let via_preset = dir.path().join("preset.nii");
        super::mask::execute(MaskCommand::Preset(MaskPresetArgs {
            preset: MaskPresetArg::RobustThreshold, common: common_mask(input, via_preset.clone()), quality: None,
        })).unwrap();
        assert_eq!(read_mask(&output), read_mask(&via_preset));
    }

    /// A multi-section preset from files: BET on the input AND Otsu on `--quality`, then the
    /// recipe's own refinements. Without `--quality` the phase-quality section reads the input.
    #[test]
    fn test_mask_preset_bet_and_phase() {
        let dir = tempfile::tempdir().unwrap();
        let (mag, quality) = (dir.path().join("mag.nii"), dir.path().join("quality.nii"));
        testutils::write_magnitude(&mag);
        // A quality map that is 1 everywhere except one dark slice, so the AND has something to remove.
        testutils::write_mask_where(&quality, |i| i >= 64);

        let with_quality = dir.path().join("with.nii");
        super::mask::execute(MaskCommand::Preset(MaskPresetArgs {
            preset: MaskPresetArg::BetAndPhase, common: common_mask(mag.clone(), with_quality.clone()),
            quality: Some(quality.clone()),
        })).unwrap();
        let without = dir.path().join("without.nii");
        super::mask::execute(MaskCommand::Preset(MaskPresetArgs {
            preset: MaskPresetArg::BetAndPhase, common: common_mask(mag.clone(), without.clone()), quality: None,
        })).unwrap();
        let (w, wo) = (read_mask(&with_quality), read_mask(&without));
        assert_ne!(w, wo, "--quality was not read");
        assert!(w[..64].iter().all(|&v| v == 0), "the dark slice of the quality map is masked out");
        assert!(w.contains(&1), "the AND of BET and the quality map is not empty");

        // --quality must be on the input's grid.
        let small = dir.path().join("small.nii");
        testutils::write_mismatched_volume(&small, 1.0);
        let err = super::mask::execute(MaskCommand::Preset(MaskPresetArgs {
            preset: MaskPresetArg::BetAndPhase, common: common_mask(mag, dir.path().join("x.nii")),
            quality: Some(small),
        })).unwrap_err();
        assert!(format!("{err}").contains("--quality"), "{err}");
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
            common: common_mask(input.clone(), output.clone()), iterations: 1,
        })).unwrap();
        assert!(output.exists());

        // Every subcommand is the first link of a chain: --op runs after its own operation.
        let count = |p: &std::path::Path| read_mask(p).iter().map(|&v| v as usize).sum::<usize>();
        let chained = dir.path().join("chained.nii");
        let mut c = common_mask(input, chained.clone());
        c.ops = vec!["erode:1".to_string()];
        super::mask::execute(MaskCommand::Erode(MaskErodeArgs { common: c, iterations: 1 })).unwrap();
        assert!(count(&chained) < count(&output), "erode --op erode:1 erodes twice");
    }

    // --- Mask morphological operations ---

    #[test]
    fn test_mask_dilate() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mask.nii");
        let output = dir.path().join("dilated.nii");
        testutils::write_mask(&input);

        super::mask::execute(MaskCommand::Dilate(MaskDilateArgs {
            common: common_mask(input, output.clone()), iterations: 1,
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
            common: common_mask(input, output.clone()), radius: 1,
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
            common: common_mask(input, output.clone()), max_size: 0,
        })).unwrap();
        assert!(output.exists());
        // The bare op agrees with the subcommand default and the presets: 0 = automatic.
        assert_eq!(crate::pipeline::config::parse_mask_op("fill-holes").unwrap(),
                   crate::pipeline::config::MaskOp::FillHoles { max_size: 0 });
    }

    #[test]
    fn test_mask_smooth() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mask.nii");
        let output = dir.path().join("smoothed.nii");
        testutils::write_mask(&input);

        super::mask::execute(MaskCommand::Smooth(MaskSmoothArgs {
            common: common_mask(input, output.clone()), sigma: 2.0,
        })).unwrap();
        assert!(output.exists());

        // Binarises at 0.5 like every other subcommand — it used to take anything above 0.
        let faint = dir.path().join("faint.nii");
        let n = testutils::N_VOXELS;
        qsm_core::io::save_nifti_to_file(&faint, &vec![0.3; n], (8, 8, 8), (1.0, 1.0, 1.0),
            &[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]).unwrap();
        let out = dir.path().join("faint_smoothed.nii");
        super::mask::execute(MaskCommand::Smooth(MaskSmoothArgs {
            common: common_mask(faint, out.clone()), sigma: 1.0,
        })).unwrap();
        assert!(read_mask(&out).iter().all(|&v| v == 0), "0.3 everywhere is not a mask");
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
            threshold: None, max_radius: None, min_radius: None,
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
            input: Some(input), output: Some(output.clone()),
            phase: None, magnitude: None, phase_out: None, magnitude_out: None,
            no_noise_fill: false,
        }).unwrap();
        assert!(output.exists());
    }

    /// Wrapped phase goes through the complex domain, so it needs its magnitude alongside.
    #[test]
    fn test_resample_phase_with_magnitude() {
        let dir = tempfile::tempdir().unwrap();
        let phase = dir.path().join("phase.nii");
        let magnitude = dir.path().join("mag.nii");
        let phase_out = dir.path().join("phase_axial.nii");
        let magnitude_out = dir.path().join("mag_axial.nii");
        testutils::write_phase(&phase);
        testutils::write_magnitude(&magnitude);

        super::resample::execute(ResampleArgs {
            input: None, output: None,
            phase: Some(phase), magnitude: Some(magnitude),
            phase_out: Some(phase_out.clone()), magnitude_out: Some(magnitude_out.clone()),
            no_noise_fill: true,
        }).unwrap();
        assert!(phase_out.exists() && magnitude_out.exists());
    }

    #[test]
    fn test_resample_requires_an_input() {
        let err = super::resample::execute(ResampleArgs {
            input: None, output: None, phase: None, magnitude: None,
            phase_out: None, magnitude_out: None, no_noise_fill: false,
        });
        assert!(err.is_err(), "no input should be an error, not a panic");
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
        args.pipeline.qsm_algorithm = Some(QsmAlgorithmArg::Tkd);
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
        args.pipeline.qsm_algorithm = Some(QsmAlgorithmArg::Tkd);
        args.pipeline.unwrapping_algorithm = Some(UnwrapAlgorithmArg::Laplacian);
        args.pipeline.bf_algorithm = Some(BfAlgorithmArg::Vsharp);
        args.pipeline.masking_input = Some(MaskInputArg::MagnitudeFirst);
        args.dry = false;
        args.no_mem_limit = true;
        super::run::execute(args).unwrap();

        let deriv = out.join("derivatives/qsmxt");
        assert!(deriv.join("sub-1/anat/sub-1_Chimap.nii").exists());
        assert!(deriv.join("sub-1/anat/sub-1_mask.nii").exists());
        assert!(deriv.join("sub-1/anat/sub-1_part-mag_T2starw.nii").exists());
        assert!(deriv.join("pipeline_config.toml").exists());
        assert!(deriv.join("dataset_description.json").exists());

        // BEP028 (BIDS-Prov) records
        assert!(deriv.join("prov/prov-qsmxt_base.json").exists());
        assert!(deriv.join("prov/prov-qsmxt_soft.json").exists());
        assert!(deriv.join("prov/prov-qsmxt_env.json").exists());
        assert!(deriv.join("prov/prov-qsmxt_act.json").exists());
        assert!(deriv.join("prov/prov-qsmxt_ent.json").exists());

        // The final QSM map has a GeneratedBy sidecar referencing the
        // reference-step activity that produced it.
        let sidecar: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(deriv.join("sub-1/anat/sub-1_Chimap.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(sidecar["GeneratedBy"], "bids::prov/#reference-sub-1");
        // SkullStripped is required for derivative anat images; the QSM map is
        // referenced within the brain mask.
        assert_eq!(sidecar["SkullStripped"], true);
        assert!(deriv.join(".bidsignore").exists());

        // The referenced activity exists in the activity records.
        let act: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(deriv.join("prov/prov-qsmxt_act.json")).unwrap(),
        )
        .unwrap();
        assert!(act.get("bids::prov/#reference-sub-1").is_some());
    }

    #[test]
    fn test_run_caching() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_single_echo_bids(&bids);

        let make_args = || {
            let mut args = default_run_args(bids.clone(), out.clone());
            args.pipeline.qsm_algorithm = Some(QsmAlgorithmArg::Tkd);
            args.pipeline.unwrapping_algorithm = Some(UnwrapAlgorithmArg::Laplacian);
            args.pipeline.bf_algorithm = Some(BfAlgorithmArg::Vsharp);
            args.pipeline.masking_input = Some(MaskInputArg::MagnitudeFirst);
            args.dry = false;
            args.no_mem_limit = true;
            args
        };

        // First run: produces all outputs
        super::run::execute(make_args()).unwrap();

        let deriv = out.join("derivatives/qsmxt");
        let qsm = deriv.join("sub-1/anat/sub-1_Chimap.nii");
        let mask = deriv.join("sub-1/anat/sub-1_mask.nii");
        let mag = deriv.join("sub-1/anat/sub-1_part-mag_T2starw.nii");
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
            args.pipeline.qsm_algorithm = Some(QsmAlgorithmArg::Tkd);
            args.pipeline.unwrapping_algorithm = Some(UnwrapAlgorithmArg::Laplacian);
            args.pipeline.bf_algorithm = Some(BfAlgorithmArg::Vsharp);
            args.pipeline.masking_input = Some(MaskInputArg::Magnitude);
            args.dry = false;
            args.no_mem_limit = true;
            args.pipeline.do_t2starmap = do_t2star;
            args
        };

        // First run: QSM only (no T2*)
        super::run::execute(make_args(false)).unwrap();

        let deriv = out.join("derivatives/qsmxt");
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
        args.pipeline.qsm_algorithm = Some(QsmAlgorithmArg::Tkd);
        args.pipeline.unwrapping_algorithm = Some(UnwrapAlgorithmArg::Laplacian);
        args.pipeline.bf_algorithm = Some(BfAlgorithmArg::Vsharp);
        args.pipeline.masking_input = Some(MaskInputArg::Magnitude);
        args.pipeline.phase_offset_removal = Some(true);
        args.dry = false;
        args.no_mem_limit = true;
        args.pipeline.do_swi = true;
        args.pipeline.do_t2starmap = true;
        args.pipeline.do_r2starmap = true;
        super::run::execute(args).unwrap();

        let deriv = out.join("derivatives/qsmxt");
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
        args.pipeline.qsm_algorithm = Some(QsmAlgorithmArg::Tgv);
        args.pipeline.masking_input = Some(MaskInputArg::MagnitudeFirst);
        args.pipeline.tgv_params.tgv_iterations = Some(5);
        args.pipeline.tgv_params.tgv_erosions = Some(0);
        args.dry = false;
        args.no_mem_limit = true;
        args.pipeline.inhomogeneity_correction = true;
        args.pipeline.mask_sections_cli = Some(vec!["phase-quality,threshold:otsu".to_string()]);
        super::run::execute(args).unwrap();

        assert!(out.join("derivatives/qsmxt/sub-1/anat/sub-1_Chimap.nii").exists());
    }

    #[test]
    fn test_run_with_mask_ops() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_single_echo_bids(&bids);

        let mut args = default_run_args(bids, out.clone());
        args.pipeline.qsm_algorithm = Some(QsmAlgorithmArg::Tkd);
        args.pipeline.unwrapping_algorithm = Some(UnwrapAlgorithmArg::Laplacian);
        args.pipeline.bf_algorithm = Some(BfAlgorithmArg::Vsharp);
        args.dry = false;
        args.no_mem_limit = true;
        args.clean_intermediates = true;
        args.pipeline.mask_sections_cli = Some(vec!["phase-quality,threshold:otsu,dilate:1,erode:1".to_string()]);
        super::run::execute(args).unwrap();

        assert!(out.join("derivatives/qsmxt/sub-1/anat/sub-1_Chimap.nii").exists());
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
            tol: None, max_iter: None, radius: None,
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
            threshold: None, radius: None,
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
            pipeline: Default::default(),
        }).unwrap();

        assert!(out.join("derivatives/qsmxt/slurm").exists());
    }

    /// The jobs run `qsmxt run --config <pipeline_config.toml>`, so the written
    /// config must carry the requested pipeline — not the defaults.
    #[test]
    fn test_slurm_writes_requested_pipeline_config() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_single_echo_bids(&bids);

        super::slurm::execute(SlurmArgs {
            bids_dir: bids,
            output_dir: Some(out.clone()),
            account: "testacct".to_string(),
            partition: None,
            config: None,
            time: "01:00:00".to_string(),
            mem: 16, cpus_per_task: 2, submit: false,
            include: None, exclude: None, num_echoes: None,
            pipeline: PipelineArgs {
                qsm_algorithm: Some(QsmAlgorithmArg::Tkd),
                unwrapping_algorithm: Some(UnwrapAlgorithmArg::Laplacian),
                bf_algorithm: Some(BfAlgorithmArg::Pdf),
                ..Default::default()
            },
        }).unwrap();

        let written = std::fs::read_to_string(out.join("derivatives/qsmxt/pipeline_config.toml")).unwrap();
        let config: crate::pipeline::config::PipelineConfig = toml::from_str(&written).unwrap();
        assert_eq!(config.inversion.algorithm, crate::pipeline::config::QsmAlgorithm::Tkd);
        assert_eq!(
            config.field_mapping.unwrapping_algorithm,
            crate::pipeline::config::UnwrappingAlgorithm::Laplacian
        );
        assert_eq!(config.bg_removal.algorithm, crate::pipeline::config::BfAlgorithm::Pdf);
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
