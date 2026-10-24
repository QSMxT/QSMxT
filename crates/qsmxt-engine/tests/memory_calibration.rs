//! The per-step memory estimate against what was measured (`fixtures/memory_measured.json`), on
//! the example data (184×256×128, 5 echoes). Nothing is computed here: each record is planned
//! and its steps' estimates compared with the figures recorded when the steps ran.
//!
//! - `browser`: qsmbly's run worker posts the WASM heap's size as each step finishes. The heap
//!   only grows, so a step whose figure is above the one before set it, and its estimate must be
//!   within the margin; any other step's estimate must not exceed the heap by more than the margin.
//!   A record `isolated` at a step ran it in a fresh worker, its earlier steps cached.
//! - `harness`: QSM.rs's WASM harness, one network single-threaded, the heap after it (or out
//!   of memory: the estimate must then exceed the heap).
//! - `native`: allocations counted per step natively, the volumes held outside the count: the
//!   step's own need, which the native estimate is (before its margin for the allocator).
//!
//! The margin: an estimate may be at most [`UNDER`] below a measurement and at most [`OVER`]
//! above it. Under-estimating lets a run fail; over-estimating only warns too early.

use qsmxt_engine::memory::{Host, MemoryProfile, StoreKind, WASM32_HEAP_BYTES};
use qsmxt_engine::plan::{plan_for, Plan};
use qsmxt_engine::scan::ScanFacts;
use qsmxt_engine::PipelineConfig;
use serde_json::Value;

const UNDER: f64 = 0.10;
const OVER: f64 = 0.30;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/memory_measured.json")).unwrap()
}

fn facts(fx: &Value, echoes: usize) -> ScanFacts {
    let s = &fx["scan"];
    let dims: Vec<usize> = serde_json::from_value(s["dims"].clone()).unwrap();
    let tes: Vec<f64> = serde_json::from_value(s["echo_times"].clone()).unwrap();
    ScanFacts {
        dims: (dims[0], dims[1], dims[2]),
        n_echoes: echoes,
        echo_times: tes[..echoes].to_vec(),
        has_magnitude: true,
        n_coils: 1,
        real_imaginary: false,
        field_strength: s["field_strength"].as_f64().unwrap(),
        b0_dir: None,
        mese_echo_times: None,
        custom_mask: false, custom_dseg: false, custom_qsm: false, custom_r2: false, custom_r2prime: false,
    }
}

/// The record's configuration: the browser's for a browser or harness record (QSMxT's defaults
/// with the browser profile, as qsmbly starts), QSMxT's own for a native one unless it recorded
/// the profile, with the record's settings on top.
fn config(r: &Value, browser: bool) -> PipelineConfig {
    let mut c = PipelineConfig::default();
    if browser {
        c.apply_browser_profile();
    }
    for s in r["settings"].as_array().unwrap() {
        let (path, value) = (s[0].as_str().unwrap(), s[1].clone());
        qsmxt_engine::schema::set(&mut c, path, value).unwrap_or_else(|e| panic!("{}: {path}: {e}", r["name"]));
        // A record's field mapping is what it was measured with, kept under QSMART too (as a
        // front end marks a choice).
        let fm = &mut c.field_mapping;
        match path {
            "field_mapping.unwrapping_algorithm" => fm.unwrapping_chosen = true,
            "field_mapping.laplacian_solver" => fm.laplacian_solver_chosen = true,
            "field_mapping.b0_estimation" => fm.b0_estimation_chosen = true,
            _ => {}
        }
    }
    c
}

fn estimate(p: &Plan, step: &str) -> u64 {
    p.steps.iter().find(|s| s.name == step).unwrap_or_else(|| panic!("no step {step}")).memory_bytes
}

fn gb(b: f64) -> String {
    format!("{:.2} GB", b / 1e9)
}

