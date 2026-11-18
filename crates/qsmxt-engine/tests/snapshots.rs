//! Snapshots of what the engine derives from a configuration: the command line that reproduces
//! it, the methods text, and each planned step with the key it is cached under. One file per
//! fixture configuration under `tests/snapshots/`: a released config file (v1 defaults, v9.22.0),
//! QSMxT's defaults, every reference pipeline, and a maximal config with every output on.
//!
//! These must not change in a refactor. The keys are what decides whether an earlier run's
//! outputs are reused, so a changed key recomputes every user's cached steps. Regenerate with
//! `QSMXT_UPDATE_SNAPSHOTS=1 cargo test -p qsmxt-engine --test snapshots` only for an intended
//! change, and say why in the PR.

#[path = "../../../tests/support/snapshot.rs"]
mod snapshot;

use qsmxt_engine::scan::ScanFacts;
use qsmxt_engine::observe::VolumeData;
use qsmxt_engine::store::{Header, Item, MemoryStore};
use qsmxt_engine::{pipelines, PipelineConfig};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots");

const N: usize = 8;
const ECHO_TIMES: [f64; 3] = [0.004, 0.008, 0.012];

/// A 3-echo 3 T scan on an 8³ grid of 1 mm voxels, with magnitude.
fn scan() -> (ScanFacts, MemoryStore) {
    let mut affine = [0.0; 16];
    for i in 0..4 {
        affine[5 * i] = 1.0;
    }
    let header = Header { dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), affine, orientation: None, sidecar: None };
    let mut store = MemoryStore::new(header, serde_json::json!("snapshot-scan"));
    for e in 1..=ECHO_TIMES.len() {
        store.insert(Item::AcquiredPhase(e), VolumeData::Real(vec![0.5; N * N * N]));
        store.insert(Item::AcquiredMagnitude(e), VolumeData::Real(vec![100.0; N * N * N]));
    }
    let facts: ScanFacts = serde_json::from_value(serde_json::json!({
        "dims": [N, N, N], "voxel_size": [1.0, 1.0, 1.0], "n_echoes": ECHO_TIMES.len(),
        "echo_times": ECHO_TIMES, "has_magnitude": true, "n_coils": 1, "real_imaginary": false,
        "field_strength": 3.0, "nominal_field_strength": 3.0, "b0_dir": null, "mese_echo_times": null,
        "custom_mask": false, "custom_dseg": false, "custom_qsm": false, "custom_r2": false,
        "custom_r2prime": false, "manufacturer": null,
    })).expect("ScanFacts from JSON");
    (facts, store)
}

/// A config file as a release wrote it, read as `qsmxt run --config` reads it.
fn fixture(name: &str) -> PipelineConfig {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    PipelineConfig::from_toml_noting(&text).unwrap_or_else(|e| panic!("{path}: {e}")).0
}

/// Everything the snapshot holds for one configuration.
fn render(config: &PipelineConfig) -> String {
    let (facts, store) = scan();
    let mut out = String::new();
    out.push_str("## Command\n\n");
    out.push_str(&qsmxt_engine::generate_command(config));
    out.push_str("\n\n## Config hash\n\n");
    out.push_str(&qsmxt_engine::cache::config_hash(config));
    out.push_str("\n\n## Steps (dry run on a 3-echo 3 T scan: step, state, cache key)\n\n");
    let plan = qsmxt_engine::plan::plan(config, &facts);
    match qsmxt_engine::run::dry_run(&plan, config, &facts, &store) {
        Ok(steps) => {
            for s in steps {
                let state = serde_json::to_value(s.state).unwrap();
                out.push_str(&format!("{:<24} {:<10} {}\n", s.id, state.as_str().unwrap(), s.key.as_deref().unwrap_or("-")));
            }
        }
        Err(e) => out.push_str(&format!("refused: {e}\n")),
    }
    out.push_str("\n## Methods\n\n");
    out.push_str(&qsmxt_engine::generate_methods(config));
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

#[test]
fn a_v1_defaults_config_file() {
    snapshot::check(DIR, "config_v1_defaults.md", &render(&fixture("config_v1_defaults.toml")));
}

#[test]
fn a_v9_22_0_config_file() {
    snapshot::check(DIR, "config_v9_22_0.md", &render(&fixture("config_v9_22_0.toml")));
}

#[test]
fn the_maximal_config_file() {
    snapshot::check(DIR, "config_maximal.md", &render(&fixture("config_maximal.toml")));
}

#[test]
fn qsmxt_defaults() {
    snapshot::check(DIR, "defaults.md", &render(&PipelineConfig::default()));
}

/// Every pipeline a front end lists, chosen from QSMxT's defaults with nothing known of the data.
#[test]
fn every_pipeline() {
    for id in pipelines::ids() {
        let config = pipelines::target(&PipelineConfig::default(), id, &Default::default())
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        snapshot::check(DIR, &format!("pipeline_{id}.md"), &render(&config));
    }
}
