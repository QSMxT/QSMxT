//! Param-coverage guard.
//!
//! qsmxt-engine is the single source of truth for the user-facing algorithm
//! parameter set (downstream consumers like qsmbly serialize these configs
//! directly instead of hand-maintaining their own lists). This test makes sure
//! the wrapper layer doesn't silently fall behind qsm-core: for each
//! `qsm_core::*Params` / `qsmxt_engine::*Config` pair, every core field must be
//! either present in the config or explicitly listed as intentionally-not-exposed.
//!
//! Add a field to a qsm-core `*Params` struct and forget the matching `*Config`,
//! and this test goes red — instead of shipping a stale default set.
//!
//! Runs only under the `introspection` feature, which turns on qsm-core's
//! (otherwise-absent) `Serialize` derives so we can read its field names:
//!   cargo test -p qsmxt-engine --features introspection
#![cfg(feature = "introspection")]

use serde::Serialize;

/// Top-level field names of a struct's serialized default.
fn field_names<T: Serialize + Default>() -> Vec<String> {
    match serde_json::to_value(T::default()).expect("serialize default") {
        serde_json::Value::Object(map) => map.keys().cloned().collect(),
        other => panic!("expected a JSON object, got {other:?}"),
    }
}

fn assert_covers<P, C>(params_name: &str, config_name: &str, ignore: &[&str])
where
    P: Serialize + Default,
    C: Serialize + Default,
{
    let core = field_names::<P>();
    let cfg = field_names::<C>();
    let missing: Vec<&String> = core
        .iter()
        .filter(|k| !cfg.iter().any(|c| c == *k) && !ignore.contains(&k.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "\n{params_name} has field(s) {missing:?} not covered by {config_name}.\n\
         Add them to {config_name}, or — if intentionally not exposed — to this \
         pair's ignore list in tests/param_coverage.rs.\n"
    );
}

macro_rules! cover {
    ($params:ty => $config:ty $(, ignore: [$($ig:literal),* $(,)?])?) => {
        assert_covers::<$params, $config>(
            stringify!($params),
            stringify!($config),
            &[$($($ig),*)?],
        );
    };
}

#[test]
fn qsmxt_engine_covers_all_qsm_core_params() {
    use qsm_core as q;
    use qsmxt_engine as c;

    // Inversion algorithms
    cover!(q::inversion::RtsParams => c::RtsConfig);
    cover!(q::inversion::TvParams => c::TvConfig);
    cover!(q::inversion::TkdParams => c::TkdConfig);
    cover!(q::inversion::TikhonovParams => c::TikhonovConfig);
    // b0/echo_times come from scan metadata (the dispatcher fills them).
    cover!(q::inversion::NltvParams => c::NltvConfig, ignore: ["b0", "echo_times"]);
    // b0/echo_times come from scan metadata. `msmv` (an Option<MsmvParams>) is flattened into
    // msmv + msmv_* (guarded below).
    cover!(q::inversion::MediParams => c::MediConfig, ignore: ["b0", "echo_times"]);
    assert_eq!(field_names::<q::inversion::MsmvParams>(), ["prefilter", "maxk", "vessel_radius"],
               "MsmvParams changed: expose it in MediConfig's msmv_* settings");
    // b0/echo_times come from scan metadata.
    cover!(q::inversion::TfiParams => c::TfiConfig, ignore: ["b0", "echo_times"]);
    // normalize_weight: AMP-PE's solution does not depend on the weight's scale (QSM.rs #253), so
    // it is left off rather than offered.
    cover!(q::inversion::AmpPeParams => c::AmpPeConfig, ignore: ["normalize_weight"]);
    // b0/echo_times come from scan metadata; is_tgv is set by the dispatcher from the algorithm.
    cover!(q::inversion::FansiParams => c::FansiConfig, ignore: ["b0", "echo_times", "is_tgv"]);
    cover!(q::inversion::L1QsmParams => c::L1qsmConfig, ignore: ["b0", "echo_times"]);
    cover!(q::inversion::WhQsmParams => c::WhqsmConfig, ignore: ["b0", "echo_times"]);
    cover!(q::inversion::HdQsmParams => c::HdqsmConfig);
    cover!(q::inversion::NdiParams => c::NdiConfig, ignore: ["b0", "echo_times"]);
    // `precision` (astewartau/QSM.rs#372) becomes a setting in QSMxT#464.
    cover!(q::inversion::IlsqrParams => c::IlsqrConfig, ignore: ["precision"]);
    cover!(q::swi::SmwiParams => c::SmwiConfig);
    // `inference` (FullVolume | SlidingWindow { patch, tile_step }) is `patch` here: None is the
    // whole volume, Some(p) a window of p at qsm-core's low-memory step (half a patch), which
    // QSMxT does not offer to change (config::synthseg_inference).
    // `head_crop` and `posteriors` are not exposed yet (#358).
    cover!(q::segment::SynthSegParams => c::SegmentationConfig, ignore: ["inference", "head_crop", "posteriors"]);
    // `b0` comes from scan metadata and `mask_output` is forced off when HEIDI consumes the
    // solution, so qsm-core's dispatcher owns both — neither is the user's to set.
    cover!(q::inversion::LsqrQsmParams => c::LsqrConfig, ignore: ["b0", "mask_output"]);
    // `denoise` is an Option<AnisotropicDiffusionParams> in qsm-core; LsqrConfig's sibling
    // flattens it into `denoise` + three `denoise_*` values, which is what the TUI and CLI show.
    // b0/echo_times come from scan metadata.
    cover!(q::inversion::HeidiParams => c::HeidiConfig, ignore: ["denoise", "b0", "echo_times"]);

    // Chi-separation methods. cf/b0 come from scan metadata; se_echo_times is set by the
    // runner from spin-echo data — none are user knobs.
    // χ-sep iLSQR's percentile pairs `prc_w_i` / `prc_w_g` are `prc_w_*_min` / `_max` here.
    cover!(q::separation::ChiSepIlsqrParams => c::ChiSepIlsqrConfig, ignore: ["cf", "prc_w_i", "prc_w_g"]);
    assert_eq!(<c::ChiSepIlsqrConfig as Default>::default().params().prc_w_i, q::separation::ChiSepIlsqrParams::default().prc_w_i);
    cover!(q::separation::ChiSepMediParams => c::ChiSepMediConfig, ignore: ["cf"]);
    cover!(q::separation::R2starQsmParams => c::R2starQsmConfig, ignore: ["b0"]);
    cover!(q::separation::WaveSepParams => c::WaveSepConfig);
    // `solvers` picks each stage's solver; qsm-core has one (lsqcurvefit's trust-region-reflective
    // method, the reference's), so there is nothing to choose yet.
    cover!(q::separation::DecomposeParams => c::DecomposeConfig, ignore: ["b0", "solvers"]);
    cover!(q::solvers::LsqTrrOptions => c::LsqcurvefitConfig);
    cover!(q::separation::HcChisepParams => c::HcChisepConfig, ignore: ["b0", "se_echo_times"]);
    cover!(q::separation::ChiSepNetParams => c::ChiSepNetConfig);
    cover!(q::separation::SusepNetParams => c::SusepNetConfig);
    // `reference_divergence` / `reference_edge_erosion` / `reference_output_box` reproduce the
    // reference package's quirks and output for parity: the pipeline runs qsm-core's defaults
    // (exact adjoint, edge erosion, χ inside the eroded mask), and only `qsmxt invert tgv` offers
    // them (--reference-divergence, --reference-edge-erosion, --reference-output-box).
    cover!(q::inversion::TgvParams => c::TgvConfig,
           ignore: ["reference_divergence", "reference_edge_erosion", "reference_output_box"]);
    // b0/echo_times come from scan metadata (the dispatcher fills them).
    cover!(q::inversion::StarParams => c::StarConfig, ignore: ["b0", "echo_times"]);

    // Background-field removal
    // `ball` picks which implementation's SMV ball to reproduce (STI Suite's, the default, or
    // QSM.jl's): the pipeline runs qsm-core's default; `qsmxt bgremove vsharp --ball` offers both.
    cover!(q::bgremove::VsharpParams => c::VsharpConfig, ignore: ["ball"]);
    cover!(q::bgremove::PdfParams => c::PdfConfig);
    cover!(q::bgremove::LbvParams => c::LbvConfig, ignore: ["max_iter"]);
    cover!(q::bgremove::IsmvParams => c::IsmvConfig);
    // `variant` picks which implementation to reproduce (SEPIA's, the default, or QSM.jl's for
    // parity): the pipeline runs qsm-core's default; `qsmxt bgremove sharp --variant` offers both.
    cover!(q::bgremove::SharpParams => c::SharpConfig, ignore: ["variant"]);
    cover!(q::bgremove::ResharpParams => c::ResharpConfig);
    cover!(q::bgremove::HarperellaParams => c::HarperellaConfig);
    cover!(q::bgremove::IharperellaParams => c::IharperellaConfig);

    // Masking / field mapping / misc
    // HdBetParams / Rs2NetParams / MindGrabParams have no *Config pair by design: a DL brain extractor's
    // parameters live on its MaskOp variant (MaskOp::HdBet, MaskOp::Rs2Net, MaskOp::MindGrab), because they are
    // per-mask-section, not global. BetParams is the odd one out — it predates mask sections
    // and still backs the --bet-* flags.
    cover!(q::bet::BetParams => c::BetConfig);
    // declared_b0 comes from the run's scan metadata; rigid is RegistrationConfig.
    cover!(q::motion::MotionParams => c::MotionCorrectionConfig, ignore: ["declared_b0", "rigid"]);
    // initial is per volume, and overwritten by chaining.
    cover!(q::registration::RigidParams => c::RegistrationConfig, ignore: ["initial"]);
    cover!(q::utils::HomogeneityParams => c::HomogeneityConfig);
    // N4 runs without a mask (on the magnitude masking is made from), and rescale_intensities
    // applies only with one, so it would change nothing.
    cover!(q::utils::N4Params => c::N4Config, ignore: ["rescale_intensities"]);
    cover!(q::utils::LinearFitParams => c::LinearFitConfig);
    // estimate_offset is LinearFitConfig's, shared.
    cover!(q::fieldmap::RobustFitParams => c::RobustFitConfig, ignore: ["estimate_offset"]);
    cover!(q::fieldmap::FitPpmComplexParams => c::ComplexFitConfig);
    // CLEAR-SWI's magnitude processing and echo weighting stay at CLEARSWI.jl's defaults.
    cover!(q::swi::SwiParams => c::SwiConfig,
        ignore: ["mag_combine", "sensitivity", "softplus", "echo_weighting"]);
    // The MCPC-3D-S phase-offset smoothing is FieldMappingConfig.phase_offset_sigma_mm, at the
    // pipeline's default (the uncombined-coil sigma is coil_combination_sigma, QSMxT's own).
    assert_eq!(c::FieldMappingConfig::default().phase_offset_sigma_mm,
               q::pipeline::FieldMappingConfig::default().phase_offset_sigma_mm);
    // Laplacian unwrapping of the raw phase: off by default, as in qsm-core.
    assert_eq!(c::FieldMappingConfig::default().laplacian_unmasked,
               q::pipeline::FieldMappingConfig::default().laplacian_unmasked);

    // QSMART: ppm/b0_dir are runtime; frangi_scale_range is reshaped into
    // frangi_scale_min/max in the config.
    cover!(q::utils::QsmartParams => c::QsmartConfig,
        ignore: ["ppm", "b0_dir", "frangi_scale_range"]);

    // ROMEO exposes a deliberate subset of the unwrapper's knobs.
    cover!(q::unwrap::RomeoParams => c::RomeoConfig,
        ignore: ["bestpath", "temporal_uncertain_unwrapping", "max_seeds",
                 "merge_regions", "correct_regions", "wrap_addition"]);
}