/// `est` against `measured`: `None` when within the margin, else the line to report. Every
/// comparison is printed as a table row (`cargo test --test memory_calibration -- --nocapture`).
fn off(what: String, est: f64, measured: f64) -> Option<String> {
    let ratio = est / measured;
    println!("| {what} | {} | {} | {:+.0}% |", gb(measured), gb(est), (ratio - 1.0) * 100.0);
    (!(1.0 - UNDER..=1.0 + OVER).contains(&ratio))
        .then(|| format!("{what}: estimated {}, measured {} ({:+.0}%)", gb(est), gb(measured), (ratio - 1.0) * 100.0))
}

#[test]
fn the_estimate_is_within_the_margin_of_every_measurement() {
    let fx = fixture();
    let mut misses = Vec::new();
    let mut checked = 0;
    println!("| record and step | measured | estimated | difference |\n|---|---|---|---|");
    for r in fx["records"].as_array().unwrap() {
        let name = format!("{} {}", r["kind"].as_str().unwrap(), r["name"].as_str().unwrap());
        let threads = r["threads"].as_u64().unwrap() as usize;
        match r["kind"].as_str().unwrap() {
            "browser" => {
                // The run worker runs a plan with models on the deep-learning bundle's 4 threads.
                let c = config(r, true);
                let probe = plan_for(&c, &facts(&fx, 5), &MemoryProfile::browser(threads));
                let threads = if probe.models.is_empty() { threads } else { 4 };
                let p = plan_for(&c, &facts(&fx, 5), &MemoryProfile::browser(threads));
                let mut high = 0.0f64;
                for (i, s) in r["heap"].as_array().unwrap().iter().enumerate() {
                    let (step, heap) = (s[0].as_str().unwrap(), s[1].as_f64().unwrap());
                    let est = estimate(&p, step) as f64;
                    // The step that grew the heap set it; the first of an isolated run did too.
                    let set_it = heap > high * 1.001 && (i > 0 || !r["isolated"].is_null() || step != "load");
                    if step == "load" {
                        // Loading reads the scan and the weights into a fresh heap: nothing to model.
                    } else if set_it {
                        misses.extend(off(format!("{name} {step}"), est, heap));
                        checked += 1;
                    } else if est > heap * (1.0 + OVER) {
                        misses.push(format!("{name} {step}: estimated {}, more than the heap ever reached ({})", gb(est), gb(heap)));
                    }
                    high = high.max(heap);
                }
            }
            "harness" => {
                let p = plan_for(&config(r, true), &facts(&fx, 5), &MemoryProfile::browser(threads));
                let est = estimate(&p, r["step"].as_str().unwrap()) as f64;
                if r["out_of_memory"].as_bool() == Some(true) {
                    if est <= WASM32_HEAP_BYTES as f64 {
                        misses.push(format!("{name}: ran out of the {} heap, but estimated {}", gb(WASM32_HEAP_BYTES as f64), gb(est)));
                    }
                } else {
                    misses.extend(off(name, est, r["heap"].as_f64().unwrap()));
                }
                checked += 1;
            }
            "native" => {
                let browser_profile = r["browser_profile"].as_bool().unwrap();
                let echoes = r["echoes"].as_u64().unwrap() as usize;
                let profile = MemoryProfile { host: Host::Native, store: StoreKind::Disk, threads };
                let p = plan_for(&config(r, browser_profile), &facts(&fx, echoes), &profile);
                for s in r["need"].as_array().unwrap() {
                    let (step, need) = (s[0].as_str().unwrap(), s[1].as_f64().unwrap());
                    // Too small to matter (a header, a lookup) and allocator noise.
                    if need < 50e6 {
                        continue;
                    }
                    // The native estimate carries a 1.3× margin for the allocator and file I/O.
                    let est = estimate(&p, step) as f64 / 1.3;
                    misses.extend(off(format!("{name} {step}"), est, need));
                    checked += 1;
                }
            }
            other => panic!("unknown kind {other}"),
        }
    }
    assert!(checked > 100, "only {checked} figures checked");
    assert!(misses.is_empty(), "{} of {checked} outside -{}%/+{}%:\n{}", misses.len(), UNDER * 100.0, OVER * 100.0,
            misses.join("\n"));
}
