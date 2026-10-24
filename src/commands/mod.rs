pub mod common;
pub mod aggregate;
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
pub mod segment;
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
            no_dataset_steps: false,
            pipeline: PipelineArgs {
                qsm_algorithm: None,
                orientation_group: None,
                multi_orientation_algorithm: None,
                multi_orientation_lambda: None,
                multi_orientation_force: false,
                no_orientation_registration: false,
                cosmos_magnitude_weighting: false,
                cosmos_tol: None,
                cosmos_max_iter: None,
                unwrapping_algorithm: None,
                bf_algorithm: None,
                masking_input: None,
                phase_offset_removal: None,
                phase_offset_sigma_mm: None,
                coil_combination_sigma: None,
                bipolar_correction: false,
                b0_estimation: None,
                b0_weight_type: None,
                b0_weight_assumed_t2star: None,
                laplacian_solver: None,
                laplacian_fft_pad: None,
                bet_fractional_intensity: None,
                bet_voxel_scale: None,
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
                lsqr_params: Default::default(),
                heidi_params: Default::default(),
                qsmart_params: Default::default(),
                ndi_params: Default::default(),
                fansi_params: Default::default(),
                l1qsm_params: Default::default(),
                whqsm_params: Default::default(),
                hdqsm_params: Default::default(),
                amp_pe_params: Default::default(),
                separation_params: Default::default(),
                motion_params: Default::default(),
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
                smwi_params: Default::default(),
                segmentation_params: Default::default(),
                tiling_params: Default::default(),
                homogeneity_sigma_mm: None,
                homogeneity_nbox: None,
                linear_fit_reliability_threshold: None,
                linear_fit_estimate_offset: None,
                robust_fit_loss: None, robust_fit_tuning: None, robust_fit_iterations: None,
                robust_fit_downweight_level: None, robust_fit_flag_ratio: None,
                complex_fit_reltol: None, complex_fit_max_iter: None,
                no_qsm: false,
                do_swi: false,
                do_smwi: false,
                do_segmentation: false,
                do_analysis: false,
                do_t2starmap: false,
                do_r2starmap: false,
                do_r2map: false,
                do_r2primemap: false,
                do_chi_separation: false,
                chi_separation_algorithm: None,
                use_custom_qsm: None,
                use_custom_r2: None,
                use_custom_r2prime: None,
                r2prime_strategy: None,
                r2primenet_ignore_field_mismatch: false,
                r2primenet_patch: None,
                export_dicom: false,
                inhomogeneity_correction: false,
                no_inhomogeneity_correction: false,
                axial_resampling: false,
                no_axial_resampling: false,
                obliquity_threshold: None,
                crop_to_mask: false,
                ignore_slice_gap: false,
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

    fn scan(echo_times: &[f64]) -> crate::cli::ScanParamArgs {
        crate::cli::ScanParamArgs { field_strength: 3.0, echo_times: echo_times.to_vec() }
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
            voxel_scale: None,
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
            window: false, low_memory: false, patch: None, tta: false, tile_step: None,
        })).unwrap_err();
        assert!(format!("{err}").contains("deep-learning"), "{err}");
    }

    #[cfg(not(feature = "dl"))]
    #[test]
    fn test_mask_mindgrab_needs_dl() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("mag.nii");
        testutils::write_magnitude(&input);
        let err = super::mask::execute(MaskCommand::Mindgrab(crate::cli::MaskMindGrabArgs {
            common: common_mask(input, dir.path().join("mask.nii")), tta: false,
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
            common: c, window: false, low_memory: true, patch: None, tta: false, tile_step: None,
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

    // ── `qsmxt separate` and `qsmxt segment` ──
    // Neither had any coverage: separate.rs was 0/151 lines and segment.rs 0/47, which put the
    // whole workspace under the release gate.

    fn separate_common(dir: &Path, prefix: &str) -> SeparateCommonArgs {
        let qsm = dir.join("qsm.nii");
        let mask = dir.join("mask.nii");
        testutils::write_field(&qsm);
        testutils::write_mask(&mask);
        SeparateCommonArgs {
            qsm,
            mask,
            output: dir.join(prefix),
            field_strength: 3.0,
            b0_direction: vec![0.0, 0.0, 1.0],
        }
    }

    /// The three maps land under the prefix, and χ_total is the sum of its parts — the property
    /// that makes a separation a separation.
    #[test]
    fn test_separate_r2star_qsm() {
        let dir = tempfile::tempdir().unwrap();
        let common = separate_common(dir.path(), "sep");
        let r2star = dir.path().join("r2star.nii");
        testutils::write_field(&r2star);

        super::separate::execute(SeparateCommand::R2starQsm(SeparateR2starQsmArgs {
            common,
            r2star: Some(r2star),
            magnitude: vec![],
            echo_times: vec![],
            r_const_3t: None,
        }))
        .unwrap();

        let para = dir.path().join("sep_desc-paramagnetic_Chimap.nii");
        let dia = dir.path().join("sep_desc-diamagnetic_Chimap.nii");
        let total = dir.path().join("sep_desc-total_Chimap.nii");
        for p in [&para, &dia, &total] {
            assert!(p.exists(), "{} was not written", p.display());
        }
        let load = |p: &Path| qsm_core::io::read_nifti_file(p).unwrap().data;
        let (p, d, t) = (load(&para), load(&dia), load(&total));
        for i in 0..t.len() {
            assert!((p[i] - d[i] - t[i]).abs() < 1e-6 || (p[i] + d[i] - t[i]).abs() < 1e-6,
                "total is not the combination of the two sources at {i}: {} / {} / {}",
                p[i], d[i], t[i]);
        }
    }

    /// r2star-qsm needs an R2* map or the magnitude to fit one from; with neither it must say so
    /// rather than run on nothing.
    #[test]
    fn test_separate_r2star_qsm_needs_an_input() {
        let dir = tempfile::tempdir().unwrap();
        let err = super::separate::execute(SeparateCommand::R2starQsm(SeparateR2starQsmArgs {
            common: separate_common(dir.path(), "sep"),
            r2star: None,
            magnitude: vec![],
            echo_times: vec![],
            r_const_3t: None,
        }))
        .unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("--r2star") && msg.contains("--magnitude"),
                "the error should name both ways in: {msg}");
    }

    /// A method taking R2' rather than R2*, to cover the other input-loading path.
    #[test]
    fn test_separate_wavesep() {
        let dir = tempfile::tempdir().unwrap();
        let r2prime = dir.path().join("r2prime.nii");
        testutils::write_field(&r2prime);

        super::separate::execute(SeparateCommand::Wavesep(SeparateWavesepArgs {
            common: separate_common(dir.path(), "wv"),
            r2prime,
            dr_pos: None,
            dr_neg: None,
            alpha: None,
            lambda: None,
            wavelet_order: None,
            // Two iterations: this is a plumbing test, not a convergence one.
            max_iter: Some(2),
            tol: None,
        }))
        .unwrap();
        assert!(dir.path().join("wv_desc-paramagnetic_Chimap.nii").exists());
        assert!(dir.path().join("wv_desc-diamagnetic_Chimap.nii").exists());
    }

    /// DECOMPOSE fits the multi-echo magnitude signal directly, so it exercises the multi-echo
    /// loading path the R2*/R2' methods do not.
    #[test]
    fn test_separate_decompose() {
        let dir = tempfile::tempdir().unwrap();
        let mags: Vec<PathBuf> = (1..=3)
            .map(|e| {
                let p = dir.path().join(format!("mag{e}.nii"));
                testutils::write_magnitude(&p);
                p
            })
            .collect();

        super::separate::execute(SeparateCommand::Decompose(SeparateDecomposeArgs {
            common: separate_common(dir.path(), "dc"),
            magnitude: mags,
            echo_times: vec![0.004, 0.008, 0.012],
            n_inner: Some(1),
            chi_bound: None,
            max_lm_iter: Some(1),
        }))
        .unwrap();
        assert!(dir.path().join("dc_desc-paramagnetic_Chimap.nii").exists());
        assert!(dir.path().join("dc_desc-total_Chimap.nii").exists());
    }

    /// χ-sep iLSQR takes the local field and R2' together — the four-input path.
    #[test]
    fn test_separate_chi_sep_ilsqr() {
        let dir = tempfile::tempdir().unwrap();
        let local_field = dir.path().join("lfs.nii");
        let r2prime = dir.path().join("r2p.nii");
        let mag = dir.path().join("mag.nii");
        testutils::write_field(&local_field);
        testutils::write_field(&r2prime);
        testutils::write_magnitude(&mag);

        super::separate::execute(SeparateCommand::ChiSepIlsqr(SeparateChiSepIlsqrArgs {
            common: separate_common(dir.path(), "il"),
            local_field,
            r2prime,
            magnitude: vec![mag],
            dr_pos: None,
            dr_neg: None,
            percentage: None,
            max_iter: Some(1),
            tol: None,
            cg_max_iter: Some(1),
            cg_tol: None,
            lambda1: None,
            r2p_min: None,
            r2p_max: None,
        }))
        .unwrap();
        assert!(dir.path().join("il_desc-paramagnetic_Chimap.nii").exists());
        assert!(dir.path().join("il_desc-diamagnetic_Chimap.nii").exists());
    }

    /// `qsmxt segment` rejects a version it does not know rather than silently choosing one — the
    /// version selects the label table, so the wrong one mislabels every structure.
    #[test]
    fn test_segment_rejects_an_unknown_version() {
        let dir = tempfile::tempdir().unwrap();
        let mag = dir.path().join("mag.nii");
        testutils::write_magnitude(&mag);
        let err = super::segment::execute(SegmentArgs {
            magnitude: mag,
            output: dir.path().join("dseg.nii"),
            lookup: None,
            params: SegmentationParamArgs {
                synthseg_version: Some("v3".into()),
                ..Default::default()
            },
        })
        .unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("v1") && msg.contains("v2"), "the error should name the options: {msg}");
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
            magnitude: Some(mag.clone()),
            romeo_params: Default::default(),
        })).unwrap();
        assert!(output.exists());

        // A multi-echo setting cannot apply to one 3D phase, so it is refused rather than ignored.
        let err = super::unwrap::execute(UnwrapCommand::Romeo(UnwrapRomeoArgs {
            common: common_unwrap(dir.path().join("phase.nii"), dir.path().join("mask.nii"), output.clone()),
            magnitude: Some(mag),
            romeo_params: crate::cli::RomeoParamArgs { romeo_template: Some(2), ..Default::default() },
        })).unwrap_err();
        assert!(format!("{err}").contains("--romeo-template"), "{err}");
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

        let mut c = common_bgremove(input.clone(), mask.clone(), output.clone());
        c.output_mask = Some(output_mask.clone());
        super::bgremove::execute(BgremoveCommand::Vsharp(BgremoveVsharpArgs {
            common: c,
            deconvolve: None, threshold: None, max_radius: None, min_radius: None, grid: None, grid_spacing: None,
        })).unwrap();
        assert!(output.exists());
        assert!(output_mask.exists());

        // The defaults are qsm-core's V-SHARP on QSMxT's working grid (the acquired one), and
        // `--deconvolve true` reaches it.
        let field = qsm_core::io::read_nifti_file(&input).unwrap();
        let (m, _) = super::common::load_mask(&mask).unwrap();
        let (nx, ny, nz) = field.dims;
        let (vx, vy, vz) = field.voxel_size;
        let grid = qsm_core::Grid::new(nx, ny, nz, vx, vy, vz);
        let core = |p: &qsm_core::bgremove::VsharpParams| {
            let p = qsm_core::bgremove::VsharpParams { grid: qsm_core::bgremove::VsharpGrid::Native, ..p.clone() };
            qsm_core::bgremove::vsharp(&field.data, &m, &grid, &p, |_, _| {}).unwrap().0
        };
        let read = |p: &std::path::Path| qsm_core::io::read_nifti_file(p).unwrap().data;
        let off = read(&output);
        let mut c = common_bgremove(input.clone(), mask.clone(), output.clone());
        c.output_mask = None;
        super::bgremove::execute(BgremoveCommand::Vsharp(BgremoveVsharpArgs {
            common: c,
            deconvolve: Some(true), threshold: None, max_radius: None, min_radius: None, grid: None, grid_spacing: None,
        })).unwrap();
        let on = read(&output);
        let as_f32 = |v: Vec<f64>| v.into_iter().map(|x| x as f32).collect::<Vec<_>>();
        assert_eq!(as_f32(on), as_f32(core(&qsm_core::bgremove::VsharpParams {
            deconvolve: true, ..Default::default()
        })));
        assert_eq!(as_f32(off), as_f32(core(&qsm_core::bgremove::VsharpParams::default())));
    }

    #[test]
    fn test_bgremove_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("field.nii");
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("local.nii");
        testutils::write_field(&input);
        testutils::write_mask(&mask);

        // PDF weights the fit by a noise map made from the echoes' magnitudes.
        let magnitude = dir.path().join("mag.nii");
        testutils::write_magnitude(&magnitude);
        super::bgremove::execute(BgremoveCommand::Pdf(BgremovePdfArgs {
            common: common_bgremove(input.clone(), mask.clone(), output.clone()),
            scan: scan(&[0.02]),
            magnitude: vec![magnitude.clone()],
            tol: None,
        })).unwrap();
        assert!(output.exists());
        // One magnitude for two echo times is refused before PDF runs.
        let e = super::bgremove::execute(BgremoveCommand::Pdf(BgremovePdfArgs {
            common: common_bgremove(input, mask, output),
            scan: scan(&[0.01, 0.02]),
            magnitude: vec![magnitude],
            tol: None,
        })).unwrap_err().to_string();
        assert!(e.contains("magnitude of every echo"), "{e}");
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

    /// `invert tgv` takes a field in ppm like the other inversions, with the echo times and field
    /// strength its weights are in units of. One iteration is enough for the call site.
    #[test]
    fn test_invert_tgv_runs_on_a_ppm_field() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("field.nii");
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("chi.nii");
        testutils::write_field(&input);
        testutils::write_mask(&mask);

        super::invert::execute(InvertCommand::Tgv(InvertTgvArgs {
            common: common_invert(input, mask, output.clone()),
            scan: scan(&[0.005, 0.01]),
            iterations: Some(1),
            erosions: Some(0),
            alpha1: None, alpha0: None,
            step_size: None,
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

    /// `--no-noise-fill` only applies to the phase/magnitude pair, and the pair's outputs only
    /// to the pair: given with a plain volume they are refused rather than ignored (#279).
    #[test]
    fn resample_refuses_flags_that_do_not_apply() {
        use clap::Parser;
        let parse = |a: &[&str]| crate::cli::Cli::try_parse_from(
            [&["qsmxt", "resample"][..], a].concat());
        assert!(parse(&["vol.nii", "--no-noise-fill"]).is_err());
        assert!(parse(&["vol.nii", "--phase-out", "p.nii"]).is_err());
        assert!(parse(&["vol.nii", "--phase", "p.nii", "--magnitude", "m.nii"]).is_err());
        assert!(parse(&["--phase", "p.nii", "--magnitude", "m.nii", "--output", "o.nii"]).is_err());
        assert!(parse(&["--phase", "p.nii", "--magnitude", "m.nii", "--no-noise-fill"]).is_ok());
        assert!(parse(&["vol.nii", "--output", "o.nii"]).is_ok());
    }

    /// `qsmxt segment` computes a segmentation, so the run's "use a supplied one" flag is refused.
    #[test]
    fn segment_refuses_use_custom_dseg() {
        use clap::Parser;
        let cli = crate::cli::Cli::try_parse_from(
            ["qsmxt", "segment", "mag.nii", "--output", "seg.nii", "--use-custom-dseg", "*"]).unwrap();
        let crate::cli::Command::Segment(args) = cli.command else { panic!("expected segment") };
        let err = format!("{}", super::segment::execute(args).unwrap_err());
        assert!(err.contains("--use-custom-dseg"), "{err}");
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
        let output = dir.path().join("swi.nii");
        let (mut phase, mut magnitude) = (vec![], vec![]);
        for e in 1..=2 {
            let (p, m) = (dir.path().join(format!("phase{e}.nii")), dir.path().join(format!("mag{e}.nii")));
            testutils::write_phase(&p);
            testutils::write_magnitude(&m);
            phase.push(p);
            magnitude.push(m);
        }

        // Two echoes, CLEAR-SWI's own mask.
        super::swi::execute(SwiArgs {
            phase: phase.clone(), magnitude: magnitude.clone(), tes: Some(vec![0.004, 0.008]), mask: None,
            output: output.clone(),
            mip: false, mip_output: None,
            swi_params: Default::default(),
        }).unwrap();
        assert!(output.exists());

        // Several echoes need their echo times: none given, no sidecars.
        let err = super::swi::execute(SwiArgs {
            phase: phase.clone(), magnitude: magnitude.clone(), tes: None, mask: None,
            output: output.clone(), mip: false, mip_output: None, swi_params: Default::default(),
        }).unwrap_err().to_string();
        assert!(err.contains("echo times"), "{err}");
        // One magnitude per phase.
        let err = super::swi::execute(SwiArgs {
            phase, magnitude: magnitude[..1].to_vec(), tes: Some(vec![0.004, 0.008]), mask: None,
            output, mip: false, mip_output: None, swi_params: Default::default(),
        }).unwrap_err().to_string();
        assert!(err.contains("magnitude"), "{err}");
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
        // The support the map is defined on is published beside it, generated by referencing.
        let support: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(deriv.join("sub-1/anat/sub-1_desc-qsm_mask.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(support["GeneratedBy"], "bids::prov/#reference-sub-1");
        assert!(support.get("SkullStripped").is_none(), "a mask takes no SkullStripped");
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

    /// End to end, the behaviour #242 is actually about: a MESE on a grid of its own produces R2
    /// and R2'. It used to be dropped on a voxel-count check, so neither map was ever written and
    /// chi-separation quietly fell back to whatever was left — with nothing in the output saying
    /// an acquisition had been discarded.
    #[test]
    fn test_run_computes_r2_from_a_mese_on_its_own_grid() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_multi_echo_bids(&bids);

        // A 2D-style spin-echo companion: half the matrix, twice the slice pitch, covering the
        // same box as the 8x8x8 1 mm GRE. Nothing about it matches the GRE's grid.
        let anat = bids.join("sub-1/anat");
        let mese_dims = (4, 4, 4);
        let mese_affine = [
            2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        for echo in 1..=4 {
            let te = 0.012 * echo as f64;
            // Decaying, so the EPG fit has something to find rather than R2 = 0 everywhere.
            let signal = 1000.0 * (-15.0 * te).exp();
            let data = vec![signal; mese_dims.0 * mese_dims.1 * mese_dims.2];
            qsm_core::io::save_nifti_to_file(
                &anat.join(format!("sub-1_echo-{}_MESE.nii", echo)),
                &data, mese_dims, (2.0, 2.0, 2.0), &mese_affine,
            ).unwrap();
            testutils::write_sidecar(
                &anat.join(format!("sub-1_echo-{}_MESE.json", echo)), te, 3.0);
        }

        let mut args = default_run_args(bids, out.clone());
        args.pipeline.qsm_algorithm = Some(QsmAlgorithmArg::Tkd);
        args.pipeline.unwrapping_algorithm = Some(UnwrapAlgorithmArg::Laplacian);
        args.pipeline.bf_algorithm = Some(BfAlgorithmArg::Vsharp);
        args.pipeline.masking_input = Some(MaskInputArg::Magnitude);
        args.dry = false;
        args.no_mem_limit = true;
        args.pipeline.do_r2starmap = true;
        args.pipeline.do_r2map = true;
        args.pipeline.do_r2primemap = true;
        // `mese` rather than `auto` deliberately: it removes the R2PRIMEnet fallback, so this can
        // only pass by actually measuring R2' from the spin-echo data, and the test never reaches
        // for network weights. Under the old voxel-count behaviour `auto` quietly went and
        // downloaded R2PRIMEnet instead — which is the "falls back to whatever remains" in #242,
        // and took eight minutes to do it.
        args.pipeline.r2prime_strategy = Some(R2PrimeStrategyArg::Mese);
        super::run::execute(args).unwrap();

        let deriv = out.join("derivatives/qsmxt");
        assert!(deriv.join("sub-1/anat/sub-1_R2map.nii").exists(),
                "a differently-gridded MESE must still yield R2");
        assert!(deriv.join("sub-1/anat/sub-1_R2primemap.nii").exists(),
                "and so R2' = R2* - R2");
        // On the GRE's grid, not the MESE's: everything downstream indexes it that way.
        let r2 = qsm_core::io::read_nifti_file(&deriv.join("sub-1/anat/sub-1_R2map.nii")).unwrap();
        assert_eq!(r2.dims, (8, 8, 8), "R2 belongs on the reconstruction grid");

        // The R2' sidecar says it was measured, not predicted (#236), next to its provenance.
        let side: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
            deriv.join("sub-1/anat/sub-1_R2primemap.json")).unwrap()).unwrap();
        assert_eq!(side["R2primeSource"], "measured", "{side}");
        assert!(side["R2primeMethod"].as_str().unwrap().contains("EPG"), "{side}");
        assert!(side["GeneratedBy"].is_string(), "BIDS-Prov kept the source fields: {side}");
    }

    /// TFI is a total-field method (#265): it gets the field map and the brain mask, and no
    /// background-removal stage runs in front of it.
    #[test]
    fn tfi_inverts_the_total_field_with_no_background_removal() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_multi_echo_bids(&bids);
        let mut args = default_run_args(bids, out.clone());
        args.pipeline.qsm_algorithm = Some(QsmAlgorithmArg::Tfi);
        args.pipeline.unwrapping_algorithm = Some(UnwrapAlgorithmArg::Laplacian);
        args.pipeline.masking_input = Some(MaskInputArg::Magnitude);
        args.dry = false;
        args.no_mem_limit = true;
        super::run::execute(args).unwrap();
        let deriv = out.join("derivatives/qsmxt");
        assert!(deriv.join("sub-1/anat/sub-1_Chimap.nii").exists());
        assert!(!deriv.join("workflow/sub-1/bgremove").exists(), "background removal ran in front of TFI");
        let prov: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
            deriv.join("workflow/sub-1/invert/provenance.json")).unwrap()).unwrap();
        let inputs = prov["inputs"].to_string();
        assert!(inputs.contains("field-ppm") || inputs.contains("unwrap"), "TFI read {inputs}");
    }

    /// iQFM makes a local field; with an inversion that takes the total field or wrapped phase
    /// it would do nothing, and used to leave the field map unwritten (#276).
    #[test]
    fn iqfm_with_a_total_field_inversion_is_refused_before_processing() {
        for alg in [QsmAlgorithmArg::Tgv, QsmAlgorithmArg::Qsmart, QsmAlgorithmArg::Autoqsm,
                    QsmAlgorithmArg::Nextqsm, QsmAlgorithmArg::Tfi] {
            let dir = tempfile::tempdir().unwrap();
            let bids = dir.path().join("bids");
            testutils::create_multi_echo_bids(&bids);
            let mut args = default_run_args(bids, dir.path().join("out"));
            args.pipeline.qsm_algorithm = Some(alg);
            args.pipeline.bf_algorithm = Some(BfAlgorithmArg::Iqfm);
            let err = super::run::execute(args).unwrap_err().to_string();
            assert!(err.contains("iqfm") && err.contains("no effect"), "{alg:?}: {err}");
        }
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

    /// The per-structure tables are the point of `--do-analysis`, so the run has to produce one
    /// covering every map it made — not χ alone — and gather them into a dataset-level table.
    /// The segmentation is supplied so the test needs no network weights.
    #[test]
    fn analysis_tabulates_every_map_per_structure() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        // Large enough that χ is defined inside the structures after background removal's erosion.
        testutils::create_multi_echo_bids_on(&bids, testutils::STRUCTURES_N);
        testutils::write_dseg_on(&bids.join("derivatives/mydseg/sub-1/anat/sub-1_dseg.nii"), testutils::STRUCTURES_N);

        let mut args = default_run_args(bids, out.clone());
        args.pipeline.qsm_algorithm = Some(QsmAlgorithmArg::Tkd);
        args.pipeline.unwrapping_algorithm = Some(UnwrapAlgorithmArg::Laplacian);
        args.pipeline.bf_algorithm = Some(BfAlgorithmArg::Vsharp);
        args.pipeline.masking_input = Some(MaskInputArg::Magnitude);
        args.dry = false;
        args.no_mem_limit = true;
        args.pipeline.do_t2starmap = true;
        args.pipeline.do_r2starmap = true;
        args.pipeline.do_analysis = true;
        args.pipeline.segmentation_params.use_custom_dseg = Some("mydseg".to_string());
        super::run::execute(args).unwrap();

        let deriv = out.join("derivatives/qsmxt");
        let per_run = std::fs::read_to_string(
            deriv.join("sub-1/anat/sub-1_desc-segmentation_stats.tsv")).unwrap();
        let header: Vec<&str> = per_run.lines().next().unwrap().split('\t').collect();
        for col in ["map", "unit", "index", "name", "n_voxels", "n_valid",
                    "mean", "sd", "median", "min", "max", "p5", "p95"] {
            assert!(header.contains(&col), "column {col} missing from {header:?}");
        }
        let maps: std::collections::HashSet<&str> =
            per_run.lines().skip(1).map(|l| l.split('\t').next().unwrap()).collect();
        assert!(maps.contains("Chimap"), "{maps:?}");
        assert!(maps.contains("T2starmap"), "{maps:?}");
        assert!(maps.contains("R2starmap"), "{maps:?}");

        // Every row is complete, and min <= median <= max on all of them.
        for line in per_run.lines().skip(1) {
            let c: Vec<&str> = line.split('\t').collect();
            assert_eq!(c.len(), header.len(), "ragged row: {line}");
            let f = |i: usize| c[i].parse::<f64>().unwrap();
            let (median, min, max) = (f(9), f(10), f(11));
            assert!(min <= median && median <= max, "{line}");
            assert!(f(5) >= 1.0, "a row must summarise at least one voxel: {line}");
        }

        // A figure per map, beside the table, named so it sorts next to it.
        let figs: Vec<String> = std::fs::read_dir(deriv.join("sub-1/anat")).unwrap()
            .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
            .filter(|n| n.ends_with("_stats.png"))
            .collect();
        for want in ["sub-1_desc-chimap_stats.png", "sub-1_desc-t2starmap_stats.png",
                     "sub-1_desc-r2starmap_stats.png"] {
            assert!(figs.contains(&want.to_string()), "missing {want}: {figs:?}");
        }
        // Real PNGs, not empty files.
        for f in &figs {
            let p = deriv.join("sub-1/anat").join(f);
            let head = std::fs::read(&p).unwrap();
            assert!(head.starts_with(&[0x89, b'P', b'N', b'G']), "{f} is not a PNG");
            assert!(head.len() > 1000, "{f} is suspiciously small");
        }

        // The dataset-level table repeats those rows behind the run's BIDS entities.
        let group = std::fs::read_to_string(deriv.join("desc-segmentation_stats.tsv")).unwrap();
        assert!(group.lines().next().unwrap().starts_with("subject\tsession\t"));
        assert_eq!(group.lines().count(), per_run.lines().count());
        assert!(group.lines().skip(1).all(|l| l.starts_with("1\t\t\t\t\t\t")),
                "every row should carry sub-1 and no other entities");
    }

    /// Every NIfTI under `dir`, recursively, with its modification time.
    fn niftis_with_mtime(dir: &std::path::Path) -> Vec<(PathBuf, std::time::SystemTime)> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "nii") {
                    out.push((p.clone(), e.metadata().unwrap().modified().unwrap()));
                }
            }
        }
        out.sort();
        out
    }

    /// `--clean-intermediates` deleted published outputs recorded under a non-final step (the
    /// combined `rec-mcpc3ds` echoes, under `scale_phase`) and the state file, so the next run
    /// recomputed everything (#271). Now it removes only `workflow/` images, keeps the state, and
    /// an immediate re-run with the same configuration recomputes nothing.
    #[test]
    fn cleaning_intermediates_keeps_published_outputs_and_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_multi_coil_bids(&bids, 2);
        let mut args = default_run_args(bids, out.clone());
        args.dry = false;
        args.no_mem_limit = true;
        args.clean_intermediates = true;
        // What is cleaned does not depend on the algorithms; TKD and Laplacian are the cheapest.
        args.pipeline.qsm_algorithm = Some(QsmAlgorithmArg::Tkd);
        args.pipeline.unwrapping_algorithm = Some(UnwrapAlgorithmArg::Laplacian);
        let mut rerun = default_run_args(args.bids_dir.clone(), out.clone());
        rerun.pipeline.qsm_algorithm = Some(QsmAlgorithmArg::Tkd);
        rerun.pipeline.unwrapping_algorithm = Some(UnwrapAlgorithmArg::Laplacian);
        super::run::execute(args).unwrap();

        let deriv = out.join("derivatives/qsmxt");
        let anat = deriv.join("sub-1/ses-1/anat");
        let combined: Vec<_> = std::fs::read_dir(&anat).unwrap().flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("rec-mcpc3ds") && n.ends_with(".nii"))
            .collect();
        assert!(!combined.is_empty(), "the combined echoes were cleaned out of anat/");
        assert!(niftis_with_mtime(&deriv.join("workflow")).is_empty(), "workflow/ still holds images");
        let state = std::fs::read_dir(deriv.join("workflow")).is_ok();
        assert!(state, "the workflow directory (and its state file) is gone");

        let before = niftis_with_mtime(&deriv);
        rerun.dry = false;
        rerun.no_mem_limit = true;
        rerun.clean_intermediates = true;
        super::run::execute(rerun).unwrap();
        assert_eq!(niftis_with_mtime(&deriv), before, "an immediate re-run recomputed something");
    }

    /// The multi-orientation fan-in reads each member's local field, an intermediate: cleaning
    /// has to wait until it has run (#271).
    #[test]
    fn cleaning_intermediates_waits_for_the_multi_orientation_fan_in() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_multi_orientation_bids(&bids);
        let mut args = default_run_args(bids, out.clone());
        args.dry = false;
        args.no_mem_limit = true;
        args.clean_intermediates = true;
        args.include = Some(vec!["*acq-dir*".to_string()]);
        args.pipeline.orientation_group = Some("acq".to_string());
        args.pipeline.qsm_algorithm = Some(QsmAlgorithmArg::Tkd);
        args.pipeline.unwrapping_algorithm = Some(UnwrapAlgorithmArg::Laplacian);
        super::run::execute(args).unwrap();
        let deriv = out.join("derivatives/qsmxt");
        assert!(deriv.join("sub-1/ses-1/anat/sub-1_ses-1_desc-cosmos_Chimap.nii").exists());
        assert!(niftis_with_mtime(&deriv.join("workflow")).is_empty(), "workflow/ still holds images");
    }

    /// The table summarises what the run made, so a run that makes no susceptibility map at all
    /// still produces one. `--no-qsm --do-r2starmap --do-analysis` has to stay a no-QSM run:
    /// reconstruction must not be switched back on behind the request.
    #[test]
    fn analysis_works_without_qsm() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_multi_echo_bids(&bids);
        testutils::write_dseg(&bids.join("derivatives/mydseg/sub-1/anat/sub-1_dseg.nii"));

        let mut args = default_run_args(bids, out.clone());
        args.dry = false;
        args.no_mem_limit = true;
        args.pipeline.no_qsm = true;
        args.pipeline.do_r2starmap = true;
        args.pipeline.do_analysis = true;
        args.pipeline.segmentation_params.use_custom_dseg = Some("mydseg".to_string());
        super::run::execute(args).unwrap();

        let deriv = out.join("derivatives/qsmxt");
        assert!(!deriv.join("sub-1/anat/sub-1_Chimap.nii").exists(),
                "--no-qsm must survive: the analysis needs a map, not χ specifically");

        let table = std::fs::read_to_string(
            deriv.join("sub-1/anat/sub-1_desc-segmentation_stats.tsv")).unwrap();
        let maps: std::collections::HashSet<&str> =
            table.lines().skip(1).map(|l| l.split('\t').next().unwrap()).collect();
        // The R2*/T2* stage always writes both — T2* is 1/R2* — so both are there to summarise,
        // and χ is not.
        assert_eq!(maps, ["R2starmap", "T2starmap"].into_iter().collect(), "{maps:?}");
        assert!(deriv.join("desc-segmentation_stats.tsv").exists(), "the group table too");
    }

    /// Referencing to a structure means that structure's mean χ ends up at zero — that is the
    /// whole contract, and it is worth asserting on a real run rather than on the arithmetic
    /// alone, because the map, the mask and the parcellation all have to line up for it to hold.
    #[test]
    fn qsm_can_be_referenced_to_a_parcellation_region() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        // Large enough that the thalami survive background removal's erosion.
        testutils::create_multi_echo_bids_on(&bids, testutils::STRUCTURES_N);
        let dseg_path = bids.join("derivatives/mydseg/sub-1/anat/sub-1_dseg.nii");
        testutils::write_dseg_on(&dseg_path, testutils::STRUCTURES_N);

        let mut args = default_run_args(bids, out.clone());
        args.pipeline.qsm_algorithm = Some(QsmAlgorithmArg::Tkd);
        args.pipeline.unwrapping_algorithm = Some(UnwrapAlgorithmArg::Laplacian);
        args.pipeline.bf_algorithm = Some(BfAlgorithmArg::Vsharp);
        args.pipeline.masking_input = Some(MaskInputArg::Magnitude);
        args.dry = false;
        args.no_mem_limit = true;
        // Both thalami, supplied rather than segmented so the test needs no network weights.
        args.pipeline.qsm_reference = Some("thalamus".to_string());
        args.pipeline.do_analysis = true;
        args.pipeline.segmentation_params.use_custom_dseg = Some("mydseg".to_string());
        super::run::execute(args).unwrap();

        let deriv = out.join("derivatives/qsmxt");
        let chi = qsm_core::io::read_nifti_file(&deriv.join("sub-1/anat/sub-1_Chimap.nii")).unwrap();
        let dseg = qsm_core::io::read_nifti_file(&dseg_path).unwrap();
        let mask = qsm_core::io::read_nifti_file(&deriv.join("sub-1/anat/sub-1_mask.nii")).unwrap();

        let vals: Vec<f64> = chi.data.iter().zip(&dseg.data).zip(&mask.data)
            .filter(|((_, &l), &m)| m > 0.5 && matches!(l.round() as i32, 10 | 49))
            .map(|((&c, _), _)| c)
            .collect();
        assert!(!vals.is_empty(), "the test dseg should put thalamus voxels inside the mask");
        let mean = vals.iter().sum::<f64>() / vals.len() as f64;
        assert!(mean.abs() < 1e-9, "thalamus mean should be 0 after referencing, was {mean}");

        // Only the offset moved: voxels outside the brain mask stay zero, as the other
        // referencing modes leave them.
        assert!(chi.data.iter().zip(&mask.data).all(|(&c, &m)| m > 0.5 || c == 0.0));

        // The reference is recorded beside the map it produced, with what it removed.
        let sidecar: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(deriv.join("sub-1/anat/sub-1_Chimap.json")).unwrap()).unwrap();
        assert_eq!(sidecar["QsmReference"], "thalamus");
        assert_eq!(sidecar["QsmReferenceLabels"], serde_json::json!([10, 49]));
        assert!(sidecar["QsmReferenceVoxels"].as_u64().unwrap() > 0);
        let offset = sidecar["QsmReferenceOffsetPpm"].as_f64().unwrap();
        assert!(offset.is_finite());
        // BIDS-Prov writes to the same sidecar and must not have wiped those fields.
        assert!(sidecar.get("GeneratedBy").is_some(), "provenance clobbered the reference fields");

        // And on every susceptibility row of the run's table, so the spreadsheet stands alone.
        let table = std::fs::read_to_string(
            deriv.join("sub-1/anat/sub-1_desc-segmentation_stats.tsv")).unwrap();
        let cols: Vec<&str> = table.lines().next().unwrap().split('\t').collect();
        let (ir, io) = (cols.iter().position(|c| *c == "reference").unwrap(),
                        cols.iter().position(|c| *c == "reference_offset_ppm").unwrap());
        let mut chi_rows = 0;
        for line in table.lines().skip(1) {
            let f: Vec<&str> = line.split('\t').collect();
            if f[0] == "Chimap" {
                chi_rows += 1;
                assert_eq!(f[ir], "thalamus", "{line}");
                assert!((f[io].parse::<f64>().unwrap() - offset).abs() < 1e-6, "{line}");
            } else {
                assert_eq!(f[ir], "", "{} has no reference to claim: {line}", f[0]);
            }
        }
        assert!(chi_rows > 0, "the run produced no susceptibility rows to check");

        // The reference summary names the run and what it referenced to.
        let summary = std::fs::read_to_string(deriv.join("desc-reference_summary.tsv")).unwrap();
        let mut lines = summary.lines();
        let header: Vec<&str> = lines.next().unwrap().split('\t').collect();
        assert_eq!(header, [
            "subject", "session", "acquisition", "reconstruction", "inversion", "run",
            "map", "reference", "offset_ppm", "n_voxels",
        ], "{summary}");
        let row: Vec<&str> = lines.next().unwrap().split('\t').collect();
        assert_eq!(row.len(), header.len(), "{summary}");
        assert_eq!((row[0], row[6], row[7]), ("1", "Chimap", "thalamus"), "{summary}");
        assert!((row[8].parse::<f64>().unwrap() - offset).abs() < 1e-6, "{summary}");
    }

    /// A mistyped region has to fail before any data is touched — the alternative is an hours-long
    /// run whose maps are referenced to something nobody asked for.
    #[test]
    fn an_unknown_reference_region_fails_before_the_run() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_multi_echo_bids(&bids);

        let mut args = default_run_args(bids, out.clone());
        args.dry = false;
        args.no_mem_limit = true;
        args.pipeline.qsm_reference = Some("thalmus".to_string()); // typo
        let err = format!("{}", super::run::execute(args).unwrap_err());
        assert!(err.contains("unknown reference region"), "{err}");
        assert!(err.contains("thalamus"), "the error should list the valid names: {err}");
        assert!(!out.join("derivatives/qsmxt/sub-1/anat/sub-1_Chimap.nii").exists(),
                "nothing should have been reconstructed");
    }

    /// The one single-echo TGV run, for the call site that hands TGV echo 1's wrapped phase (what
    /// it is handed is `tgv_gets_the_field_in_ppm_or_echo_1_phase_with_te_in_s_and_b0_in_t`), with
    /// one iteration; also inhomogeneity correction and a phase-quality mask on a single echo.
    #[test]
    fn test_run_single_echo_tgv() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_single_echo_bids(&bids);

        let mut args = default_run_args(bids, out.clone());
        args.pipeline.qsm_algorithm = Some(QsmAlgorithmArg::Tgv);
        args.pipeline.masking_input = Some(MaskInputArg::MagnitudeFirst);
        args.pipeline.tgv_params.tgv_iterations = Some(1);
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
    fn test_invert_lsqr() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("field.nii");
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("chi.nii");
        testutils::write_field(&input);
        testutils::write_mask(&mask);

        super::invert::execute(InvertCommand::Lsqr(InvertLsqrArgs {
            common: common_invert(input, mask, output.clone()),
            magnitude: vec![],
            b0: 3.0,
            lsqr_params: LsqrParamArgs { lsqr_max_iter: Some(5), ..Default::default() },
        })).unwrap();
        assert!(output.exists());
    }

    /// HEIDI runs its LSQR seed and then the NESTA loop; both budgets are cut to keep the test
    /// quick, since what is under test is the wiring, not the reconstruction.
    #[test]
    fn test_invert_heidi() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("field.nii");
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("chi.nii");
        testutils::write_field(&input);
        testutils::write_mask(&mask);

        super::invert::execute(InvertCommand::Heidi(InvertHeidiArgs {
            common: common_invert(input, mask, output.clone()),
            scan: scan(&[0.02]),
            magnitude: vec![],
            lsqr_params: LsqrParamArgs { lsqr_max_iter: Some(5), ..Default::default() },
            heidi_params: HeidiParamArgs {
                heidi_continuation_steps: Some(1),
                heidi_inner_iterations: Some(2),
                ..Default::default()
            },
        })).unwrap();
        assert!(output.exists());
    }

    /// `invert medi` takes every echo's magnitude (one file per echo here) with the echo times,
    /// as `qsmxt run` gives MEDI them, and MEDI-mSMV with three echoes; a magnitude count that
    /// does not match the echo times is refused.
    #[test]
    fn test_invert_medi_takes_every_echo() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("field.nii");
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("chi.nii");
        testutils::write_field(&input);
        testutils::write_mask(&mask);
        let mags: Vec<PathBuf> = (0..3).map(|e| {
            let p = dir.path().join(format!("mag{e}.nii"));
            testutils::write_magnitude_decayed(&p, (-0.2 * e as f64).exp());
            p
        }).collect();
        let medi = |magnitude: Vec<PathBuf>, msmv: bool| InvertCommand::Medi(InvertMediArgs {
            common: common_invert(input.clone(), mask.clone(), output.clone()),
            scan: scan(&[0.005, 0.010, 0.015]),
            magnitude,
            csf: None, lambda_csf: None, csf_r2s_threshold: None, csf_erode: None, zeropad: None,
            lambda: None, merit: Some(false), smv: Some(false), smv_radius: None,
            msmv, msmv_maxk: None, msmv_vessel_radius: None, no_msmv_prefilter: false,
            data_weighting: None, percentage: None, cg_tol: None, cg_max_iter: Some(2),
            max_iter: Some(1), tol: None,
        });
        super::invert::execute(medi(mags.clone(), false)).unwrap();
        assert!(output.exists());
        super::invert::execute(medi(mags.clone(), true)).unwrap();
        let e = super::invert::execute(medi(mags[..2].to_vec(), false)).unwrap_err().to_string();
        assert!(e.contains("magnitude of every echo") && e.contains("2 magnitude volume(s) for 3 echo time(s)"), "{e}");
    }

    /// NDI's data weight is the combined magnitude, and it converts the field with the scan's
    /// echo times and field strength.
    #[test]
    fn test_invert_ndi_weights_by_the_magnitude() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("field.nii");
        let mask = dir.path().join("mask.nii");
        let output = dir.path().join("chi.nii");
        let magnitude = dir.path().join("mag.nii");
        testutils::write_field(&input);
        testutils::write_mask(&mask);
        testutils::write_magnitude(&magnitude);
        super::invert::execute(InvertCommand::Ndi(InvertNdiArgs {
            common: common_invert(input, mask, output.clone()),
            tau: None, alpha: None, max_iter: Some(2), auto_stop: None,
            scan: scan(&[0.02]),
            magnitude: vec![magnitude],
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
            phase: vec![phase], magnitude: vec![mag], tes: None, mask: Some(mask),
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
            mem: Some(16), cpus_per_task: 2, submit: false,
            include: None, exclude: None, num_echoes: None, force: false, clean_intermediates: false,
            debug: false, source_dicom: None, dicom_outputs: None,
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
            mem: Some(16), cpus_per_task: 2, submit: false,
            include: None, exclude: None, num_echoes: None, force: false, clean_intermediates: false,
            debug: false, source_dicom: None, dicom_outputs: None,
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

    /// The words of a generated job's command, as bash would split them: single-quoted strings
    /// (with `'\''` escapes) and backslash-newline continuations; the `exec <binary>` dropped.
    fn job_command(script: &Path) -> Vec<String> {
        let text = std::fs::read_to_string(script).unwrap();
        let body: String = text.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .collect::<Vec<_>>().join("\n").replace("\\\n", " ");
        let (mut words, mut cur, mut in_word, mut quoted) = (Vec::new(), String::new(), false, false);
        let chars: Vec<char> = body.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if quoted {
                if c == '\'' { quoted = false; } else { cur.push(c); }
            } else if c == '\'' {
                quoted = true;
                in_word = true;
            } else if c == '\\' && i + 1 < chars.len() {
                cur.push(chars[i + 1]);
                in_word = true;
                i += 1;
            } else if c.is_whitespace() {
                if in_word { words.push(std::mem::take(&mut cur)); in_word = false; }
            } else {
                cur.push(c);
                in_word = true;
            }
            i += 1;
        }
        if in_word { words.push(cur); }
        assert_eq!(words[0], "exec", "{text}");
        let mut argv = vec!["qsmxt".to_string()];
        argv.extend(words.into_iter().skip(2));
        argv
    }

    /// Run a generated job's command in-process, through the CLI parser.
    fn run_job(script: &Path) {
        use clap::Parser;
        let argv = job_command(script);
        let cli = Cli::try_parse_from(&argv).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
        match cli.command {
            Command::Run(mut args) => {
                args.no_mem_limit = true;
                super::run::execute(args).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
            }
            Command::Aggregate(args) => super::aggregate::execute(args).unwrap_or_else(|e| panic!("{argv:?}: {e}")),
            other => panic!("unexpected job command {other:?}"),
        }
    }

    /// Every file under `root`, relative to it, outside the directories that are not the
    /// dataset's results.
    fn derivative_files(root: &Path) -> std::collections::BTreeSet<PathBuf> {
        fn walk(root: &Path, dir: &Path, out: &mut std::collections::BTreeSet<PathBuf>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                let rel = p.strip_prefix(root).unwrap().to_path_buf();
                if ["workflow", "logs", "slurm", "qsmxt.log"].iter().any(|x| rel == Path::new(x)) {
                    continue;
                }
                if p.is_dir() { walk(root, &p, out) } else { out.insert(rel); }
            }
        }
        let mut out = Default::default();
        walk(root, root, &mut out);
        out
    }

    /// JSON with what legitimately differs between two executions masked: wall-clock times and
    /// durations, and the command line (a test binary's argv here, `qsmxt slurm …` there).
    fn normalised_json(text: &str) -> serde_json::Value {
        fn walk(key: &str, v: &mut serde_json::Value) {
            let volatile = key.contains("Time") || key.contains("time") || key == "Command"
                || key.contains("elapsed") || key.contains("duration") || key.contains("Duration");
            match v {
                serde_json::Value::Object(m) => for (k, x) in m.iter_mut() { walk(k, x) },
                serde_json::Value::Array(a) => for x in a { walk(key, x) },
                _ if volatile => *v = serde_json::Value::String("<volatile>".into()),
                serde_json::Value::String(s) if chrono::DateTime::parse_from_rfc3339(s).is_ok() => {
                    *v = serde_json::Value::String("<timestamp>".into())
                }
                _ => {}
            }
        }
        let mut v: serde_json::Value = serde_json::from_str(text).unwrap();
        walk("", &mut v);
        v
    }

    /// The acceptance test for #269: `qsmxt slurm`'s jobs — each run's `qsmxt run
    /// --no-dataset-steps`, then `qsmxt aggregate` — leave the same dataset as one local `qsmxt
    /// run`: the same files, the same dataset-level tables, provenance and metadata, and the same
    /// two-orientation COSMOS map.
    #[test]
    fn slurm_jobs_produce_what_a_local_run_does() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        // Large enough that χ is defined inside the structures after background removal's erosion.
        testutils::create_multi_orientation_bids_on(&bids, testutils::STRUCTURES_N);
        testutils::write_dseg_on(&bids.join("derivatives/mydseg/sub-1/ses-1/anat/sub-1_ses-1_dseg.nii"), testutils::STRUCTURES_N);
        let pipeline = || PipelineArgs {
            // What is under test is SLURM/local parity, not the algorithms: the cheapest ones.
            qsm_algorithm: Some(crate::cli::QsmAlgorithmArg::Tkd),
            unwrapping_algorithm: Some(UnwrapAlgorithmArg::Laplacian),
            orientation_group: Some("acq".to_string()),
            masking_input: Some(MaskInputArg::Magnitude),
            do_analysis: true,
            segmentation_params: SegmentationParamArgs { use_custom_dseg: Some("mydseg".to_string()), ..Default::default() },
            ..Default::default()
        };
        let include = Some(vec!["*acq-dir1*".to_string(), "*acq-dir2*".to_string()]);

        let local = dir.path().join("local");
        let mut args = default_run_args(bids.clone(), local.clone());
        args.dry = false;
        args.no_mem_limit = true;
        args.include = include.clone();
        args.num_echoes = Some(1);
        args.pipeline = pipeline();
        super::run::execute(args).unwrap();

        let dist = dir.path().join("dist");
        super::slurm::execute(SlurmArgs {
            bids_dir: bids.clone(), output_dir: Some(dist.clone()),
            account: "acct".into(), partition: None, config: None, time: "01:00:00".into(),
            mem: None, cpus_per_task: 1, submit: false,
            include, exclude: None, num_echoes: Some(1), force: false, clean_intermediates: false,
            debug: false, source_dicom: None, dicom_outputs: None,
            pipeline: pipeline(),
        }).unwrap();
        let slurm_dir = dist.join("derivatives/qsmxt/slurm");
        let mut run_scripts: Vec<PathBuf> = std::fs::read_dir(&slurm_dir).unwrap().flatten().map(|e| e.path())
            .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("qsmxt_sub-")).collect();
        run_scripts.sort();
        assert_eq!(run_scripts.len(), 2);

        for s in &run_scripts {
            run_job(s);
        }
        // The workers wrote nothing at dataset level: each left only its own log.
        let deriv = dist.join("derivatives/qsmxt");
        for f in ["qsmxt.log", "desc-reference_summary.tsv", "desc-segmentation_stats.tsv",
                  "dataset_description.json", "methods.md", "prov"] {
            assert!(!deriv.join(f).exists(), "a per-run job wrote {f}");
        }
        assert!(!deriv.join("sub-1/ses-1/anat/sub-1_ses-1_desc-cosmos_Chimap.nii").exists());
        let logs: Vec<String> = std::fs::read_dir(deriv.join("logs")).unwrap().flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(logs.len(), 2, "{logs:?}");
        assert!(logs.contains(&"sub-1_ses-1_acq-dir1_MEGRE.log".to_string()), "{logs:?}");

        run_job(&slurm_dir.join("qsmxt_aggregate.sh"));

        let local_deriv = local.join("derivatives/qsmxt");
        let (want, got) = (derivative_files(&local_deriv), derivative_files(&deriv));
        assert_eq!(want, got, "the two runs produced different files");
        for must in ["desc-reference_summary.tsv", "desc-segmentation_stats.tsv", "dataset_description.json",
                     "pipeline_config.toml", "methods.md", ".bidsignore", "README",
                     "sub-1/ses-1/anat/sub-1_ses-1_desc-cosmos_Chimap.nii"] {
            assert!(want.contains(Path::new(must)), "{must} missing from the local run: {want:?}");
        }
        assert!(want.iter().any(|p| p.starts_with("prov")), "no provenance records");

        let mut differ = Vec::new();
        for rel in &want {
            let (a, b) = (std::fs::read(local_deriv.join(rel)).unwrap(), std::fs::read(deriv.join(rel)).unwrap());
            if a == b { continue; }
            let name = rel.to_string_lossy();
            // The two outputs live in different directories, which provenance names.
            let a = match std::str::from_utf8(&a) {
                Ok(t) => t.replace(&local.display().to_string(), &dist.display().to_string()).into_bytes(),
                Err(_) => a,
            };
            let same = if a == b {
                true
            } else if name.ends_with(".json") || name.ends_with(".jsonld") {
                normalised_json(&String::from_utf8_lossy(&a)) == normalised_json(&String::from_utf8_lossy(&b))
            } else if name.ends_with(".nii") {
                // Same voxels to float precision; FFT-based solvers on a thread pool are not
                // bit-reproducible run to run.
                let (x, y) = (qsm_core::io::read_nifti_file(&local_deriv.join(rel)).unwrap().data,
                              qsm_core::io::read_nifti_file(&deriv.join(rel)).unwrap().data);
                x.len() == y.len() && x.iter().zip(&y).all(|(p, q)| (p - q).abs() <= 1e-6 * (1.0 + p.abs()))
            } else {
                false
            };
            if !same { differ.push(rel.clone()); }
        }
        assert!(differ.is_empty(), "files that differ between the local run and the SLURM jobs: {differ:?}");

        // qsmxt.log carries every worker's log, then the aggregation's own lines.
        let log = std::fs::read_to_string(deriv.join("qsmxt.log")).unwrap();
        for l in &logs {
            let worker = std::fs::read_to_string(deriv.join("logs").join(l)).unwrap();
            assert!(log.contains(&format!("==> logs/{l} <==")), "{log}");
            assert!(log.contains(&worker), "qsmxt.log is missing {l}");
        }
        // Provenance records the command the user ran, not the job's.
        let prov: String = std::fs::read_dir(deriv.join("prov")).unwrap().flatten()
            .filter(|e| e.path().is_file())
            .map(|e| std::fs::read_to_string(e.path()).unwrap_or_default()).collect();
        assert!(!prov.contains(" aggregate "), "the aggregation command leaked into provenance");
    }

    /// The cheapest pipeline: what is under test is which runs were processed, not how.
    fn cheap_pipeline() -> PipelineArgs {
        PipelineArgs {
            qsm_algorithm: Some(QsmAlgorithmArg::Tkd),
            unwrapping_algorithm: Some(UnwrapAlgorithmArg::Laplacian),
            ..Default::default()
        }
    }

    fn aggregate_args(bids: PathBuf, out: PathBuf, include: Vec<String>) -> AggregateArgs {
        AggregateArgs {
            bids_dir: bids, output_dir: Some(out), config: None, include: Some(include), exclude: None,
            num_echoes: None, pipeline: cheap_pipeline(), debug: false, force: false,
            clean_intermediates: false, command_line: None,
        }
    }

    /// `qsmxt aggregate` over a run no worker processed — or one processed with other settings —
    /// fails naming it, as a local run fails on a run that failed; the rest are still aggregated.
    #[test]
    fn aggregate_names_runs_that_were_not_processed_under_its_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_multi_orientation_bids(&bids);
        let mut worker = default_run_args(bids.clone(), out.clone());
        worker.dry = false;
        worker.no_mem_limit = true;
        worker.no_dataset_steps = true;
        worker.include = Some(vec!["*acq-dir1*".to_string()]);
        worker.pipeline = cheap_pipeline();
        super::run::execute(worker).unwrap();

        let keys = vec!["*acq-dir1*".to_string(), "*acq-dir2*".to_string()];
        let err = super::aggregate::execute(aggregate_args(bids.clone(), out.clone(), keys.clone()))
            .unwrap_err().to_string();
        assert!(err.contains("1 run(s) failed") && err.contains("acq-dir2") && err.contains("has not been processed"), "{err}");
        let deriv = out.join("derivatives/qsmxt");
        assert!(deriv.join("desc-reference_summary.tsv").exists(), "the processed run is still summarised");

        let mut other = aggregate_args(bids, out.clone(), vec!["*acq-dir1*".to_string()]);
        other.pipeline.qsm_algorithm = Some(QsmAlgorithmArg::Rts);
        let err = super::aggregate::execute(other).unwrap_err().to_string();
        assert!(err.contains("was processed with a different configuration"), "{err}");
    }

    /// Cleaning happens after the dataset-level steps, so asking a worker to clean is refused
    /// rather than silently skipped.
    #[test]
    fn a_worker_refuses_clean_intermediates() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        testutils::create_single_echo_bids(&bids);
        let mut args = default_run_args(bids, dir.path().join("out"));
        args.dry = false;
        args.no_dataset_steps = true;
        args.clean_intermediates = true;
        let err = super::run::execute(args).unwrap_err().to_string();
        assert!(err.contains("qsmxt aggregate"), "{err}");
    }

    #[test]
    fn test_validate_multi_session() {
        let dir = tempfile::tempdir().unwrap();
        testutils::create_multi_session_bids(dir.path());
        super::validate::execute(ValidateArgs {
            bids_dir: dir.path().to_path_buf(), include: None, exclude: None,
        }).unwrap();
    }

    // --- TGV's inputs (multi-echo TGV used to get the field in ppm as if it were rad) ---

    /// What a `qsmxt run` hands TGV, for a dataset with `echo_times` (s) at `b0_t` (T): the
    /// plan's TGV step (what it reads), the run's metadata and the step's parameters.
    fn tgv_wiring(echo_times: &[f64], b0_t: f64)
        -> (qsmxt_engine::plan::Step, qsmxt_engine::scan::RunMetadata, crate::pipeline::config::PipelineConfig)
    {
        let dir = tempfile::tempdir().unwrap();
        let anat = dir.path().join("sub-1/anat");
        std::fs::create_dir_all(&anat).unwrap();
        let suffix = if echo_times.len() > 1 { "MEGRE" } else { "T2starw" };
        for (e, &te) in echo_times.iter().enumerate() {
            let echo = if echo_times.len() > 1 { format!("_echo-{}", e + 1) } else { String::new() };
            for part in ["phase", "mag"] {
                let stem = anat.join(format!("sub-1{echo}_part-{part}_{suffix}"));
                if part == "phase" { testutils::write_phase(&stem.with_extension("nii")) }
                else { testutils::write_magnitude(&stem.with_extension("nii")) }
                testutils::write_sidecar(&stem.with_extension("json"), te, b0_t);
            }
        }
        let mut args = default_run_args(dir.path().to_path_buf(), dir.path().join("out"));
        args.pipeline.qsm_algorithm = Some(QsmAlgorithmArg::Tgv);
        args.pipeline.tgv_params.tgv_alpha1 = Some(0.0021);
        args.pipeline.tgv_params.tgv_alpha0 = Some(0.0037);
        let config = super::run::build_config(None, &args.pipeline).unwrap();
        let runs = crate::bids::discovery::discover_runs(dir.path(), &Default::default()).unwrap();
        let facts = crate::pipeline::requirements::scan_facts(&runs[0], &config);
        let plan = qsmxt_engine::plan::plan(&config, &facts);
        let step = plan.steps.iter()
            .find(|s| s.stage == qsmxt_engine::plan::Stage::Tgv(qsmxt_engine::plan::Pass::Main))
            .expect("a TGV run has a TGV step").clone();
        let header = qsm_core::io::read_nifti_file(&runs[0].echoes[0].phase_nifti).unwrap();
        let meta = qsmxt_engine::scan::resolve_geometry(header.dims, header.voxel_size, header.affine, &facts, &config);
        (step, meta, config)
    }

    /// Multi-echo TGV gets the B0 field map in ppm (QSM.rs#170); single-echo TGV gets echo 1's
    /// wrapped phase, with TE1 in s and B0 in T for qsm-core to convert it; the alphas are in
    /// radians at TE1 and B0 whichever the input, so both are in the step's parameters. `stage_tgv` used to pass the ppm field to a solver expecting rad at
    /// TE1, which made χ 2π·γ·B0·TE1 too small. That TGV then lands on the right scale is
    /// QSM.rs's test (`chi_comes_back_on_the_phantom_scale`, QSM.rs#204); this checks what QSMxT
    /// passes it, from the BIDS sidecars and the flags.
    #[test]
    fn tgv_gets_the_field_in_ppm_or_echo_1_phase_with_te_in_s_and_b0_in_t() {
        use qsmxt_engine::store::Item;
        use qsmxt_engine::stages::inversion::{tgv_params, tgv_params_from_config, tgv_source, TgvSource};

        let (step, meta, config) = tgv_wiring(&[0.005, 0.010, 0.015], 7.0);
        assert!(step.reads.contains(&Item::FieldPpm), "{:?}", step.reads);
        assert!(!step.reads.iter().any(|i| matches!(i, Item::Phase(_))), "{:?}", step.reads);
        assert_eq!(tgv_source(&meta), TgvSource::FieldPpm);
        let p = tgv_params(&config, &meta);
        assert_eq!(p["input"], "field_ppm", "{p}");
        assert_eq!((p["te_ms"].as_f64(), p["field_strength"].as_f64()), (Some(5.0), Some(7.0)), "{p}");
        assert_eq!(p["alphas_rad"], serde_json::json!([0.0021, 0.0037]), "{p}");
        let core = tgv_params_from_config(&config.inversion.tgv);
        assert_eq!((core.alpha1, core.alpha0), (0.0021, 0.0037));

        let (step, meta, config) = tgv_wiring(&[0.010], 7.0);
        assert!(step.reads.contains(&Item::Phase(1)), "{:?}", step.reads);
        assert!(!step.reads.contains(&Item::FieldPpm), "{:?}", step.reads);
        assert_eq!(tgv_source(&meta), TgvSource::WrappedPhase);
        // What `stages::inversion::tgv` gives qsm-core as `te_s` and `b0_t`.
        assert_eq!((meta.echo_times[0], meta.field_strength), (0.010, 7.0));
        let p = tgv_params(&config, &meta);
        assert_eq!(p["input"], "wrapped_phase", "{p}");
        assert_eq!((p["te_ms"].as_f64(), p["field_strength"].as_f64()), (Some(10.0), Some(7.0)), "{p}");
        assert_eq!(p["alphas_rad"], serde_json::json!([0.0021, 0.0037]), "{p}");
    }

    /// A requested output the data cannot produce stops the command before any run starts,
    /// naming the run and what is missing; the dry run says so too (#273).
    #[test]
    fn swi_on_a_phase_only_dataset_fails_before_processing() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_multi_echo_bids(&bids);
        for e in std::fs::read_dir(bids.join("sub-1/anat")).unwrap().flatten() {
            if e.file_name().to_string_lossy().contains("part-mag") {
                std::fs::remove_file(e.path()).unwrap();
            }
        }
        let mut args = default_run_args(bids, out.clone());
        args.pipeline.do_swi = true;
        let err = super::run::execute(args).unwrap_err().to_string();
        assert!(err.contains("sub-1") && err.contains("--do-swi") && err.contains("magnitude"), "{err}");
        assert!(!out.join("derivatives/qsmxt/sub-1").exists(), "a run was processed anyway");
    }

    /// MCPC-3D-S needs two echoes. A single-echo uncombined-coil run used to reach an assertion in
    /// qsm-core; now `qsmxt run` refuses it, and the stage itself returns an error to any direct
    /// caller (#273).
    #[test]
    fn a_single_echo_uncombined_coil_run_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path().join("bids");
        let out = dir.path().join("out");
        testutils::create_multi_coil_bids(&bids, 2);
        let mut args = default_run_args(bids.clone(), out.clone());
        args.num_echoes = Some(1);
        let err = super::run::execute(args).unwrap_err().to_string();
        assert!(err.contains("MCPC-3D-S") && err.contains("2 echoes"), "{err}");

        let filter = crate::bids::discovery::DiscoveryFilter { num_echoes: Some(1), ..Default::default() };
        let runs = crate::bids::discovery::discover_runs(&bids, &filter).unwrap();
        let coil_run = runs.iter().find(|r| r.coils.is_some()).expect("an uncombined-coil run");
        let output = crate::bids::derivatives::DerivativeOutputs::new(&out.join("direct"));
        let err = crate::pipeline::runner::run_pipeline_cached(
            coil_run, &crate::pipeline::config::PipelineConfig::default(), &output, true, &|_| {},
        ).unwrap_err().to_string();
        assert!(err.contains("MCPC-3D-S"), "{err}");
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
            args.pipeline.masking_input = Some(MaskInputArg::Magnitude);
            args.pipeline.unwrapping_algorithm = Some(UnwrapAlgorithmArg::Laplacian);
            // Output names and sidecars do not depend on the inversion, and the default (iLSQR,
            // with STI's 64 mm padding) takes minutes per run in a debug build.
            args.pipeline.qsm_algorithm = Some(QsmAlgorithmArg::Tkd);
            set(&mut args);
            super::run::execute(args).unwrap();
        };

        // Every output a multi-echo run can make: two-pass χ, the total field, SWI/SMWI, T2*/R2*,
        // R2/R2' from a MESE, chi-separation, motion correction, a (supplied) segmentation and its
        // statistics, and DICOM export — plus a GRE twin differing only by suffix.
        let bids = scratch.path().join("full");
        testutils::create_multi_echo_bids(&bids);
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
        run("full", &bids, &|a| {
            let p = &mut a.pipeline;
            p.two_pass = true;
            p.do_swi = true;
            p.do_smwi = true;
            p.do_t2starmap = true;
            p.do_r2starmap = true;
            p.do_r2map = true;
            p.do_r2primemap = true;
            p.r2prime_strategy = Some(R2PrimeStrategyArg::Mese);
            p.do_chi_separation = true;
            p.chi_separation_algorithm = Some(SeparationAlgorithmArg::R2starQsm);
            p.do_analysis = true;
            p.segmentation_params.use_custom_dseg = Some("mydseg".to_string());
            p.motion_params.motion_correction = true;
            p.export_dicom = true;
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
            a.pipeline.orientation_group = Some("acq".to_string());
        });

        // STI over six: the 4D tensor and eigenvector.
        let bids = scratch.path().join("sti");
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
        run("sti", &bids, &|a| {
            a.pipeline.orientation_group = Some("acq".to_string());
            a.pipeline.multi_orientation_algorithm = Some(MultiOrientAlgorithmArg::Sti);
            a.pipeline.multi_orientation_force = true;
        });

        // An oblique run left on its axial working grid: `space-axial` and `SpatialReference`.
        let bids = scratch.path().join("oblique");
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
        run("oblique", &bids, &|a| {
            a.pipeline.axial_resampling = true;
            a.pipeline.obliquity_threshold = Some(5.0);
            a.pipeline.output_space = Some(OutputSpaceArg::Working);
            a.pipeline.do_swi = true;
        });

        for case in ["full", "coils", "cosmos", "sti", "oblique"] {
            assert!(root.join(case).join("derivatives/qsmxt/dataset_description.json").exists(), "{case}");
        }
        let full = root.join("full/derivatives/qsmxt/sub-1/anat");
        for name in ["sub-1_acq-GRE_Chimap.nii", "sub-1_acq-MEGRE_desc-paramagneticsmwi_minIP.nii"] {
            assert!(full.join(name).exists(), "{name}");
        }
        for name in ["sub-1_acq-MEGRE_desc-total_fieldmap.nii", "sub-1_acq-MEGRE_desc-total_magnitude.nii"] {
            assert!(full.with_file_name("fmap").join(name).exists(), "{name}");
        }
        assert!(root.join("sti/derivatives/qsmxt/sub-1/anat/sub-1_desc-sti_Chimap.nii").exists());
        assert!(root.join("oblique/derivatives/qsmxt/sub-1/anat/sub-1_space-axial_Chimap.nii").exists());
    }
}
/// Tests for the multi-orientation commands (`invert cosmos`, `invert sti`).
///
/// That COSMOS and STI recover a phantom is QSM.rs's to test (`cosmos::tests::
/// recovers_chi_from_simulated_orientations`, `sti::tests::recovers_tensor_from_simulated_orientations`).
/// These check what the commands hand qsm-core: every orientation's field with its own B0
/// direction, in order, and the mask. A command that dropped an orientation, or paired a field
/// with another's direction, would not match qsm-core given the same inputs.
#[cfg(test)]
mod multiorient_tests {
    use crate::cli::*;
    use crate::testutils::multiorient as mo;
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
        use crate::bids::derivatives::DerivativeOutputs;
        use crate::bids::entities::AcquisitionKey;
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
            r2star: Some(r2star), magnitude: vec![], echo_times: vec![], r_const_3t: None,
        })).unwrap();
        for p in [pipeline.chi_para_path(&key), pipeline.chi_dia_path(&key), pipeline.chi_sep_total_path(&key)] {
            exists(name(p));
        }

        // swi --mip, with the projection left to its default name
        let swi = pipeline.swi_path(&key);
        let (phase, mag) = (d.join("phase.nii"), d.join("mag.nii"));
        crate::testutils::write_phase(&phase);
        crate::testutils::write_magnitude(&mag);
        let small_mask = d.join("small_mask.nii");
        crate::testutils::write_mask(&small_mask);
        super::swi::execute(SwiArgs {
            phase: vec![phase.clone()], magnitude: vec![mag.clone()], tes: None, mask: Some(small_mask.clone()),
            output: d.join(name(swi.clone())),
            mip: true, mip_output: None, swi_params: Default::default(),
        }).unwrap();
        exists(name(swi));
        exists(name(pipeline.swi_mip_path(&key)));

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
                crate::testutils::write_phase(&p);
                crate::testutils::write_magnitude(&m);
                phases.push(p);
                mags.push(m);
            }
        }
        super::combine::execute(CombineCommand::Mcpc3ds(CombineMcpc3dsArgs {
            phase: phases, magnitude: mags, output: d.join(unc.basename()),
            tes: Some(vec![0.005, 0.010]), num_echoes: None, sigma: None,
            hip_unwrapping: UnwrapAlgorithmArg::Laplacian,
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
}
