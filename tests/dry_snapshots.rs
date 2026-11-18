//! Snapshots of `qsmxt run --dry` (under `tests/snapshots/dry/`) on a tiny 3-echo dataset, for
//! the snapshot fixture configurations: two released config files (v1 defaults, v9.22.0),
//! QSMxT's defaults, every `--pipeline`, and a maximal config with every output on.
//!
//! The dry run shows the plan, memory estimates, the pipeline graph and the methods text a run
//! would write; it must not change in a refactor. Regenerate with
//! `QSMXT_UPDATE_SNAPSHOTS=1 cargo test --test dry_snapshots` only for an intended change, and say
//! why in the PR.

#[path = "support/snapshot.rs"]
mod snapshot;

use std::path::Path;
use std::process::Command;

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots/dry");
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/crates/qsmxt-engine/tests/fixtures");

const N: usize = 8;

/// A 3-echo 3 T MEGRE scan on an 8³ grid, magnitude and phase.
fn dataset(root: &Path) {
    let anat = root.join("sub-1/anat");
    std::fs::create_dir_all(&anat).unwrap();
    let mut affine = [0.0; 16];
    for i in 0..4 {
        affine[5 * i] = 1.0;
    }
    for (echo, te) in [0.004, 0.008, 0.012].into_iter().enumerate() {
        for (part, value) in [("phase", 0.5), ("mag", 100.0)] {
            let stem = anat.join(format!("sub-1_echo-{}_part-{part}_MEGRE", echo + 1));
            qsmxt_engine::qsm_core::io::save_nifti_to_file(
                &stem.with_extension("nii"), &vec![value; N * N * N], (N, N, N), (1.0, 1.0, 1.0), &affine,
            ).unwrap();
            let sidecar = serde_json::json!({ "EchoTime": te, "MagneticFieldStrength": 3.0 });
            std::fs::write(stem.with_extension("json"), sidecar.to_string()).unwrap();
        }
    }
}

/// What `qsmxt run <dataset> <out> --dry <flags>` prints, with the temporary paths as `<tmp>`.
fn dry(flags: &[&str]) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let bids = tmp.path().join("bids");
    dataset(&bids);
    let out = Command::new(env!("CARGO_BIN_EXE_qsmxt"))
        .arg("run").arg(&bids).arg(tmp.path().join("out"))
        .args(["--dry", "--n-procs", "1", "--mem-limit-gb", "4"])
        .args(flags)
        .env_remove("RUST_LOG")
        .output()
        .unwrap();
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() {
        text.push_str(&format!("\nexit {:?}\n{}", out.status.code(), String::from_utf8_lossy(&out.stderr)));
    }
    let text = text.replace(&tmp.path().display().to_string(), "<tmp>");
    text.lines().map(str::trim_end).collect::<Vec<_>>().join("\n") + "\n"
}

#[test]
fn qsmxt_defaults() {
    snapshot::check(DIR, "defaults.txt", &dry(&[]));
}

#[test]
fn released_and_maximal_config_files() {
    for name in ["config_v1_defaults", "config_v9_22_0", "config_maximal"] {
        let path = format!("{FIXTURES}/{name}.toml");
        snapshot::check(DIR, &format!("{name}.txt"), &dry(&["--config", &path]));
    }
}

#[test]
fn every_pipeline() {
    for id in qsmxt_engine::pipelines::ids() {
        snapshot::check(DIR, &format!("pipeline_{id}.txt"), &dry(&["--pipeline", id]));
    }
}
