//! Pipeline configuration — re-exports from qsmxt-engine library
//! plus qsmxt-specific extensions (file I/O, CLI override mapping).

use crate::cli;
use crate::error::QsmxtError;

// Re-export everything from the library, and the checks and mappings in qsmxt-io
pub use qsmxt_io::pipeline::config::*;

/// The recipe behind a `--mask-preset` value — shared by `run` and `qsmxt mask preset`.
pub fn mask_preset_recipe(preset: cli::MaskPresetArg) -> crate::Result<MaskRecipe> {
    use clap::ValueEnum;
    let name = preset.to_possible_value().expect("no MaskPresetArg is skipped");
    qsmxt_engine::masking::mask_preset(name.get_name()).map_err(QsmxtError::Config)
}

/// Map a CLI SWI unwrapping argument to the config enum (shared by `run` and `qsmxt swi`).
pub fn swi_unwrap_arg_to_config(a: cli::SwiUnwrapArg) -> qsmxt_engine::SwiUnwrap {
    match a {
        cli::SwiUnwrapArg::Laplacian => qsmxt_engine::SwiUnwrap::Laplacian,
        cli::SwiUnwrapArg::LaplacianSlice => qsmxt_engine::SwiUnwrap::LaplacianSlice,
        cli::SwiUnwrapArg::Romeo => qsmxt_engine::SwiUnwrap::Romeo,
    }
}

/// Map a CLI SWI scaling argument to the config enum — the one place `run` and `qsmxt swi` both
/// go through, so the two cannot disagree about what a value means.
pub fn swi_scaling_arg_to_config(a: cli::SwiScalingArg) -> qsmxt_engine::SwiScaling {
    use qsmxt_engine::SwiScaling as S;
    match a {
        cli::SwiScalingArg::Tanh => S::Tanh,
        cli::SwiScalingArg::NegativeTanh => S::NegativeTanh,
        cli::SwiScalingArg::Positive => S::Positive,
        cli::SwiScalingArg::Negative => S::Negative,
        cli::SwiScalingArg::Triangular => S::Triangular,
    }
}

/// Whether `a` is LSQR+HEIDI (`heidi`), which also switches HEIDI on.
pub fn heidi_alias(a: cli::QsmAlgorithmArg) -> bool {
    a == cli::QsmAlgorithmArg::Heidi
}

/// Map a CLI dipole-inversion algorithm argument to the config enum.
pub fn qsm_algorithm_arg_to_config(a: cli::QsmAlgorithmArg) -> QsmAlgorithm {
    match a {
        cli::QsmAlgorithmArg::Rts => QsmAlgorithm::Rts,
        cli::QsmAlgorithmArg::Tv => QsmAlgorithm::Tv,
        cli::QsmAlgorithmArg::Tkd => QsmAlgorithm::Tkd,
        cli::QsmAlgorithmArg::Tgv => QsmAlgorithm::Tgv,
        cli::QsmAlgorithmArg::Tikhonov => QsmAlgorithm::Tikhonov,
        cli::QsmAlgorithmArg::Nltv => QsmAlgorithm::Nltv,
        cli::QsmAlgorithmArg::Tsvd => QsmAlgorithm::Tsvd,
        cli::QsmAlgorithmArg::Medi => QsmAlgorithm::Medi,
        cli::QsmAlgorithmArg::Tfi => QsmAlgorithm::Tfi,
        cli::QsmAlgorithmArg::Ilsqr => QsmAlgorithm::Ilsqr,
        cli::QsmAlgorithmArg::Star => QsmAlgorithm::Star,
        cli::QsmAlgorithmArg::Lsqr => QsmAlgorithm::Lsqr,
        // LSQR+HEIDI: LSQR, with HEIDI switched on by the caller ([`heidi_alias`]).
        cli::QsmAlgorithmArg::Heidi => QsmAlgorithm::Lsqr,
        cli::QsmAlgorithmArg::Qsmart => QsmAlgorithm::Qsmart,
        cli::QsmAlgorithmArg::Ndi => QsmAlgorithm::Ndi,
        cli::QsmAlgorithmArg::Fansi => QsmAlgorithm::Fansi,
        cli::QsmAlgorithmArg::FansiTgv => QsmAlgorithm::FansiTgv,
        cli::QsmAlgorithmArg::L1qsm => QsmAlgorithm::L1qsm,
        cli::QsmAlgorithmArg::Whqsm => QsmAlgorithm::Whqsm,
        cli::QsmAlgorithmArg::Hdqsm => QsmAlgorithm::Hdqsm,
        cli::QsmAlgorithmArg::AmpPe => QsmAlgorithm::AmpPe,
        cli::QsmAlgorithmArg::Xqsm => QsmAlgorithm::Xqsm,
        cli::QsmAlgorithmArg::Qsmnet => QsmAlgorithm::Qsmnet,
        cli::QsmAlgorithmArg::QsmnetPlus => QsmAlgorithm::QsmnetPlus,
        cli::QsmAlgorithmArg::Qsmnetme => QsmAlgorithm::Qsmnetme,
        cli::QsmAlgorithmArg::Autoqsm => QsmAlgorithm::Autoqsm,
        cli::QsmAlgorithmArg::Qsmgan => QsmAlgorithm::Qsmgan,
        cli::QsmAlgorithmArg::Ir2qsm => QsmAlgorithm::Ir2qsm,
        cli::QsmAlgorithmArg::Lpcnn => QsmAlgorithm::Lpcnn,
        cli::QsmAlgorithmArg::ModlQsm => QsmAlgorithm::ModlQsm,
        cli::QsmAlgorithmArg::Nextqsm => QsmAlgorithm::Nextqsm,
        cli::QsmAlgorithmArg::Iqsm => QsmAlgorithm::Iqsm,
        cli::QsmAlgorithmArg::IqsmPlus => QsmAlgorithm::IqsmPlus,
    }
}

impl From<cli::QsmartFitThresholdModeArg> for QsmartFitThresholdMode {
    fn from(m: cli::QsmartFitThresholdModeArg) -> Self {
        match m {
            cli::QsmartFitThresholdModeArg::Percentile => QsmartFitThresholdMode::Percentile,
            cli::QsmartFitThresholdModeArg::Absolute => QsmartFitThresholdMode::Absolute,
        }
    }
}

