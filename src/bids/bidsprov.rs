//! BEP028 (BIDS-Prov) provenance output.
//!
//! After a run completes, this module aggregates the per-step `provenance.json`
//! files that the pipeline writes under `workflow/.../<step>/` into a set of
//! BIDS-Prov record files under `derivatives/qsmxt/prov/`, and writes a
//! `GeneratedBy` sidecar next to each final output.
//!
//! The split-file layout follows the BEP028 spec (v0.0.1): each of
//! `prov-qsmxt_{base,soft,env,act,ent}.json` holds one class of records as a
//! flat JSON object keyed by the record's IRI (the key *is* the `Id`). See the
//! spec at <https://github.com/bids-standard/BEP028_BIDSprov>.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use log::warn;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::bids::derivatives::DerivativeOutputs;
use crate::bids::discovery::QsmRun;
use crate::bids::entities::{self, AcquisitionKey};

const BIDSPROV_VERSION: &str = "0.0.1";
const CONTEXT_URL: &str = "https://purl.org/nidash/bidsprov/context.json";

/// Pipeline steps that emit a `provenance.json` (matches the `complete_step`
/// calls in `pipeline::runner`). `load` is excluded — it records no step.
const STEPS: &[&str] = &[
    "scale_phase", "magnitude", "mask", "swi", "t2star_r2star",
    "unwrap", "tgv", "qsmart", "bgremove", "invert", "reference",
];

