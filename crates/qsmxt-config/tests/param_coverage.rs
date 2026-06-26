//! Param-coverage guard.
//!
//! qsmxt-config is the single source of truth for the user-facing algorithm
//! parameter set (downstream consumers like qsmbly serialize these configs
//! directly instead of hand-maintaining their own lists). This test makes sure
//! the wrapper layer doesn't silently fall behind qsm-core: for each
//! `qsm_core::*Params` / `qsmxt_config::*Config` pair, every core field must be
//! either present in the config or explicitly listed as intentionally-not-exposed.
//!
//! Add a field to a qsm-core `*Params` struct and forget the matching `*Config`,
//! and this test goes red — instead of shipping a stale default set.
//!
//! Runs only under the `introspection` feature, which turns on qsm-core's
//! (otherwise-absent) `Serialize` derives so we can read its field names:
//!   cargo test -p qsmxt-config --features introspection
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
fn qsmxt_config_covers_all_qsm_core_params() {
    use qsm_core as q;
    use qsmxt_config as c;

    // Inversion algorithms
    cover!(q::inversion::RtsParams => c::RtsConfig);
    cover!(q::inversion::TvParams => c::TvConfig);
    cover!(q::inversion::TkdParams => c::TkdConfig);
    cover!(q::inversion::TikhonovParams => c::TikhonovConfig);
    cover!(q::inversion::NltvParams => c::NltvConfig);
    cover!(q::inversion::MediParams => c::MediConfig);
    cover!(q::inversion::IlsqrParams => c::IlsqrConfig);
    // fieldstrength/te are runtime scan parameters, not user knobs.
    cover!(q::inversion::TgvParams => c::TgvConfig, ignore: ["fieldstrength", "te"]);

    // Background-field removal
    cover!(q::bgremove::VsharpParams => c::VsharpConfig);
    cover!(q::bgremove::PdfParams => c::PdfConfig);
    cover!(q::bgremove::LbvParams => c::LbvConfig);
    cover!(q::bgremove::IsmvParams => c::IsmvConfig);
    cover!(q::bgremove::SharpParams => c::SharpConfig);
    cover!(q::bgremove::ResharpParams => c::ResharpConfig);
    cover!(q::pipeline::HarperellaParams => c::HarperellaConfig);

    // Masking / field mapping / misc
    cover!(q::bet::BetParams => c::BetConfig);
    cover!(q::utils::HomogeneityParams => c::HomogeneityConfig);
    cover!(q::utils::LinearFitParams => c::LinearFitConfig);
    cover!(q::swi::SwiParams => c::SwiConfig);
    cover!(q::utils::PhaseOffsetParams => c::Mcpc3dsConfig);

    // QSMART: ppm/b0_dir are runtime; frangi_scale_range is reshaped into
    // frangi_scale_min/max in the config.
    cover!(q::utils::QsmartParams => c::QsmartConfig,
        ignore: ["ppm", "b0_dir", "frangi_scale_range"]);

    // ROMEO exposes a deliberate subset of the unwrapper's knobs.
    cover!(q::unwrap::RomeoParams => c::RomeoConfig,
        ignore: ["bestpath", "temporal_uncertain_unwrapping", "max_seeds",
                 "merge_regions", "correct_regions", "wrap_addition"]);
}