/// Apply QSMART parameter overrides onto a config. Shared by the `run` pipeline and the standalone
/// `qsmxt qsmart` command so both honour the same flags.
pub fn apply_qsmart_overrides(config: &mut PipelineConfig, p: &cli::QsmartParamArgs) -> crate::Result<()> {
    if p.qsmart_inversion.is_some_and(heidi_alias) {
        return Err(QsmxtError::Config(
            "--qsmart-inversion heidi: HEIDI is not one of QSMART's inner inversions (its two stages are its own recipe)".into()));
    }
    if let Some(v) = p.qsmart_ilsqr_pad_mm { config.inversion.qsmart.ilsqr_pad_mm = v; }
    if let Some(v) = p.qsmart_ilsqr_max_iter { config.inversion.qsmart.ilsqr_max_iter = v; }
    if let Some(v) = p.qsmart_vasc_sphere_radius { config.inversion.qsmart.vasc_sphere_radius = v; }
    if let Some(a) = p.qsmart_inversion { config.inversion.qsmart.inversion = qsm_algorithm_arg_to_config(a); }
    if let Some(v) = p.qsmart_sdf_sigma1_stage1 { config.inversion.qsmart.sdf_sigma1_stage1 = v; }
    if let Some(v) = p.qsmart_sdf_sigma2_stage1 { config.inversion.qsmart.sdf_sigma2_stage1 = v; }
    if let Some(v) = p.qsmart_sdf_sigma1_stage2 { config.inversion.qsmart.sdf_sigma1_stage2 = v; }
    if let Some(v) = p.qsmart_sdf_sigma2_stage2 { config.inversion.qsmart.sdf_sigma2_stage2 = v; }
    if let Some(v) = p.qsmart_sdf_lower_lim { config.inversion.qsmart.sdf_lower_lim = v; }
    if let Some(v) = p.qsmart_sdf_curv_constant { config.inversion.qsmart.sdf_curv_constant = v; }
    if let Some(v) = p.qsmart_frangi_scale_min { config.inversion.qsmart.frangi_scale_min = v; }
    if let Some(v) = p.qsmart_frangi_scale_max { config.inversion.qsmart.frangi_scale_max = v; }
    if let Some(v) = p.qsmart_frangi_scale_ratio { config.inversion.qsmart.frangi_scale_ratio = v; }
    if let Some(v) = p.qsmart_frangi_c { config.inversion.qsmart.frangi_c = v; }
    if let Some(m) = p.qsmart_frangi_c_mode {
        config.inversion.qsmart.frangi_c_mode = match m {
            cli::QsmartFrangiCModeArg::Absolute => qsmxt_engine::QsmartFrangiCMode::Absolute,
            cli::QsmartFrangiCModeArg::Relative => qsmxt_engine::QsmartFrangiCMode::Relative,
        };
    }
    if let Some(v) = p.qsmart_frangi_c_factor { config.inversion.qsmart.frangi_c_factor = v; }
    if let Some(u) = p.qsmart_vessel_units {
        config.inversion.qsmart.vessel_size_units = match u {
            cli::VesselSizeUnitsArg::Voxels => VesselSizeUnits::Voxels,
            cli::VesselSizeUnitsArg::Mm => VesselSizeUnits::Mm,
        };
    }
    if let Some(m) = p.qsmart_vessel_magnitude {
        config.inversion.qsmart.vessel_magnitude = match m {
            cli::QsmartVesselMagnitudeArg::MeanN4 => QsmartVesselMagnitude::MeanN4,
            cli::QsmartVesselMagnitudeArg::Combined => QsmartVesselMagnitude::Combined,
        };
    }
    if let Some(g) = p.qsmart_offset_grid {
        config.inversion.qsmart.offset_grid = match g {
            cli::QsmartOffsetGridArg::Fft => qsmxt_engine::QsmartOffsetGrid::Fft,
            cli::QsmartOffsetGridArg::QsmartReference => qsmxt_engine::QsmartOffsetGrid::QsmartReference,
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn dl_qsm_args_map_to_config() {
        use crate::cli::QsmAlgorithmArg as A;
        for a in [
            A::Xqsm, A::Qsmnet, A::QsmnetPlus, A::Qsmnetme, A::Autoqsm, A::Qsmgan, A::Ir2qsm,
            A::Lpcnn, A::ModlQsm, A::Nextqsm, A::Iqsm, A::IqsmPlus,
        ] {
            // Each DL arg maps to a distinct config algorithm whose Display is a non-empty id.
            let alg = qsm_algorithm_arg_to_config(a);
            assert!(!alg.to_string().is_empty());
        }
    }

    /// A generated QSMART command (all params) must parse back through the CLI and
    /// round-trip the QSMART config values through the flags.
    #[test]
    fn qsmart_generated_command_roundtrips() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Qsmart;
        config.inversion.qsmart.inversion = QsmAlgorithm::Tv;
        config.inversion.qsmart.sdf_sigma1_stage1 = 11.0;
        config.inversion.qsmart.sdf_lower_lim = 0.45;
        config.inversion.qsmart.frangi_scale_min = 1.5;
        config.inversion.qsmart.frangi_scale_max = 7.0;
        config.inversion.qsmart.frangi_scale_ratio = 1.0;
        config.inversion.qsmart.frangi_c = 333.0;

        let cmd = generate_command(&config);
        // Tokens after `qsmxt run <bids_dir>`.
        let argv: Vec<String> = cmd.split_whitespace().map(|s| s.to_string()).collect();
        let cli = cli::Cli::try_parse_from(&argv)
            .unwrap_or_else(|e| panic!("generated command did not parse: {}\ncmd: {}", e, cmd));

        let run_args = match cli.command {
            cli::Command::Run(a) => a,
            _ => panic!("expected Run subcommand"),
        };
        let mut rebuilt = PipelineConfig::default();
        crate::cli_settings::apply_normalised(&mut rebuilt, &run_args.pipeline).unwrap();

        assert_eq!(rebuilt.inversion.algorithm, QsmAlgorithm::Qsmart);
        assert_eq!(rebuilt.inversion.qsmart.inversion, QsmAlgorithm::Tv);
        assert_eq!(rebuilt.inversion.qsmart.sdf_sigma1_stage1, 11.0);
        assert_eq!(rebuilt.inversion.qsmart.sdf_lower_lim, 0.45);
        assert_eq!(rebuilt.inversion.qsmart.frangi_scale_min, 1.5);
        assert_eq!(rebuilt.inversion.qsmart.frangi_scale_max, 7.0);
        assert_eq!(rebuilt.inversion.qsmart.frangi_scale_ratio, 1.0);
        assert_eq!(rebuilt.inversion.qsmart.frangi_c, 333.0);
    }

    fn config_from_cli(argv: &[&str]) -> PipelineConfig {
        let cli = cli::Cli::try_parse_from(argv).expect("parse");
        let run_args = match cli.command {
            cli::Command::Run(a) => a,
            _ => panic!("expected Run subcommand"),
        };
        let mut config = PipelineConfig::default();
        crate::cli_settings::apply_normalised(&mut config, &run_args.pipeline).unwrap();
        config
    }

    /// A value the run cannot use is an error, not a warning followed by a default (#266's sweep):
    /// each of these used to be logged and ignored, so the run went ahead with something else.
    #[test]
    fn unusable_flag_values_are_refused() {
        let cases: &[&[&str]] = &[
            &["--mask", "magnitud,threshold:otsu"],
            &["--mask", "magnitude,threshhold:otsu"],
            &["--mask", "magnitude,threshold:otsu,bet"],
            &["--two-pass", "--two-pass-mask", "phase-quality,nonsense"],
            &["--mask-refine", "erode:x"],
            &["--mask-refine", "erode:2:3"],
            &["--mask", "magnitude,threshold:fixed:abc"],
            &["--mask-refine", "threshold:otsu"],
            &["--motion-correction", "--motion-reference", "middle"],
            &["--laplacian-fft-pad", "8", "8"],
            &["--ilsqr-pad-mm", "NaN"],
            &["--do-segmentation", "--synthseg-version", "v3"],
        ];
        for extra in cases {
            let mut argv = vec!["qsmxt", "run", "<bids>"];
            argv.extend_from_slice(extra);
            let cli = cli::Cli::try_parse_from(&argv).expect("clap accepts it; the override must not");
            let cli::Command::Run(run_args) = cli.command else { panic!("expected run") };
            let mut config = PipelineConfig::default();
            let err = crate::cli_settings::apply_normalised(&mut config, &run_args.pipeline);
            assert!(err.is_err(), "{extra:?} was accepted");
            let msg = format!("{}", err.unwrap_err());
            assert!(msg.contains(extra[extra.len() - 2]) || msg.contains(extra[extra.len() - 1]),
                    "{extra:?}: the error should name the flag or value: {msg}");
        }
    }

    /// The weight file follows the version, so v2 never runs v1's network output through v2's
    /// label table (#264). Both versions' weights are published in QSM.rs's registry, so both are
    /// accepted; a supplied segmentation needs no weights at all.
    #[test]
    fn synthseg_weights_follow_the_version() {
        assert_eq!(SynthSegVersion::V1.weights_id(), "synthseg");
        assert_eq!(SynthSegVersion::V2.weights_id(), "synthseg-2.0");
        for v in [SynthSegVersion::V1, SynthSegVersion::V2] {
            let spec = qsm_core::models::find_model(v.weights_id()).expect("QSM.rs lists the version");
            assert!(spec.is_available(), "{v}: weights not published in QSM.rs's registry");
        }

        let v1 = config_from_cli(&["qsmxt", "run", "<bids>", "--do-segmentation"]);
        assert_eq!(v1.segmentation.version, SynthSegVersion::V1, "v1 stays the default");
        assert!(validate_segmentation(&v1).is_ok());

        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--do-segmentation", "--synthseg-version", "v2"]);
        assert_eq!(c.segmentation.version, SynthSegVersion::V2);
        assert!(validate_segmentation(&c).is_ok());
        // A region reference turns segmentation on; `csf` is a v2 label.
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--qsm-reference", "csf", "--synthseg-version", "v2"]);
        assert!(c.pipeline.do_segmentation);
        assert!(validate_segmentation(&c).is_ok());
        // A supplied v2 segmentation is read through v2's label table and needs no weights.
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--do-segmentation", "--synthseg-version", "v2",
                                  "--use-custom-dseg", "*"]);
        assert!(validate_segmentation(&c).is_ok());
    }

    /// `--qsm-algorithm heidi` is LSQR with HEIDI on, `--heidi` turns HEIDI on after any
    /// inversion, and QSMART's inner inversion cannot be HEIDI.
    #[test]
    fn heidi_from_cli() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--qsm-algorithm", "heidi"]);
        assert_eq!((c.inversion.algorithm, c.inversion.heidi.enabled), (QsmAlgorithm::Lsqr, true));
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--qsm-algorithm", "rts", "--heidi", "--heidi-cone-threshold", "0.2"]);
        assert_eq!((c.inversion.algorithm, c.inversion.heidi.enabled, c.inversion.heidi.cone_threshold), (QsmAlgorithm::Rts, true, 0.2));
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--qsm-algorithm", "lsqr"]);
        assert!(!c.inversion.heidi.enabled);
        let cli = cli::Cli::try_parse_from(["qsmxt", "run", "<bids>", "--qsm-algorithm", "qsmart", "--qsmart-inversion", "heidi"]).unwrap();
        let cli::Command::Run(a) = cli.command else { panic!("expected run") };
        assert!(crate::cli_settings::apply_normalised(&mut PipelineConfig::default(), &a.pipeline).is_err());
    }

    #[test]
    fn assumed_decay_b0_weighting_from_cli() {
        let c = config_from_cli(&[
            "qsmxt", "run", "<bids>", "--b0-weight-type", "assumed-decay",
            "--b0-weight-assumed-t2star", "35",
        ]);
        assert_eq!(c.field_mapping.b0_weight_type, B0WeightType::AssumedDecay);
        assert_eq!(c.field_mapping.b0_weight_assumed_t2star_ms, 35.0);
        let d = config_from_cli(&["qsmxt", "run", "<bids>", "--b0-weight-type", "assumed-decay"]);
        assert_eq!(d.field_mapping.b0_weight_assumed_t2star_ms, 40.0);
        // The never-released `t2star` spelling is gone, not aliased.
        assert!(cli::Cli::try_parse_from(["qsmxt", "run", "<bids>", "--b0-weight-type", "t2star"]).is_err());
        assert!(cli::Cli::try_parse_from(["qsmxt", "run", "<bids>", "--b0-weight-t2star", "40"]).is_err());
    }

    #[test]
    fn laplacian_fft_pad_takes_one_value_or_three() {
        let base = ["qsmxt", "run", "<bids>", "--unwrapping-algorithm", "laplacian", "--laplacian-solver", "fft"];
        let with = |extra: &[&str]| config_from_cli(&[&base[..], extra].concat());
        assert_eq!(with(&[]).field_mapping.laplacian_fft_pad, [64; 3]);
        assert_eq!(with(&["--laplacian-fft-pad", "12"]).field_mapping.laplacian_fft_pad, [12; 3]);
        assert_eq!(with(&["--laplacian-fft-pad", "64", "64", "32"]).field_mapping.laplacian_fft_pad, [64, 64, 32]);
        // Two values is neither: refused (see `unusable_flag_values_are_refused`).
    }

    /// Each method's `--<method>-te`, and the method settings QSM.rs v0.40.0 added, reach the
    /// config; `auto` puts a derived setting back to its derived value.
    #[test]
    fn echo_times_and_method_settings_from_cli() {
        use qsmxt_engine::EchoTimeChoice as E;
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--nltv-te", "last", "--medi-te", "first",
            "--tfi-te", "sum", "--ndi-te", "spacing", "--fansi-te", "first", "--l1qsm-te", "12.5",
            "--whqsm-te", "last", "--heidi-te", "first", "--tgv-te", "spacing"]);
        let i = &c.inversion;
        assert_eq!([i.nltv.te, i.medi.te, i.tfi.te, i.ndi.te, i.fansi.te, i.l1qsm.te, i.whqsm.te, i.heidi.te, i.tgv.te],
                   [E::Last, E::First, E::Sum, E::Spacing, E::First, E::Seconds(0.0125), E::Last, E::First, E::Spacing]);
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--whqsm-weight", "in-vivo", "--nltv-rho", "0.2",
            "--nltv-tol-newton", "1e-5", "--rts-tau", "0.5", "--rts-gamma", "2", "--pdf-n-cg", "30",
            "--pdf-space", "kspace", "--pdf-n-pad", "0", "--harperella-pad", "1", "2", "3",
            "--iharperella-pad", "4", "5", "6", "--fansi-mu1", "0.1", "--tv-rho", "0.3"]);
        assert_eq!(c.inversion.whqsm.weight, qsmxt_engine::WhqsmWeight::InVivo);
        assert_eq!((c.inversion.nltv.rho, c.inversion.nltv.tol_newton), (Some(0.2), 1e-5));
        assert_eq!((c.inversion.rts.tau, c.inversion.rts.gamma), (0.5, 2.0));
        assert_eq!((c.bg_removal.pdf.n_cg, c.bg_removal.pdf.space, c.bg_removal.pdf.n_pad),
                   (30, qsmxt_engine::PdfSpace::Kspace, 0));
        assert_eq!((c.bg_removal.harperella.pad, c.bg_removal.iharperella.pad), ([1, 2, 3], [4, 5, 6]));
        assert_eq!((c.inversion.fansi.mu1, c.inversion.tv.rho), (Some(0.1), Some(0.3)));
        // `auto` overrides a value from a config file with the derived one.
        let args = |argv: &[&str]| match cli::Cli::try_parse_from(argv).unwrap().command {
            cli::Command::Run(r) => r.pipeline,
            _ => unreachable!(),
        };
        let mut c = PipelineConfig::default();
        (c.inversion.tv.rho, c.inversion.nltv.rho, c.inversion.hdqsm.mu1_l2) = (Some(1.0), Some(1.0), Some(1.0));
        crate::cli_settings::apply_normalised(&mut c, &args(&["qsmxt", "run", "<bids>", "--tv-rho", "auto", "--nltv-rho", "auto",
            "--hdqsm-mu1-l2", "auto"])).unwrap();
        assert_eq!((c.inversion.tv.rho, c.inversion.nltv.rho, c.inversion.hdqsm.mu1_l2), (None, None, None));
        assert_eq!(config_from_cli(&["qsmxt", "run", "<bids>", "--bf-erode-voxels", "3"]).bg_removal.erode_voxels, 3);
        assert_eq!(config_from_cli(&["qsmxt", "run", "<bids>"]).bg_removal.erode_voxels, 0);
        assert!(config_from_cli(&["qsmxt", "run", "<bids>", "--bf-polish-mask"]).bg_removal.polish_mask);
        assert!(!config_from_cli(&["qsmxt", "run", "<bids>"]).bg_removal.polish_mask);
        let polished = config_from_cli(&["qsmxt", "run", "<bids>", "--bf-polish-mask"]);
        assert!(qsmxt_engine::generate_command(&polished).contains("--bf-polish-mask"));
        assert!(config_from_cli(&["qsmxt", "run", "<bids>", "--bf-ukb-mask-trim"]).bg_removal.ukb_mask_trim);
        assert!(!config_from_cli(&["qsmxt", "run", "<bids>"]).bg_removal.ukb_mask_trim);
        let trimmed = config_from_cli(&["qsmxt", "run", "<bids>", "--bf-ukb-mask-trim"]);
        assert!(qsmxt_engine::generate_command(&trimmed).contains("--bf-ukb-mask-trim"));
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--harperella-rim", "sti", "--iharperella-rim", "whole",
            "--ndi-weight-norm", "mask-p999", "--whqsm-weight", "flat-in-mask"]);
        assert_eq!((c.bg_removal.harperella.rim, c.bg_removal.iharperella.rim),
                   (qsmxt_engine::HarperellaRim::Sti, qsmxt_engine::HarperellaRim::Whole));
        assert_eq!((c.inversion.ndi.weight_norm, c.inversion.whqsm.weight),
                   (qsmxt_engine::NdiWeightNorm::MaskP999, qsmxt_engine::WhqsmWeight::FlatInMask));
        for bad in [["--medi-te", "median"], ["--whqsm-weight", "heavy"], ["--pdf-space", "real"], ["--tv-rho", "x"],
                    ["--harperella-rim", "edge"], ["--ndi-weight-norm", "max"]] {
            assert!(cli::Cli::try_parse_from(["qsmxt", "run", "<bids>", bad[0], bad[1]]).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn inhomogeneity_method_and_n4_flags() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>"]);
        assert_eq!(c.masking.inhomogeneity_method, qsmxt_engine::InhomogeneityMethod::Makehomogeneous);
        assert_eq!(c.n4, qsmxt_engine::N4Config::default());
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--inhomogeneity-method", "n4",
            "--n4-shrink-factor", "2", "--n4-max-iterations", "20", "10", "--n4-convergence-threshold", "0.001",
            "--n4-mesh", "2", "2", "1", "--n4-spline-order", "2", "--n4-bias-field-fwhm", "0.2",
            "--n4-wiener-noise", "0.02", "--n4-histogram-bins", "100"]);
        assert_eq!(c.masking.inhomogeneity_method, qsmxt_engine::InhomogeneityMethod::N4);
        assert_eq!(c.n4, qsmxt_engine::N4Config {
            shrink_factor: 2, max_iterations: vec![20, 10], convergence_threshold: 0.001, mesh: [2, 2, 1],
            spline_order: 2, bias_field_fwhm: 0.2, wiener_noise: 0.02, histogram_bins: 100,
        });
        assert_eq!(c.masking.n4_geometry, qsmxt_engine::N4Geometry::Scanner);
    }

    /// N4's geometry from the command line and a config file, and back to the same command line.
    #[test]
    fn n4_geometry_from_the_command_line() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--inhomogeneity-method", "n4", "--n4-geometry", "make-nii"]);
        assert_eq!(c.masking.n4_geometry, qsmxt_engine::N4Geometry::MakeNii);
        let cmd = qsmxt_engine::generate_command(&c);
        assert!(cmd.contains("--n4-geometry make-nii"), "{cmd}");
        assert!(cli::Cli::try_parse_from(["qsmxt", "run", "<bids>", "--n4-geometry", "nifti"]).is_err());
        let c = PipelineConfig::from_toml("[masking]\ninhomogeneity_method = \"n4\"\nn4_geometry = \"make-nii\"\n").unwrap();
        assert_eq!(c.masking.n4_geometry, qsmxt_engine::N4Geometry::MakeNii);
        assert_eq!(PipelineConfig::from_toml("").unwrap().masking.n4_geometry, qsmxt_engine::N4Geometry::Scanner);
    }

    #[test]
    fn star_settings_from_cli() {
        let d = PipelineConfig::default();
        assert_eq!(d.inversion.star.pad, qsm_core::inversion::StarParams::default().pad);
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--qsm-algorithm", "star",
            "--star-pad", "4", "6", "10", "--star-te", "first"]);
        assert_eq!(c.inversion.algorithm, QsmAlgorithm::Star);
        assert_eq!(c.inversion.star.pad, [4, 6, 10]);
        assert_eq!(c.inversion.star.te, qsmxt_engine::EchoTimeChoice::First);
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--star-te", "12.5"]);
        assert_eq!(c.inversion.star.te, qsmxt_engine::EchoTimeChoice::Seconds(0.0125));
        assert!(cli::Cli::try_parse_from(["qsmxt", "run", "<bids>", "--star-te", "median"]).is_err());
        assert!(cli::Cli::try_parse_from(["qsmxt", "run", "<bids>", "--star-pad", "4", "4"]).is_err());
    }

    #[test]
    fn ilsqr_pad_mm_from_cli() {
        let d = config_from_cli(&["qsmxt", "run", "<bids>"]);
        assert_eq!(d.inversion.ilsqr.pad_mm, qsm_core::inversion::IlsqrParams::default().pad_mm);
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--ilsqr-pad-mm", "32"]);
        assert_eq!(c.inversion.ilsqr.pad_mm, 32.0);
        assert_eq!(c.inversion.qsmart.ilsqr_pad_mm, d.inversion.qsmart.ilsqr_pad_mm, "QSMART keeps its own");
        assert_eq!(config_from_cli(&["qsmxt", "run", "<bids>", "--ilsqr-pad-mm", "0"]).inversion.ilsqr.pad_mm, 0.0);
    }

    #[test]
    fn complex_fit_medi_csf_and_tfi_from_cli() {
        let d = config_from_cli(&["qsmxt", "run", "<bids>", "--b0-estimation", "complex-fit"]);
        assert_eq!(d.field_mapping.b0_estimation, B0Estimation::ComplexFit);
        assert_eq!(d.field_mapping.complex_fit, ComplexFitConfig::default());
        assert_eq!((d.inversion.medi.clone(), d.inversion.tfi.clone()), (MediConfig::default(), TfiConfig::default()));
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--b0-estimation", "complex-fit",
            "--complex-fit-reltol", "0.001", "--complex-fit-max-iter", "12",
            "--medi-csf", "false", "--medi-lambda-csf", "50", "--medi-csf-r2s-threshold", "4",
            "--medi-csf-erode", "false", "--medi-zeropad", "1", "2", "3", "--tfi-merit", "false", "--tfi-linear", "false"]);
        assert_eq!((c.field_mapping.complex_fit.reltol, c.field_mapping.complex_fit.max_iter), (0.001, 12));
        let m = &c.inversion.medi;
        assert_eq!((m.csf, m.lambda_csf, m.csf_r2s_threshold, m.csf_erode, m.zeropad), (false, 50.0, 4.0, false, [1, 2, 3]));
        assert_eq!((c.inversion.tfi.merit, c.inversion.tfi.linear), (false, false));
    }

    #[test]
    fn robust_fit_from_cli() {
        let d = config_from_cli(&["qsmxt", "run", "<bids>", "--b0-estimation", "robust-fit"]);
        assert_eq!(d.field_mapping.b0_estimation, B0Estimation::RobustFit);
        assert_eq!(d.field_mapping.robust_fit, RobustFitConfig::default());
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--b0-estimation", "robust-fit",
            "--robust-fit-loss", "huber", "--robust-fit-tuning", "2", "--robust-fit-iterations", "4",
            "--robust-fit-downweight-level", "0.6", "--robust-fit-flag-ratio", "5",
            "--linear-fit-estimate-offset", "false"]);
        assert_eq!(c.field_mapping.robust_fit, RobustFitConfig {
            loss: RobustLoss::Huber, tuning: Some(2.0), iterations: 4, downweight_level: 0.6, flag_ratio: 5.0,
        });
        assert!(!c.field_mapping.linear_fit.estimate_offset);
    }

    /// `--laplacian-unmasked` turns on Laplacian unwrapping of the raw phase, and nothing else.
    #[test]
    fn laplacian_unmasked_from_cli() {
        let base = ["qsmxt", "run", "<bids>", "--unwrapping-algorithm", "laplacian"];
        let fm = config_from_cli(&base).field_mapping;
        assert!(!fm.laplacian_unmasked, "off by default");
        let on = config_from_cli(&[&base[..], &["--laplacian-unmasked"]].concat()).field_mapping;
        assert!(on.laplacian_unmasked);
        assert_eq!(qsmxt_engine::FieldMappingConfig { laplacian_unmasked: false, ..on }, fm);
    }

    #[test]
    fn ignore_slice_gap_from_cli() {
        assert!(!config_from_cli(&["qsmxt", "run", "<bids>"]).pipeline.ignore_slice_gap);
        assert!(config_from_cli(&["qsmxt", "run", "<bids>", "--ignore-slice-gap"]).pipeline.ignore_slice_gap);
    }

    #[test]
    fn motion_correction_from_cli() {
        let d = config_from_cli(&["qsmxt", "run", "<bids>"]);
        assert!(!d.motion_correction.enabled, "off by default");
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--motion-correction",
            "--motion-reference", "3", "--no-motion-chain", "--motion-chain-levels", "2",
            "--motion-min-ncc", "0.9", "--registration-levels", "4", "--registration-max-samples", "1000",
            "--registration-max-iterations", "7", "--registration-max-evaluations", "99",
            "--registration-tolerance", "0.001", "--registration-rotation-step-deg", "2",
            "--registration-translation-step-mm", "1.5", "--registration-min-overlap", "0.4"]);
        let m = &c.motion_correction;
        assert!(m.enabled && !m.chain);
        assert_eq!(m.reference, qsmxt_engine::MotionReference::Echo(3));
        assert_eq!((m.chain_levels, m.min_ncc), (2, 0.9));
        let r = &c.registration;
        assert_eq!((r.levels, r.max_samples, r.max_iterations, r.max_evaluations), (4, 1000, 7, 99));
        assert_eq!((r.tolerance, r.rotation_step_deg, r.translation_step_mm, r.min_overlap), (0.001, 2.0, 1.5, 0.4));
        let l = config_from_cli(&["qsmxt", "run", "<bids>", "--motion-correction", "--motion-reference", "last"]);
        assert_eq!(l.motion_correction.reference, qsmxt_engine::MotionReference::Last);
    }

    /// The BET preset's sections (QSMART's reference mask).
    fn bet_sections() -> Vec<qsmxt_engine::MaskSection> {
        qsmxt_engine::masking::mask_presets().into_iter().find(|(n, _)| *n == "bet").unwrap().1.sections
    }

    /// QSMART runs the mask it is given, the default one included: nothing replaces it.
    #[test]
    fn qsmart_keeps_the_default_mask() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--qsm-algorithm", "qsmart"]);
        assert_eq!(c.masking.sections, default_mask_sections());
    }

    #[test]
    fn qsmart_respects_explicit_mask() {
        let c = config_from_cli(&[
            "qsmxt", "run", "<bids>", "--qsm-algorithm", "qsmart",
            "--mask", "phase-quality,threshold:otsu",
        ]);
        assert_ne!(c.masking.sections, bet_sections());
    }

    #[test]
    fn the_removed_mouse_bet_preset_says_what_to_use_instead() {
        let cli = cli::Cli::try_parse_from(["qsmxt", "run", "<bids>", "--mask-preset", "mouse-bet"])
            .expect("still parsed, so it can be explained");
        let cli::Command::Run(run_args) = cli.command else { panic!("expected Run") };
        let err = crate::cli_settings::apply_normalised(&mut PipelineConfig::default(), &run_args.pipeline)
            .expect_err("mouse-bet was removed").to_string();
        assert!(err.contains("removed") && err.contains("rs2-net") && err.contains("scale="), "{err}");
    }

    /// `--mask-preset`'s values are the engine's presets, each once.
    #[test]
    fn every_mask_preset_value_is_an_engine_preset() {
        use clap::ValueEnum;
        let shown: Vec<String> = cli::MaskPresetArg::value_variants().iter()
            .filter_map(|v| v.to_possible_value()).filter(|v| !v.is_hide_set()).map(|v| v.get_name().to_string()).collect();
        let engine: Vec<&str> = qsmxt_engine::mask_presets().into_iter().map(|(n, _)| n).collect();
        assert_eq!(shown, engine);
    }

    /// `qsmxt run --qsm-algorithm medi` runs the general default background removal; MEDI's
    /// reference (PDF) is the rules' to point out, and `--bf-algorithm` sets another.
    #[test]
    fn medi_runs_the_background_removal_it_is_given() {
        let medi = |extra: &[&str]| {
            let mut argv = vec!["qsmxt", "run", "<bids>", "--qsm-algorithm", "medi"];
            argv.extend_from_slice(extra);
            config_from_cli(&argv).normalised().0
        };
        assert_eq!(medi(&[]).bg_removal.algorithm, BfAlgorithm::Vsharp);
        assert_eq!(medi(&["--bf-algorithm", "pdf"]).bg_removal.algorithm, BfAlgorithm::Pdf);
        assert_eq!(medi(&["--bf-algorithm", "lbv"]).bg_removal.algorithm, BfAlgorithm::Lbv);
    }

    #[test]
    fn medi_smv_shrink_mask_flags() {
        let medi = |extra: &[&str]| {
            let mut argv = vec!["qsmxt", "run", "<bids>", "--qsm-algorithm", "medi"];
            argv.extend_from_slice(extra);
            config_from_cli(&argv).inversion.medi.smv_shrink_mask
        };
        assert!(medi(&[]));
        assert!(!medi(&["--no-medi-smv-shrink-mask"]));
        assert!(medi(&["--medi-smv-shrink-mask"]));
        assert!(cli::Cli::try_parse_from(["qsmxt", "run", "<bids>", "--medi-smv-shrink-mask",
            "--no-medi-smv-shrink-mask"]).is_err());
    }

    #[test]
    fn qsmart_respects_mask_preset() {
        // robust-threshold preset is an explicit choice; don't override it with BET.
        let c = config_from_cli(&[
            "qsmxt", "run", "<bids>", "--qsm-algorithm", "qsmart",
            "--mask-preset", "robust-threshold",
        ]);
        assert_eq!(c.masking.sections, default_mask_sections());
    }

    /// Under QSMART the field mapping is the one given (the general defaults unless a flag sets
    /// it); QSMART's vessel flags reach the config.
    #[test]
    fn qsmart_field_mapping_flags_choose_it() {
        let qsmart = |extra: &[&str]| {
            let mut argv = vec!["qsmxt", "run", "<bids>", "--qsm-algorithm", "qsmart"];
            argv.extend_from_slice(extra);
            config_from_cli(&argv).field_mapping
        };
        assert_eq!(qsmart(&[]), qsmxt_engine::FieldMappingConfig::default());
        let fm = qsmart(&["--unwrapping-algorithm", "laplacian", "--laplacian-solver", "mirror", "--b0-estimation", "qsmart-echofit"]);
        assert_eq!((fm.unwrapping_algorithm, fm.laplacian_solver, fm.b0_estimation),
                   (UnwrappingAlgorithm::Laplacian, LaplacianSolver::Mirror, B0Estimation::QsmartEchofit));
        let fm = qsmart(&["--unwrapping-algorithm", "laplacian", "--laplacian-solver", "dct", "--qsmart-fit-threshold", "30"]);
        assert_eq!((fm.unwrapping_algorithm, fm.laplacian_solver, fm.b0_estimation),
                   (UnwrappingAlgorithm::Laplacian, LaplacianSolver::Dct, B0Estimation::WeightedAvg));
        // A fixed residual given alone selects the fixed mode; a percentile, the percentile mode.
        assert_eq!(fm.qsmart_fit_threshold(), qsm_core::utils::QsmartFitThreshold::Absolute(30.0));
        assert_eq!(qsmart(&[]).qsmart_fit_threshold(), qsm_core::utils::QsmartFitThreshold::default());
        assert_eq!(qsmart(&["--qsmart-fit-percentile", "95"]).qsmart_fit_threshold(),
                   qsm_core::utils::QsmartFitThreshold::Percentile(95.0));
        // The mode as given wins.
        let fm = qsmart(&["--qsmart-fit-threshold-mode", "percentile", "--qsmart-fit-threshold", "30"]);
        assert_eq!(fm.qsmart_fit_threshold(), qsm_core::utils::QsmartFitThreshold::Percentile(qsm_core::utils::QSMART_FIT_PERCENTILE));
        assert_eq!(qsmart(&["--qsmart-fit-threshold-mode", "absolute"]).qsmart_fit_threshold(),
                   qsm_core::utils::QsmartFitThreshold::Absolute(qsm_core::utils::QSMART_FIT_THRESHOLD));
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--qsm-algorithm", "qsmart",
                                  "--qsmart-vessel-magnitude", "combined", "--qsmart-vessel-units", "mm"]);
        assert_eq!(c.inversion.qsmart.vessel_magnitude, QsmartVesselMagnitude::Combined);
        assert_eq!(c.inversion.qsmart.vessel_size_units, VesselSizeUnits::Mm);
        assert_eq!(c.inversion.qsmart.frangi_c(), qsm_core::utils::FrangiC::Absolute(qsm_core::utils::QSMART_FRANGI_C));
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--qsm-algorithm", "qsmart",
                                  "--qsmart-frangi-c-mode", "relative", "--qsmart-frangi-c-factor", "0.3"]);
        assert_eq!(c.inversion.qsmart.frangi_c(), qsm_core::utils::FrangiC::RelativeToMedian(0.3));
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--qsm-algorithm", "qsmart", "--qsmart-frangi-c", "50"]);
        assert_eq!(c.inversion.qsmart.frangi_c(), qsm_core::utils::FrangiC::Absolute(50.0));
        // The echo fit brings no unwrapping with it: it runs on ROMEO's echoes as given.
        let fm = config_from_cli(&["qsmxt", "run", "<bids>", "--b0-estimation", "qsmart-echofit"]).field_mapping;
        assert_eq!(fm.unwrapping_algorithm, UnwrappingAlgorithm::Romeo);
        assert!(!fm.laplacian_unmasked);
        assert!(fm.validate().is_ok());
    }

    /// The mask flags under QSMART set the mask as anywhere else.
    #[test]
    fn qsmart_mask_flags_set_the_mask() {
        let qsmart = |extra: &[&str]| {
            let mut argv = vec!["qsmxt", "run", "<bids>", "--qsm-algorithm", "qsmart"];
            argv.extend_from_slice(extra);
            config_from_cli(&argv)
        };
        let c = qsmart(&["--masking-input", "magnitude"]);
        assert!(c.masking.sections.iter().all(|s| s.input == MaskingInput::Magnitude
            && matches!(s.generator, MaskOp::Threshold { .. })));
        assert_eq!(qsmart(&["--mask-preset", "bet"]).masking.sections, bet_sections());
        let c = qsmart(&["--mask-preset", "bet", "--bet-fractional-intensity", "0.3"]);
        assert!(matches!(c.masking.sections[0].generator, MaskOp::Bet { fractional_intensity, .. }
                         if fractional_intensity == 0.3));
    }

    /// A QSMART run's saved config reloads as the same run, and so
    /// does the command it generates.
    #[test]
    fn qsmart_mask_choice_survives_the_saved_config_and_the_command() {
        let no_flags = |mut c: PipelineConfig| {
            let cli = cli::Cli::try_parse_from(["qsmxt", "run", "<bids>"]).unwrap();
            let cli::Command::Run(r) = cli.command else { panic!() };
            crate::cli_settings::apply_normalised(&mut c, &r.pipeline).unwrap();
            c
        };
        for extra in [&[][..], &["--mask-preset", "robust-threshold"], &["--mask-preset", "bet"],
                      &["--masking-input", "phase-quality"], &["--bet-fractional-intensity", "0.3"]] {
            let mut argv = vec!["qsmxt", "run", "<bids>", "--qsm-algorithm", "qsmart"];
            argv.extend_from_slice(extra);
            let c = config_from_cli(&argv);
            let reloaded = no_flags(PipelineConfig::from_toml(&c.to_toml().unwrap()).unwrap());
            assert_eq!(reloaded, c, "{extra:?}");
            let cmd = qsmxt_engine::generate_command(&c);
            let args: Vec<&str> = cmd.split_whitespace().collect();
            assert_eq!(config_from_cli(&args), c, "{extra:?}: {cmd}");
        }
    }

    #[test]
    fn non_qsmart_keeps_default_mask() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--qsm-algorithm", "rts"]);
        assert_eq!(c.masking.sections, default_mask_sections());
    }

    #[test]
    fn masking_algorithm_flag_is_rejected() {
        // Removed in favour of --mask-preset / --mask; must be a parse error,
        // not silently ignored.
        assert!(cli::Cli::try_parse_from([
            "qsmxt", "run", "<bids>", "--masking-algorithm", "bet",
        ]).is_err());
    }

    /// `--bet-voxel-scale` has to reach the BET step the recipe actually runs, whichever
    /// recipe that is — qsm-core reads `voxel_scale` off the op, not off `config.bet`.
    #[test]
    fn bet_voxel_scale_reaches_every_bet_step() {
        // `--mask-preset bet`, resolved after the flag is read.
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--mask-preset", "bet",
                                  "--bet-voxel-scale", "10"]);
        assert_eq!(c.bet.voxel_scale, 10.0);
        assert!(matches!(c.masking.sections[0].generator, MaskOp::Bet { voxel_scale, .. }
                         if voxel_scale == 10.0));

        // A BET step anywhere in a `--mask` section, and one among the combined refinements.
        let c = config_from_cli(&["qsmxt", "run", "<bids>",
                                  "--mask", "magnitude,bet:0.4,erode:2",
                                  "--mask", "phase-quality,threshold:otsu",
                                  "--bet-voxel-scale", "10"]);
        assert!(matches!(c.masking.sections[0].generator, MaskOp::Bet { voxel_scale, .. }
                         if voxel_scale == 10.0));
    }

    /// `--bet-fractional-intensity` used to be stored in `config.bet` and never reach a BET step
    /// (#279). It is now every BET step's fractional intensity, like `--bet-voxel-scale`, and a
    /// config file's `[bet]` values reach the steps the same way. BET's other parameters cannot
    /// travel on a mask step, so they are refused rather than ignored.
    #[test]
    fn bet_fractional_intensity_reaches_every_bet_step() {
        let fi = |c: &PipelineConfig| -> Vec<f64> {
            c.masking.sections.iter().map(|s| &s.generator).chain(c.masking.refinements.iter())
                .filter_map(|op| match op { MaskOp::Bet { fractional_intensity, .. } => Some(*fractional_intensity), _ => None })
                .collect()
        };
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--mask", "magnitude,bet:0.4,erode:2",
                                  "--mask", "magnitude,bet", "--bet-fractional-intensity", "0.3"]);
        assert_eq!(fi(&c), vec![0.3, 0.3]);
        // Without the flag, each step keeps its own.
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--mask", "magnitude,bet:0.4"]);
        assert_eq!(fi(&c), vec![0.4]);
        // From a config file.
        let mut file = PipelineConfig::from_toml("[bet]\nfractional_intensity = 0.35\n").unwrap();
        file.masking.sections = vec![MaskSection { input: MaskingInput::MagnitudeFirst, generator: MaskOp::bet(0.5), refinements: vec![] }];
        let cli = cli::Cli::try_parse_from(["qsmxt", "run", "<bids>"]).unwrap();
        let cli::Command::Run(r) = cli.command else { panic!() };
        crate::cli_settings::apply_normalised(&mut file, &r.pipeline).unwrap();
        assert_eq!(fi(&file), vec![0.35]);
        // BET's surface settings reach the run's config (and from there every BET step;
        // QSM.rs#180), from the command line and from a config file alike.
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--bet-smoothness", "2", "--bet-gradient-threshold",
                                  "-0.2", "--bet-iterations", "500", "--bet-subdivisions", "3"]);
        assert_eq!((c.bet.smoothness, c.bet.gradient_threshold, c.bet.iterations, c.bet.subdivisions),
                   (2.0, -0.2, 500, 3));
        let c = PipelineConfig::from_toml("[bet]\nsmoothness = 2.0\niterations = 500\n").unwrap();
        assert_eq!((c.bet.smoothness, c.bet.iterations), (2.0, 500));
    }

    /// QSMART's mask recipe and BET's orientation, from the command line and a config file, and
    /// back to the same command line.
    #[test]
    fn mask_recipe_ops_and_bet_orientation_from_the_command_line() {
        let args = ["qsmxt", "run", "<bids>", "--mask", "magnitude-first,bet:0.20,min-intensity:100,majority,close:2",
                    "--bet-orientation", "as-stored"];
        let c = config_from_cli(&args);
        assert_eq!(c.masking.sections[0].refinements,
                   [MaskOp::MinIntensity { value: 100.0 }, MaskOp::Majority, MaskOp::Close { radius: 2 }]);
        assert_eq!(c.bet.orientation, qsmxt_engine::BetOrientation::AsStored);
        let cmd = qsmxt_engine::generate_command(&c);
        assert!(cmd.contains("min-intensity:100,majority,close:2") && cmd.contains("--bet-orientation as-stored"), "{cmd}");
        assert_eq!(config_from_cli(&["qsmxt", "run", "<bids>"]).bet.orientation, qsmxt_engine::BetOrientation::Fsl);
        assert!(cli::Cli::try_parse_from(["qsmxt", "run", "<bids>", "--bet-orientation", "left"]).is_err());
        let c = PipelineConfig::from_toml("[bet]\norientation = \"as-stored\"\n").unwrap();
        assert_eq!(c.bet.orientation, qsmxt_engine::BetOrientation::AsStored);
    }

    /// The flag is the whole recipe's answer: it also wins over a `scale=` spelled out in
    /// `--mask`, so the command a saved config generates (which carries both) round-trips.
    #[test]
    fn bet_voxel_scale_flag_wins_over_a_spelled_out_scale() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>",
                                  "--mask", "magnitude,bet:0.4:scale=2",
                                  "--bet-voxel-scale", "10"]);
        assert!(matches!(c.masking.sections[0].generator, MaskOp::Bet { voxel_scale, .. }
                         if voxel_scale == 10.0));
        // Without the flag, the per-step `scale=` stands on its own.
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--mask", "magnitude,bet:0.4:scale=2"]);
        assert!(matches!(c.masking.sections[0].generator, MaskOp::Bet { voxel_scale, .. }
                         if voxel_scale == 2.0));
        assert_eq!(c.bet.voxel_scale, 1.0, "config.bet is untouched by a per-step scale");
    }

    /// Default: nothing anywhere is scaled, so no existing run changes behaviour.
    #[test]
    fn bet_voxel_scale_defaults_to_the_acquired_geometry() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--mask-preset", "bet-and-phase"]);
        assert_eq!(c.bet.voxel_scale, 1.0);
        for section in &c.masking.sections {
            for op in section.all_ops() {
                if let MaskOp::Bet { voxel_scale, .. } = op {
                    assert_eq!(voxel_scale, 1.0);
                }
            }
        }
    }

    #[test]
    fn masking_input_overrides_default_sections() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--masking-input", "magnitude"]);
        assert!(c.masking.sections.iter().all(|s| s.input == MaskingInput::Magnitude));
        // Generator and refinements keep the default recipe.
        let d = default_mask_sections();
        assert_eq!(c.masking.sections[0].generator, d[0].generator);
        assert_eq!(c.masking.sections[0].refinements, d[0].refinements);
    }

    #[test]
    fn hd_bet_mask_preset_and_sections() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--mask-preset", "hd-bet"]);
        assert_eq!(c.masking.sections, hd_bet_mask_sections());
        // --mask: hd-bet is recognised as the generator wherever it appears in the section.
        let c = config_from_cli(&[
            "qsmxt", "run", "<bids>", "--mask", "magnitude,signal-erode:0.7,hd-bet:low-memory",
        ]);
        assert_eq!(c.masking.sections[0].generator,
                   MaskOp::HdBet { patch: Some([128, 128, 64]), tta: false, tile_step: hd_bet_default_tile_step() });
        assert!(matches!(c.masking.sections[0].refinements[..], [MaskOp::SignalErode { threshold, .. }] if threshold == 0.7));
    }

    #[test]
    fn masking_input_combines_with_mask_preset() {
        let c = config_from_cli(&[
            "qsmxt", "run", "<bids>",
            "--mask-preset", "bet",
            "--masking-input", "magnitude-first",
        ]);
        assert_eq!(c.masking.sections.len(), 1);
        assert_eq!(c.masking.sections[0].input, MaskingInput::MagnitudeFirst);
        assert!(matches!(c.masking.sections[0].generator, MaskOp::Bet { .. }));
    }

    /// Round-trip a config's masking through `generate_command` and back.
    fn masking_round_trip(config: &PipelineConfig) -> (String, MaskingConfig) {
        let cmd = generate_command(config);
        let argv: Vec<String> = cmd.split_whitespace().map(|s| s.to_string()).collect();
        let cli = cli::Cli::try_parse_from(&argv)
            .unwrap_or_else(|e| panic!("generated command did not parse: {e}\ncmd: {cmd}"));
        let run_args = match cli.command { cli::Command::Run(a) => a, _ => panic!("expected Run") };
        let mut rebuilt = PipelineConfig::default();
        crate::cli_settings::apply_normalised(&mut rebuilt, &run_args.pipeline).unwrap();
        (cmd, rebuilt.masking)
    }

    /// A preclinical recipe survives the trip out to a command line and back. The command
    /// carries the scaling twice (the flag and the BET step's own `scale=`); both say 10, so
    /// it does not matter which one the rebuild reads.
    #[test]
    fn generated_command_round_trips_a_scaled_bet_recipe() {
        let mut config = PipelineConfig::default();
        config.bet.voxel_scale = 10.0;
        config.masking.sections = vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Bet { fractional_intensity: 0.4, voxel_scale: 10.0 },
            refinements: vec![MaskOp::Erode { iterations: 2 }],
        }];
        let (cmd, masking) = masking_round_trip(&config);
        assert_eq!(masking.sections, config.masking.sections, "cmd: {cmd}");
    }

    /// The generated `qsmxt run` command has to carry the combine mode and the post-combine
    /// steps, or a hand-built intersection silently runs as a union.
    #[test]
    fn generated_command_round_trips_an_and_recipe() {
        // Not one of the presets — this is the path that spells the sections out.
        let mut config = PipelineConfig::default();
        config.masking.sections = vec![
            MaskSection { input: MaskingInput::Magnitude,
                          generator: MaskOp::bet(0.4), refinements: vec![] },
            MaskSection { input: MaskingInput::Magnitude,
                          generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
                          refinements: vec![] },
        ];
        config.masking.combine = MaskCombine::And;
        config.masking.refinements = vec![MaskOp::FillHoles { max_size: 0 }];

        let (cmd, masking) = masking_round_trip(&config);
        assert!(cmd.contains("--mask-combine and"), "cmd: {cmd}");
        assert!(cmd.contains("--mask-refine fill-holes:0"), "cmd: {cmd}");
        assert_eq!(masking.sections, config.masking.sections);
        assert_eq!(masking.combine, config.masking.combine);
        assert_eq!(masking.refinements, config.masking.refinements);
    }

    /// Choosing a preset should read as choosing a preset, not as its expansion — and it still
    /// has to rebuild the same masking.
    #[test]
    fn generated_command_names_the_preset() {
        for (preset, recipe) in mask_presets() {
            let mut config = PipelineConfig::default();
            config.masking.sections = recipe.sections.clone();
            config.masking.combine = recipe.combine;
            config.masking.refinements = recipe.refinements.clone();

            let (cmd, masking) = masking_round_trip(&config);
            if preset == "robust-threshold" {
                // The default: saying it adds noise rather than removing it.
                assert!(!cmd.contains("--mask"), "default masking should print nothing: {cmd}");
            } else {
                assert!(cmd.contains(&format!("--mask-preset {preset}")), "cmd: {cmd}");
                assert!(!cmd.contains("--mask "), "preset should replace the expansion: {cmd}");
                assert!(!cmd.contains("--mask-refine"), "preset should replace the expansion: {cmd}");
            }
            assert_eq!(masking.sections, recipe.sections, "{preset} did not rebuild");
            assert_eq!(masking.combine, recipe.combine, "{preset} did not rebuild");
            assert_eq!(masking.refinements, recipe.refinements, "{preset} did not rebuild");
        }
    }

    /// A preset whose sections all read a different image is still a preset, plus one flag.
    #[test]
    fn generated_command_names_the_preset_with_an_input_override() {
        let mut config = PipelineConfig::default();
        config.masking.sections = default_mask_sections();
        for section in &mut config.masking.sections { section.input = MaskingInput::Magnitude; }

        let (cmd, masking) = masking_round_trip(&config);
        assert!(cmd.contains("--mask-preset robust-threshold"), "cmd: {cmd}");
        assert!(cmd.contains("--masking-input magnitude"), "cmd: {cmd}");
        assert_eq!(masking.sections, config.masking.sections);

        // Editing a parameter as well takes it back to the spelled-out form.
        config.masking.sections[0].refinements.push(MaskOp::Erode { iterations: 3 });
        let (cmd, masking) = masking_round_trip(&config);
        assert!(cmd.contains("--mask magnitude,"), "cmd: {cmd}");
        assert_eq!(masking.sections, config.masking.sections);
    }

    #[test]
    fn mask_combine_and_refine_from_cli() {
        let c = config_from_cli(&[
            "qsmxt", "run", "<bids>",
            "--mask", "magnitude-first,bet:0.5",
            "--mask", "phase-quality,threshold:otsu",
            "--mask-combine", "and",
            "--mask-refine", "fill-holes:0",
            "--mask-refine", "erode:1",
        ]);
        assert_eq!(c.masking.sections.len(), 2);
        assert_eq!(c.masking.combine, MaskCombine::And);
        assert_eq!(c.masking.refinements,
                   vec![MaskOp::FillHoles { max_size: 0 }, MaskOp::Erode { iterations: 1 }]);

        // Default is unchanged when the flags are absent.
        let c = config_from_cli(&["qsmxt", "run", "<bids>"]);
        assert_eq!(c.masking.combine, MaskCombine::Or);
        assert!(c.masking.refinements.is_empty());
    }

    /// An erosion, dilation or closing of 0 would do nothing: refused, naming the step.
    #[test]
    fn a_morphology_step_of_zero_is_refused_on_the_command_line() {
        for (flag, spec) in [("--mask", "magnitude,bet:0.5,erode:0"), ("--mask-refine", "dilate:0"),
                             ("--two-pass-mask", "phase-quality,threshold:otsu,close:0")] {
            let cli = cli::Cli::try_parse_from(["qsmxt", "run", "<bids>", flag, spec]).expect("parse");
            let cli::Command::Run(run) = cli.command else { panic!("expected run") };
            let e = crate::cli_settings::apply_normalised(&mut PipelineConfig::default(), &run.pipeline).unwrap_err().to_string();
            assert!(e.contains("must be at least 1, got 0"), "{flag} {spec}: {e}");
        }
    }

    #[test]
    fn two_pass_is_off_unless_asked_for() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>"]);
        assert!(!c.masking.two_pass);
        assert!(c.masking.two_pass_sections.is_none());

        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--two-pass"]);
        assert!(c.masking.two_pass);
        // Left unset, so the default recipe is what resolves — and the generated command stays
        // a bare --two-pass.
        assert!(c.masking.two_pass_sections.is_none());
        assert_eq!(c.masking.resolved_two_pass_sections(), default_two_pass_sections());
    }

    /// The command line's required changes (QSMxT's own, an output another setting needs) are
    /// applied, not refused, and recorded for the methods Note: `--qsm-reference ventricles`
    /// alone segments, `--do-smwi --no-qsm` reconstructs QSM.
    #[test]
    fn required_changes_are_applied_and_recorded() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--qsm-reference", "ventricles"]);
        assert!(c.pipeline.do_segmentation);
        assert_eq!(c.required_changes, ["Segmentation turned on: Reference = ventricles needs it (the reference region \
            is measured on a SynthSeg segmentation of the magnitude, which is saved with the outputs)"]);
        let methods = crate::pipeline::methods::generate_methods(&c);
        assert!(methods.contains("Outputs the run turned on because other settings need them:")
            && methods.contains("Segmentation turned on: Reference = ventricles needs it"), "{methods}");
        // Asked for, nothing is required or recorded.
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--qsm-reference", "ventricles", "--do-segmentation"]);
        assert!(c.pipeline.do_segmentation && c.required_changes.is_empty());

        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--do-smwi", "--no-qsm"]);
        assert!(c.pipeline.do_qsm);
        assert_eq!(c.required_changes, ["QSM turned on: SMWI needs it (SMWI weights the magnitude by the susceptibility map)"]);
        // Kept in the config file the run writes.
        assert!(c.to_toml().unwrap().contains("required_changes = ["));
    }

    /// Spelling out a reliable mask is an unambiguous request for the method; making the user
    /// pass `--two-pass` as well would only be a way to get it silently ignored.
    #[test]
    fn two_pass_mask_implies_two_pass() {
        let c = config_from_cli(&[
            "qsmxt", "run", "<bids>", "--two-pass-mask", "magnitude,threshold:percentile:30",
        ]);
        assert!(c.masking.two_pass);
        let sections = c.masking.two_pass_sections.expect("sections");
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].input, MaskingInput::Magnitude);
        assert!(matches!(sections[0].generator,
                         MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value: Some(v) } if v == 30.0));
    }

    /// The reliable mask has its own combine mode, which implies two-pass as `--two-pass-mask`
    /// does; unset, it is the main mask's, and setting one leaves the other alone.
    #[test]
    fn two_pass_mask_combine_is_the_reliable_masks_own() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--two-pass", "--mask-combine", "and"]);
        assert_eq!(c.masking.two_pass_combine, None);
        assert_eq!(c.masking.resolved_two_pass_combine(), MaskCombine::And);
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--mask-combine", "and", "--two-pass-mask-combine", "or",
                                  "--two-pass-mask", "magnitude,threshold:otsu", "--two-pass-mask", "phase-quality,threshold:otsu"]);
        assert!(c.masking.two_pass);
        assert_eq!((c.masking.combine, c.masking.two_pass_combine), (MaskCombine::And, Some(MaskCombine::Or)));
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--two-pass-mask-combine", "and"]);
        assert!(c.masking.two_pass);
        assert_eq!((c.masking.combine, c.masking.resolved_two_pass_combine()), (MaskCombine::Or, MaskCombine::And));
    }

    /// `--two-pass-mode` chooses how the second pass is made and merged, and asks for two-pass;
    /// the default is `local`.
    #[test]
    fn two_pass_mode_from_the_command_line() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--two-pass"]);
        assert_eq!(c.masking.two_pass_mode, qsmxt_engine::TwoPassMode::Local);
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--two-pass-mode", "original"]);
        assert!(c.masking.two_pass);
        assert_eq!(c.masking.two_pass_mode, qsmxt_engine::TwoPassMode::Original);
    }

    /// Two-pass leaves the main mask alone — it adds a second one rather than replacing the first.
    #[test]
    fn two_pass_does_not_disturb_the_main_mask() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--two-pass"]);
        assert_eq!(c.masking.sections, default_mask_sections());
        assert_eq!(c.masking.combine, MaskCombine::Or);
        assert!(c.masking.refinements.is_empty());
    }

    /// A reliable mask that fills its own holes reconstructs the same region as the main pass. It
    /// is honoured (the user may have a reason) but warned about rather than silently doubling the
    /// run time for nothing.
    #[test]
    fn two_pass_mask_keeps_hole_filling_the_user_asked_for() {
        let c = config_from_cli(&[
            "qsmxt", "run", "<bids>", "--two-pass-mask", "phase-quality,threshold:otsu,fill-holes:0",
        ]);
        let sections = c.masking.two_pass_sections.expect("sections");
        assert_eq!(sections[0].refinements, vec![MaskOp::FillHoles { max_size: 0 }]);

        // ...and the user is told, rather than paying for a second reconstruction in silence.
        let warnings = super::hole_closing_warnings(&sections);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("closes the holes"), "{}", warnings[0]);
        assert!(warnings[0].contains("fill-holes"), "the message should quote the recipe: {}", warnings[0]);
    }

    /// Every op that can swallow a hole has to trigger the warning, not just the obvious one —
    /// `close` bridges a hole's rim and a Gaussian smooth re-thresholds small ones away.
    #[test]
    fn every_hole_closing_op_is_warned_about() {
        let section = |op: MaskOp| MaskSection {
            input: MaskingInput::PhaseQuality,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![op],
        };
        for op in [MaskOp::FillHoles { max_size: 0 }, MaskOp::Close { radius: 1 },
                   MaskOp::GaussianSmooth { sigma_mm: 4.0 }] {
            assert_eq!(super::hole_closing_warnings(&[section(op.clone())]).len(), 1, "{op} should warn");
        }
        // Ops that only shrink or grow the mask leave its holes intact.
        for op in [MaskOp::Erode { iterations: 1 }, MaskOp::Dilate { iterations: 1 }] {
            assert!(super::hole_closing_warnings(&[section(op.clone())]).is_empty(), "{op} should not warn");
        }
        // The default recipe must be silent, or the warning is noise.
        assert!(super::hole_closing_warnings(&default_two_pass_sections()).is_empty());
    }

    /// A generator in --mask-refine has nothing to refine — it would throw the combined mask
    /// away. It is dropped with a warning rather than silently replacing the mask.
    #[test]
    fn mask_refine_takes_refinements() {
        // A generator is refused (see `unusable_flag_values_are_refused`); refinements are kept.
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--mask-refine", "erode:2"]);
        assert_eq!(c.masking.refinements, vec![MaskOp::Erode { iterations: 2 }]);
    }

    #[test]
    fn bet_and_phase_preset_is_a_whole_recipe() {
        let c = config_from_cli(&["qsmxt", "run", "<bids>", "--mask-preset", "bet-and-phase"]);
        let recipe = bet_and_phase_mask_recipe();
        assert_eq!(c.masking.sections, recipe.sections);
        assert_eq!(c.masking.combine, recipe.combine);
        assert_eq!(c.masking.refinements, recipe.refinements);

        // A single-section preset over a two-section config clears the combine mode and the
        // post-combine steps, rather than leaving them to apply to one section.
        let cli = cli::Cli::try_parse_from(["qsmxt", "run", "<bids>", "--mask-preset", "bet"]).expect("parse");
        let run_args = match cli.command { cli::Command::Run(a) => a, _ => panic!("expected Run") };
        let mut config = PipelineConfig {
            masking: MaskingConfig { sections: recipe.sections, combine: recipe.combine,
                                     refinements: recipe.refinements, ..MaskingConfig::default() },
            ..PipelineConfig::default()
        };
        crate::cli_settings::apply_normalised(&mut config, &run_args.pipeline).unwrap();
        assert_eq!(config.masking.combine, MaskCombine::Or);
        assert!(config.masking.refinements.is_empty());
    }

    #[test]
    fn masking_input_ignored_with_explicit_mask_sections() {
        let c = config_from_cli(&[
            "qsmxt", "run", "<bids>",
            "--mask", "phase-quality,threshold:otsu",
            "--masking-input", "magnitude",
        ]);
        assert_eq!(c.masking.sections[0].input, MaskingInput::PhaseQuality);
    }

    #[test]
    fn mask_erosions_flag_is_rejected() {
        // Removed alongside --masking-algorithm; erosion belongs in --mask
        // sections (erode:N).
        assert!(cli::Cli::try_parse_from([
            "qsmxt", "run", "<bids>", "--mask-erosions", "3",
        ]).is_err());
    }

    /// A browser run is reproducible as a command: the browser profile's values reach the
    /// generated `qsmxt run` command, and the command rebuilds them.
    #[test]
    fn browser_profile_round_trips_through_the_command() {
        use qsmxt_engine::profile::{BROWSER_R2PRIMENET_PATCH, BROWSER_TILE_HALO, BROWSER_TILE_SIZE};
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Xqsm;
        c.masking.sections = qsmxt_engine::masking::hd_bet_mask_sections();
        c.pipeline.do_r2primemap = true;
        c.separation.r2prime_strategy = qsmxt_engine::R2PrimeStrategy::R2primenet;
        let _ = c.apply_browser_profile();

        let cmd = generate_command(&c);
        let argv: Vec<String> = cmd.split_whitespace().map(|s| s.to_string()).collect();
        let cli = cli::Cli::try_parse_from(&argv)
            .unwrap_or_else(|e| panic!("generated command did not parse: {e}\ncmd: {cmd}"));
        let cli::Command::Run(run_args) = cli.command else { panic!("expected Run") };
        let mut rebuilt = PipelineConfig::default();
        crate::cli_settings::apply_normalised(&mut rebuilt, &run_args.pipeline).unwrap();

        assert_eq!(rebuilt.inversion.tile_size, Some(BROWSER_TILE_SIZE), "{cmd}");
        assert_eq!(rebuilt.inversion.tile_halo, Some(BROWSER_TILE_HALO), "{cmd}");
        assert_eq!(rebuilt.separation.r2primenet.patch, BROWSER_R2PRIMENET_PATCH, "{cmd}");
        assert_eq!(rebuilt.masking.sections, c.masking.sections, "{cmd}");
        assert_eq!(generate_command(&rebuilt), cmd);
    }

    /// A config on a pipeline is `--pipeline <id>` plus its departures, and the command rebuilds
    /// it: for every pipeline, as chosen and with a departure.
    #[test]
    fn pipelines_round_trip_through_the_command() {
        use qsmxt_engine::pipelines;
        let facts = qsmxt_engine::schema::ViewFacts::default();
        let rebuild = |cmd: &str| {
            let argv: Vec<&str> = cmd.split_whitespace().collect();
            let cli = cli::Cli::try_parse_from(&argv).unwrap_or_else(|e| panic!("{e}\ncmd: {cmd}"));
            let cli::Command::Run(run_args) = cli.command else { panic!("expected Run") };
            let mut rebuilt = PipelineConfig::default();
            crate::cli_settings::apply_normalised(&mut rebuilt, &run_args.pipeline).unwrap();
            rebuilt
        };
        for p in pipelines::pipelines().into_iter().filter(|p| p.is_toolbox()) {
            let mut c = PipelineConfig::default();
            pipelines::choose(&mut c, p.id, &facts).unwrap();
            let cmd = generate_command(&c);
            assert_eq!(cmd, format!("qsmxt run <bids_dir> --pipeline {}", p.id));
            assert_eq!(generate_command(&rebuild(&cmd)), cmd);
            assert!(pipelines::status(&rebuild(&cmd), &facts).departures.is_empty(), "{cmd}");
            // Another setting: its flag follows.
            c.pipeline.do_swi = true;
            let cmd = generate_command(&c);
            assert_eq!(cmd, format!("qsmxt run <bids_dir> --pipeline {} --do-swi", p.id));
            assert_eq!(generate_command(&rebuild(&cmd)), cmd);
        }
        // A departure from a pipeline's ROMEO settings back to QSMxT's (the χ-separation toolbox's
        // romeo3 weights and no global correction) has a flag, which reads back.
        let mut c = PipelineConfig::default();
        pipelines::choose(&mut c, "chi-sep-ilsqr", &facts).unwrap();
        assert!(!c.field_mapping.romeo.phase_linearity && !c.field_mapping.romeo.correct_global);
        (c.field_mapping.romeo.phase_linearity, c.field_mapping.romeo.correct_global) = (true, true);
        let cmd = generate_command(&c);
        assert_eq!(cmd, "qsmxt run <bids_dir> --pipeline chi-sep-ilsqr --romeo-correct-global --romeo-phase-linearity");
        let back = rebuild(&cmd);
        assert!(back.field_mapping.romeo.phase_linearity && back.field_mapping.romeo.correct_global);
        assert_eq!(generate_command(&back), cmd);
        assert!(cli::Cli::try_parse_from(["qsmxt", "run", "d", "--romeo-phase-linearity", "--no-romeo-phase-linearity"]).is_err());
        // The flags after --pipeline win.
        let c = rebuild("qsmxt run <bids_dir> --pipeline medi-toolbox --bf-algorithm vsharp");
        assert_eq!(c.bg_removal.algorithm, qsmxt_engine::BfAlgorithm::Vsharp);
        assert_eq!(pipelines::status(&c, &facts).text(), "MEDI toolbox · 1 departure");
    }

    /// The browser's R2PRIMEnet patch is the one qsm-core sizes for a WASM heap.
    #[cfg(feature = "dl")]
    #[test]
    fn browser_r2primenet_patch_is_qsm_cores_wasm_patch() {
        let (x, y, z) = qsm_core::relaxometry::WASM_PATCH;
        assert_eq!(qsmxt_engine::profile::BROWSER_R2PRIMENET_PATCH, [x, y, z]);
    }

    /// Members whose inversion skips background removal have no local field to combine; that is
    /// refused before processing rather than after every member has run (#270).
    #[test]
    fn an_inversion_without_background_removal_is_refused_for_a_group() {
        use crate::pipeline::config::{validate_multi_orientation, QsmAlgorithm};
        let mut config = PipelineConfig::default();
        config.multi_orientation.group_by = "acq".into();
        assert!(validate_multi_orientation(&config).is_ok());
        // MEDI takes the local field with every SMV option.
        config.inversion.algorithm = QsmAlgorithm::Medi;
        config.inversion.medi.smv = true;
        assert!(validate_multi_orientation(&config).is_ok());
        config.inversion.algorithm = QsmAlgorithm::Tgv;
        assert!(validate_multi_orientation(&config).unwrap_err().to_string().contains("tgv"));
        config.inversion.algorithm = QsmAlgorithm::Rts;
        config.pipeline.do_qsm = false;
        assert!(validate_multi_orientation(&config).unwrap_err().to_string().contains("--no-qsm"));
    }
}