/// Deserialization view of the per-step `provenance.json` written by
/// `pipeline::runner::Provenance`. Extra fields (e.g. `peak_memory_bytes`) are
/// ignored.
#[derive(Deserialize)]
struct StepProvenance {
    #[allow(dead_code)]
    step: String,
    algorithm: Option<String>,
    parameters: Value,
    inputs: Vec<String>,
    outputs: Vec<String>,
    duration_secs: f64,
    timestamp: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct AgentRecord {
    label: String,
    version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct EnvironmentRecord {
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    operating_system: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ActivityRecord {
    label: String,
    command: String,
    #[serde(skip_serializing_if = "Value::is_null")]
    parameters: Value,
    associated_with: String,
    used: Vec<String>,
    started_at_time: String,
    ended_at_time: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct EntityRecord {
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    at_location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generated_by: Option<String>,
}

/// Compute the lexical relative path from `base` to `target`.
///
/// Purely lexical (no filesystem access), so it is stable regardless of whether
/// the target still exists — both paths are built from the same base directory,
/// so shared leading components cancel and divergences become `..` segments.
fn relative_path(base: &Path, target: &Path) -> PathBuf {
    let b: Vec<_> = base.components().collect();
    let t: Vec<_> = target.components().collect();
    let mut i = 0;
    while i < b.len() && i < t.len() && b[i] == t[i] {
        i += 1;
    }
    let mut out = PathBuf::new();
    for _ in i..b.len() {
        out.push("..");
    }
    for c in &t[i..] {
        out.push(c.as_os_str());
    }
    out
}

/// Entity IRI for a file: `bids::<relative-path-from-derivatives-root>`.
fn entity_iri(derivatives_dir: &Path, file: &str) -> String {
    let rel = relative_path(derivatives_dir, Path::new(file));
    format!("bids::{}", rel.to_string_lossy().replace('\\', "/"))
}

/// Dataset-relative location string used for an entity's `AtLocation`.
fn at_location(derivatives_dir: &Path, file: &str) -> String {
    relative_path(derivatives_dir, Path::new(file))
        .to_string_lossy()
        .replace('\\', "/")
}

fn file_label(file: &str) -> String {
    Path::new(file)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.to_string())
}

fn activity_iri(step: &str, key: &AcquisitionKey) -> String {
    format!("bids::prov/#{}-{}", step, key.basename())
}

/// Split a step's end `timestamp` (RFC3339) and `duration_secs` into
/// `(StartedAtTime, EndedAtTime)`. Falls back to the raw timestamp for both if
/// it cannot be parsed.
fn start_end(timestamp: &str, duration_secs: f64) -> (String, String) {
    match chrono::DateTime::parse_from_rfc3339(timestamp) {
        Ok(end) => {
            let started = end - chrono::Duration::milliseconds((duration_secs * 1000.0) as i64);
            (started.to_rfc3339(), end.to_rfc3339())
        }
        Err(_) => (timestamp.to_string(), timestamp.to_string()),
    }
}

/// Per-stage `Parameters`: the step's parameters with its `algorithm` folded in
/// (so the stage detail is preserved once `Command` holds the top-level argv).
fn stage_parameters(algorithm: Option<&str>, parameters: &Value) -> Value {
    let mut params = parameters.clone();
    if let (Some(alg), Value::Object(map)) = (algorithm, &mut params) {
        map.insert("algorithm".to_string(), Value::String(alg.to_string()));
    }
    params
}

/// A final output and how to describe it in its sidecar.
struct FinalOutput {
    path: PathBuf,
    /// Step (activity) that produced this file.
    step: &'static str,
    /// Whether acquisition metadata (field strength / echo time) is still valid
    /// for this output and should be forwarded into the sidecar.
    forward_meta: bool,
    /// `SkullStripped` value, or `None` for `mask` (where the key does not apply
    /// per the BIDS derivatives schema).
    skull_stripped: Option<bool>,
}

/// Final outputs and how each should be described.
///
/// `SkullStripped` (required for derivative anat images except `mask`) reflects
/// whether the pipeline confines the output to the brain mask:
/// - Chimap is referenced within the mask, and T2star*/R2star* are masked
///   explicitly, so they are skull-stripped;
/// - the RSS magnitude (T2starw) and SWI/minIP retain full field-of-view signal.
fn final_outputs(output: &DerivativeOutputs, key: &AcquisitionKey) -> Vec<FinalOutput> {
    vec![
        FinalOutput { path: output.qsm_path(key), step: "reference", forward_meta: true, skull_stripped: Some(true) },
        FinalOutput { path: output.mask_path(key), step: "mask", forward_meta: false, skull_stripped: None },
        FinalOutput { path: output.magnitude_path(key), step: "magnitude", forward_meta: true, skull_stripped: Some(false) },
        FinalOutput { path: output.swi_path(key), step: "swi", forward_meta: true, skull_stripped: Some(false) },
        FinalOutput { path: output.swi_mip_path(key), step: "swi", forward_meta: true, skull_stripped: Some(false) },
        FinalOutput { path: output.t2star_path(key), step: "t2star_r2star", forward_meta: true, skull_stripped: Some(true) },
        FinalOutput { path: output.r2star_path(key), step: "t2star_r2star", forward_meta: true, skull_stripped: Some(true) },
    ]
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> crate::Result<()> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| crate::error::QsmxtError::Config(format!("Failed to serialize {}: {}", path.display(), e)))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, json)?;
    Ok(())
}

fn environment_record() -> (String, EnvironmentRecord) {
    let iri = "bids::prov/#environment-runtime".to_string();
    // qsmxt is a self-contained static binary with no external runtime
    // dependencies, so the environment record only records the OS.
    let rec = EnvironmentRecord {
        label: "qsmxt runtime environment".to_string(),
        operating_system: Some(format!("{} ({})", std::env::consts::OS, std::env::consts::ARCH)),
    };
    (iri, rec)
}

/// Software (Agent) records: qsmxt itself and its bundled reconstruction
/// library QSM.rs (the `qsm-core` crate, compiled into the binary — shipped
/// with qsmxt rather than an external dependency, but independently versioned).
fn agent_records() -> Vec<(String, AgentRecord)> {
    let qsmxt_version = env!("CARGO_PKG_VERSION");
    let core_version = env!("QSM_CORE_VERSION");
    vec![
        (
            format!("bids::prov/#qsmxt-{}", qsmxt_version),
            AgentRecord { label: "qsmxt".to_string(), version: qsmxt_version.to_string() },
        ),
        (
            format!("bids::prov/#qsm-core-{}", core_version),
            AgentRecord { label: "QSM.rs".to_string(), version: core_version.to_string() },
        ),
    ]
}

/// Write BIDS-Prov records (`prov/`) and per-output `GeneratedBy` sidecars.
///
/// `command` is the actual top-level invocation that ran the pipeline (the real
/// `qsmxt run …` argv). BEP028 provenance is retrospective, so every Activity's
/// `Command` is this literal command — the one that genuinely produced all the
/// outputs — while `Label` names the stage and `Parameters` holds its settings.
///
/// Best-effort: individual unreadable `provenance.json` files are skipped with a
/// warning rather than failing the whole write.
pub fn write_provenance(
    derivatives_dir: &Path,
    runs: &[QsmRun],
    output: &DerivativeOutputs,
    command: &str,
) -> crate::Result<()> {
    let agents = agent_records();
    // qsmxt (the invoked tool) is the agent activities are AssociatedWith.
    let agent_iri = agents[0].0.clone();
    let (env_iri, environment) = environment_record();

    let mut activities: Map<String, Value> = Map::new();
    let mut entities: Map<String, Value> = Map::new();

    // Pass 1: read every step's provenance.json; register output entities keyed
    // to their producing activity, and build a path -> activity index so that a
    // later step's input can be linked to the activity that produced it.
    struct ParsedStep {
        step: &'static str,
        activity: String,
        prov: StepProvenance,
    }
    let mut parsed: Vec<ParsedStep> = Vec::new();
    let mut produced_by: BTreeMap<String, String> = BTreeMap::new();

    for run in runs {
        for &step in STEPS {
            let path = output.provenance_path(&run.key, step);
            if !path.exists() {
                continue;
            }
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    warn!("BIDS-Prov: cannot read {}: {}", path.display(), e);
                    continue;
                }
            };
            let prov: StepProvenance = match serde_json::from_str(&text) {
                Ok(p) => p,
                Err(e) => {
                    warn!("BIDS-Prov: cannot parse {}: {}", path.display(), e);
                    continue;
                }
            };
            let activity = activity_iri(step, &run.key);
            for out in &prov.outputs {
                produced_by.insert(out.clone(), activity.clone());
            }
            parsed.push(ParsedStep { step, activity, prov });
        }
    }

    // Output entities: generated by their producing activity.
    for ps in &parsed {
        for out in &ps.prov.outputs {
            let iri = entity_iri(derivatives_dir, out);
            let rec = EntityRecord {
                label: file_label(out),
                at_location: Some(at_location(derivatives_dir, out)),
                generated_by: Some(ps.activity.clone()),
            };
            entities.insert(iri, serde_json::to_value(rec).unwrap_or(Value::Null));
        }
    }

    // Activities + input (Used) entities.
    for ps in &parsed {
        let (started, ended) = start_end(&ps.prov.timestamp, ps.prov.duration_secs);
        let mut used = vec![env_iri.clone()];
        for inp in &ps.prov.inputs {
            let iri = entity_iri(derivatives_dir, inp);
            entities.entry(iri.clone()).or_insert_with(|| {
                let rec = EntityRecord {
                    label: file_label(inp),
                    at_location: Some(at_location(derivatives_dir, inp)),
                    generated_by: produced_by.get(inp).cloned(),
                };
                serde_json::to_value(rec).unwrap_or(Value::Null)
            });
            used.push(iri);
        }
        let activity = ActivityRecord {
            label: ps.step.to_string(),
            command: command.to_string(),
            parameters: stage_parameters(ps.prov.algorithm.as_deref(), &ps.prov.parameters),
            associated_with: agent_iri.clone(),
            used,
            started_at_time: started,
            ended_at_time: ended,
        };
        activities.insert(ps.activity.clone(), serde_json::to_value(activity).unwrap_or(Value::Null));
    }

    // Write the prov/ split files.
    let prov_dir = derivatives_dir.join("prov");
    let base = serde_json::json!({
        "@context": CONTEXT_URL,
        "BIDSProvVersion": BIDSPROV_VERSION,
    });
    write_json(&prov_dir.join("prov-qsmxt_base.json"), &base)?;

    let mut soft = Map::new();
    for (iri, rec) in &agents {
        soft.insert(iri.clone(), serde_json::to_value(rec).unwrap_or(Value::Null));
    }
    write_json(&prov_dir.join("prov-qsmxt_soft.json"), &Value::Object(soft))?;

    let mut env = Map::new();
    env.insert(env_iri.clone(), serde_json::to_value(&environment).unwrap_or(Value::Null));
    write_json(&prov_dir.join("prov-qsmxt_env.json"), &Value::Object(env))?;

    write_json(&prov_dir.join("prov-qsmxt_act.json"), &Value::Object(activities))?;
    write_json(&prov_dir.join("prov-qsmxt_ent.json"), &Value::Object(entities))?;

    // Per-output GeneratedBy sidecars.
    for run in runs {
        for fo in final_outputs(output, &run.key) {
            if !fo.path.exists() {
                continue;
            }
            let Some(sidecar) = entities::sidecar_path(&fo.path) else {
                continue;
            };
            let mut obj = Map::new();
            // Required for derivative anat images (except mask).
            if let Some(ss) = fo.skull_stripped {
                obj.insert("SkullStripped".to_string(), Value::Bool(ss));
            }
            if fo.forward_meta {
                obj.insert(
                    "MagneticFieldStrength".to_string(),
                    serde_json::json!(run.magnetic_field_strength),
                );
                // EchoTime remains valid only when a single echo was used.
                if run.echo_times.len() == 1 {
                    obj.insert("EchoTime".to_string(), serde_json::json!(run.echo_times[0]));
                }
            }
            obj.insert(
                "GeneratedBy".to_string(),
                Value::String(activity_iri(fo.step, &run.key)),
            );
            write_json(&sidecar, &Value::Object(obj))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bids::entities::AcquisitionKey;

    fn key() -> AcquisitionKey {
        AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        }
    }

    #[test]
    fn test_relative_path_into_subdir() {
        let rel = relative_path(Path::new("/out/derivatives/qsmxt"), Path::new("/out/derivatives/qsmxt/sub-01/anat/x.nii"));
        assert_eq!(rel, PathBuf::from("sub-01/anat/x.nii"));
    }

    #[test]
    fn test_relative_path_to_parent_raw_input() {
        let rel = relative_path(Path::new("/out/derivatives/qsmxt"), Path::new("/out/sub-01/anat/x.nii"));
        assert_eq!(rel, PathBuf::from("../../sub-01/anat/x.nii"));
    }

    #[test]
    fn test_entity_iri() {
        let iri = entity_iri(Path::new("/out/derivatives/qsmxt"), "/out/derivatives/qsmxt/sub-01/anat/sub-01_Chimap.nii");
        assert_eq!(iri, "bids::sub-01/anat/sub-01_Chimap.nii");
    }

    #[test]
    fn test_activity_iri() {
        assert_eq!(activity_iri("reference", &key()), "bids::prov/#reference-sub-01");
    }

    #[test]
    fn test_start_end_from_duration() {
        let (start, end) = start_end("2026-07-30T10:00:05+00:00", 5.0);
        assert_eq!(end, "2026-07-30T10:00:05+00:00");
        assert!(start.starts_with("2026-07-30T10:00:00"));
    }

    #[test]
    fn test_start_end_unparseable_falls_back() {
        let (start, end) = start_end("not-a-date", 5.0);
        assert_eq!(start, "not-a-date");
        assert_eq!(end, "not-a-date");
    }

    #[test]
    fn test_write_provenance_end_to_end() {
        use crate::bids::discovery::{EchoFiles, QsmRun};

        let dir = tempfile::tempdir().unwrap();
        let deriv = dir.path();
        let output = DerivativeOutputs::new(deriv);
        let k = key();

        // Simulate the pipeline: a raw phase input, a reference step producing
        // the final Chimap, whose provenance.json lives in the workflow dir.
        let raw_phase = deriv.join("../sub-01/anat/sub-01_echo-1_part-phase_MEGRE.nii");
        let qsm = output.qsm_path(&k);
        std::fs::create_dir_all(qsm.parent().unwrap()).unwrap();
        std::fs::write(&qsm, b"nii").unwrap();

        let prov_path = output.provenance_path(&k, "reference");
        std::fs::create_dir_all(prov_path.parent().unwrap()).unwrap();
        let prov = serde_json::json!({
            "step": "reference",
            "algorithm": "mean",
            "parameters": {"reference": "mean"},
            "inputs": [raw_phase.display().to_string()],
            "outputs": [qsm.display().to_string()],
            "duration_secs": 2.0,
            "peak_memory_bytes": 123,
            "timestamp": "2026-07-30T10:00:05+00:00"
        });
        std::fs::write(&prov_path, serde_json::to_string(&prov).unwrap()).unwrap();

        let run = QsmRun {
            key: k.clone(),
            echoes: vec![EchoFiles {
                echo_number: 1,
                phase_nifti: raw_phase.clone(),
                phase_json: raw_phase.with_extension("json"),
                magnitude_nifti: None,
                magnitude_json: None,
            }],
            magnetic_field_strength: 3.0,
            echo_times: vec![0.02],
            b0_dir: (0.0, 0.0, 1.0),
            dims: (4, 4, 4),
            has_magnitude: false,
        };

        write_provenance(deriv, &[run], &output, "qsmxt run /data/bids --qsm-algorithm tkd").unwrap();

        // base
        let base: Value = serde_json::from_str(
            &std::fs::read_to_string(deriv.join("prov/prov-qsmxt_base.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(base["BIDSProvVersion"], "0.0.1");
        assert_eq!(base["@context"], CONTEXT_URL);

        // activity keyed by IRI: Command is the real top-level argv, Label names
        // the stage, and Parameters carries the stage's settings (algorithm).
        let act: Value = serde_json::from_str(
            &std::fs::read_to_string(deriv.join("prov/prov-qsmxt_act.json")).unwrap(),
        )
        .unwrap();
        let a = &act["bids::prov/#reference-sub-01"];
        assert_eq!(a["Command"], "qsmxt run /data/bids --qsm-algorithm tkd");
        assert_eq!(a["Label"], "reference");
        assert_eq!(a["Parameters"]["algorithm"], "mean");
        assert_eq!(a["AssociatedWith"], format!("bids::prov/#qsmxt-{}", env!("CARGO_PKG_VERSION")));
        assert!(a["Used"].as_array().unwrap().iter().any(|v| v == "bids::prov/#environment-runtime"));

        // entities: the output Chimap generated by the reference activity, and
        // the raw phase input as a leaf entity (no GeneratedBy).
        let ent: Value = serde_json::from_str(
            &std::fs::read_to_string(deriv.join("prov/prov-qsmxt_ent.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            ent["bids::sub-01/anat/sub-01_Chimap.nii"]["GeneratedBy"],
            "bids::prov/#reference-sub-01"
        );
        assert!(ent["bids::../sub-01/anat/sub-01_echo-1_part-phase_MEGRE.nii"]["GeneratedBy"].is_null());

        // sidecar next to the final Chimap
        let sidecar: Value = serde_json::from_str(
            &std::fs::read_to_string(deriv.join("sub-01/anat/sub-01_Chimap.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(sidecar["GeneratedBy"], "bids::prov/#reference-sub-01");
        assert_eq!(sidecar["MagneticFieldStrength"], 3.0);
        assert_eq!(sidecar["EchoTime"], 0.02);
        assert_eq!(sidecar["SkullStripped"], true);
    }
}
