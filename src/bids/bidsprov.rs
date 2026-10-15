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

/// Every step that records a `provenance.json` under a run (the names `complete_step` is called
/// with in `pipeline::runner`), in pipeline order. Order matters where a later step rewrites an
/// earlier one's file in place — `output_space` returns the published maps to the acquired grid —
/// because the file on disk is then the later step's. `load` records no step. A test keeps this
/// list in step with the runner.
pub const STEPS: &[&str] = &[
    "motion", "scale_phase", "magnitude", "mask", "mask-reliable", "segmentation", "swi",
    "t2star_r2star", "r2_r2prime", "unwrap",
    "bgremove", "bgremove-reliable", "tgv", "tgv-reliable", "qsmart", "qsmart-reliable",
    "invert", "invert-reliable", "twopass", "reference-singlepass", "reference",
    "smwi", "chi_separation", "analysis", "output_space",
];

/// Steps recorded under a multi-orientation group's key (`pipeline::multiorient`).
pub const GROUP_STEPS: &[&str] = &["cosmos", "sti"];

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
    #[serde(rename = "CodeURL", skip_serializing_if = "Option::is_none")]
    code_url: Option<String>,
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

/// Which acquisition metadata is still true of an output, and is forwarded into its sidecar.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Forward {
    /// Nothing: a mask or label image.
    Nothing,
    /// `MagneticFieldStrength` only: a map fitted across echoes, or from another acquisition.
    FieldStrength,
    /// `MagneticFieldStrength`, and `EchoTime` when the run had a single echo.
    FieldAndEcho,
}

/// How a published output is described in its sidecar.
#[derive(Clone, Copy, PartialEq, Debug)]
struct OutputMeta {
    forward: Forward,
    /// `SkullStripped`, or `None` where the key does not apply (masks and label images, per the
    /// BIDS derivatives schema).
    skull_stripped: Option<bool>,
}

const MASK: OutputMeta = OutputMeta { forward: Forward::Nothing, skull_stripped: None };

/// The sidecar description of the published outputs QSMxT knows. Which step made each file is
/// not listed here: it is read from the provenance records, so a sidecar can only ever name an
/// activity that exists.
///
/// `SkullStripped` (required for derivative anat images except `mask`) reflects whether the
/// pipeline confines the output to the brain mask:
/// - Chimap is referenced within the mask, and T2star*/R2star* are masked explicitly, so they
///   are skull-stripped; SMWI is the magnitude weighted by a χ-derived mask, so it is confined
///   the way the Chimap is;
/// - the RSS magnitude (T2starw) and SWI/minIP retain full field-of-view signal.
fn run_outputs(output: &DerivativeOutputs, key: &AcquisitionKey, n_echoes: usize) -> Vec<(PathBuf, OutputMeta)> {
    let image = |forward, ss| OutputMeta { forward, skull_stripped: Some(ss) };
    use Forward::*;
    // The MCPC-3D-S combined echoes of a per-coil run: one echo each, full field of view.
    let combined = (1..=n_echoes).flat_map(|e| [
        (output.combined_phase_path(key, e), image(FieldStrength, false)),
        (output.combined_mag_path(key, e), image(FieldStrength, false)),
    ]);
    combined.chain([
        (output.qsm_path(key), image(FieldAndEcho, true)),
        (output.singlepass_qsm_path(key), image(FieldAndEcho, true)),
        (output.mask_path(key), MASK),
        (output.qsm_support_path(key), MASK),
        (output.two_pass_mask_path(key), MASK),
        (output.magnitude_path(key), image(FieldAndEcho, false)),
        (output.swi_path(key), image(FieldAndEcho, false)),
        (output.swi_mip_path(key), image(FieldAndEcho, false)),
        (output.dseg_path(key), MASK),
        (output.smwi_path(key, "paramagnetic"), image(FieldAndEcho, true)),
        (output.smwi_path(key, "diamagnetic"), image(FieldAndEcho, true)),
        (output.smwi_mip_path(key, "paramagnetic"), image(FieldAndEcho, true)),
        (output.smwi_mip_path(key, "diamagnetic"), image(FieldAndEcho, true)),
        (output.t2star_path(key), image(FieldStrength, true)),
        (output.r2star_path(key), image(FieldStrength, true)),
        // R2 comes from the multi-echo spin echo, so the GRE's echo time says nothing about it.
        (output.r2_path(key), image(FieldStrength, true)),
        (output.r2prime_path(key), image(FieldStrength, true)),
        (output.r2prime_coverage_path(key), MASK),
        (output.motion_coverage_path(key), MASK),
        (output.chi_para_path(key), image(FieldAndEcho, true)),
        (output.chi_dia_path(key), image(FieldAndEcho, true)),
        (output.chi_sep_total_path(key), image(FieldAndEcho, true)),
        // The field map is computed inside the brain mask and is 0 outside it; its magnitude is
        // the first echo as acquired.
        (output.total_field_path(key), image(FieldStrength, true)),
        (output.total_field_magnitude_path(key), image(FieldStrength, false)),
    ]).collect()
}

/// A multi-orientation group's outputs: COSMOS / STI maps, all fitted across orientations.
fn group_meta() -> OutputMeta {
    OutputMeta { forward: Forward::FieldStrength, skull_stripped: Some(true) }
}

fn is_nifti(path: &Path) -> bool {
    let name = path.to_string_lossy();
    name.ends_with(".nii") || name.ends_with(".nii.gz")
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

/// The QSM.rs (qsm-core) version recorded everywhere: its tag, or `rev-<full commit>`.
pub const QSM_RS_VERSION: &str = env!("QSM_CORE_VERSION");

/// Software (Agent) records: qsmxt itself and its bundled reconstruction
/// library QSM.rs (the `qsm-core` crate, compiled into the binary — shipped
/// with qsmxt rather than an external dependency, but independently versioned
/// and pinned to an exact commit).
fn agent_records() -> Vec<(String, AgentRecord)> {
    let qsmxt_version = env!("CARGO_PKG_VERSION");
    let code_url = env!("QSM_CORE_CODE_URL");
    vec![
        (
            format!("bids::prov/#qsmxt-{}", qsmxt_version),
            AgentRecord { label: "qsmxt".to_string(), version: qsmxt_version.to_string(), code_url: None },
        ),
        (
            format!("bids::prov/#qsm-rs-{}", QSM_RS_VERSION),
            AgentRecord {
                label: "QSM.rs".to_string(),
                version: QSM_RS_VERSION.to_string(),
                code_url: (!code_url.is_empty()).then(|| code_url.to_string()),
            },
        ),
    ]
}

/// A multi-orientation group whose COSMOS / STI outputs are described alongside the runs.
pub struct ProvGroup {
    /// The group key its outputs and records are filed under.
    pub key: AcquisitionKey,
    pub magnetic_field_strength: f64,
}

/// Write BIDS-Prov records (`prov/`) and per-output `GeneratedBy` sidecars.
///
/// `command` is the actual top-level invocation that ran the pipeline (the real
/// `qsmxt run …` argv). BEP028 provenance is retrospective, so every Activity's
/// `Command` is this literal command — the one that genuinely produced all the
/// outputs — while `Label` names the stage and `Parameters` holds its settings.
///
/// Every published NIfTI a step recorded (anything it wrote into the key's `anat/`) gets a
/// sidecar whose `GeneratedBy` is the activity that last wrote it, and which records the qsmxt
/// and QSM.rs versions.
///
/// Best-effort: individual unreadable `provenance.json` files are skipped with a
/// warning rather than failing the whole write.
pub fn write_provenance(
    derivatives_dir: &Path,
    runs: &[QsmRun],
    groups: &[ProvGroup],
    output: &DerivativeOutputs,
    command: &str,
) -> crate::Result<()> {
    let agents = agent_records();
    // qsmxt (the invoked tool) is the agent activities are AssociatedWith.
    let agent_iri = agents[0].0.clone();
    let (env_iri, environment) = environment_record();

    let mut activities: Map<String, Value> = Map::new();
    let mut entities: Map<String, Value> = Map::new();

    // Pass 1: read every step's provenance.json; build a path -> activity index so that a later
    // step's input can be linked to the activity that produced it.
    struct ParsedStep {
        step: &'static str,
        activity: String,
        prov: StepProvenance,
        /// Inputs that were the step's own output before it rewrote them in place, mapped to
        /// the activity that had made them.
        used_from: BTreeMap<String, String>,
    }
    /// Who a published file belongs to, and how its sidecar describes it.
    struct Owner<'a> {
        /// The directories its published files are in (`anat/`, `fmap/`).
        dirs: Vec<PathBuf>,
        known: Vec<(PathBuf, OutputMeta)>,
        default_meta: OutputMeta,
        field_strength: f64,
        echo_times: &'a [f64],
    }
    let mut parsed: Vec<ParsedStep> = Vec::new();
    let mut produced_by: BTreeMap<String, String> = BTreeMap::new();
    let mut owners: Vec<(AcquisitionKey, Owner, &'static [&'static str])> = Vec::new();
    for run in runs {
        owners.push((run.key.clone(), Owner {
            dirs: output.published_dirs(&run.key).to_vec(),
            known: run_outputs(&output.published(&run.key), &run.key, run.echo_times.len()),
            default_meta: MASK,
            field_strength: run.magnetic_field_strength,
            echo_times: &run.echo_times,
        }, STEPS));
    }
    for group in groups {
        owners.push((group.key.clone(), Owner {
            dirs: vec![output.anat_dir(&group.key)],
            known: Vec::new(),
            default_meta: group_meta(),
            field_strength: group.magnetic_field_strength,
            echo_times: &[],
        }, GROUP_STEPS));
    }

    // Which owner's step last wrote each file — runs of one session share `anat/`, so the
    // directory alone does not say whose a file is.
    let mut written_for: BTreeMap<String, usize> = BTreeMap::new();
    for (owner_idx, (key, _, steps)) in owners.iter().enumerate() {
        for &step in steps.iter() {
            let path = output.provenance_path(key, step);
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
            let activity = activity_iri(step, key);
            // A step that rewrites files in place (output_space) used the earlier version of each:
            // the working-grid copy it read is the file an earlier activity made.
            let mut used_from = BTreeMap::new();
            for inp in &prov.inputs {
                if produced_by.contains_key(inp) {
                    continue;
                }
                let name = Path::new(inp).file_name();
                if let Some(earlier) = prov.outputs.iter()
                    .find(|o| Path::new(o).file_name() == name)
                    .and_then(|o| produced_by.get(o))
                {
                    used_from.insert(inp.clone(), earlier.clone());
                }
            }
            for out in &prov.outputs {
                produced_by.insert(out.clone(), activity.clone());
                written_for.insert(out.clone(), owner_idx);
            }
            parsed.push(ParsedStep { step, activity, prov, used_from });
        }
    }

    // Output entities: generated by the activity that last wrote them.
    for ps in &parsed {
        for out in &ps.prov.outputs {
            let iri = entity_iri(derivatives_dir, out);
            let rec = EntityRecord {
                label: file_label(out),
                at_location: Some(at_location(derivatives_dir, out)),
                generated_by: produced_by.get(out).cloned(),
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
                    generated_by: produced_by.get(inp).or_else(|| ps.used_from.get(inp)).cloned(),
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

    // A published output no step claims cannot be given a GeneratedBy that resolves: say so
    // rather than name an activity that is not in `prov/`.
    for (_, owner, _) in &owners {
        for (path, _) in &owner.known {
            if path.exists() && !written_for.contains_key(&path.display().to_string()) {
                warn!(
                    "BIDS-Prov: no step recorded writing {}, so its sidecar names no GeneratedBy \
                     (was its workflow directory removed?)",
                    path.display()
                );
            }
        }
    }

    // Per-output sidecars: every published NIfTI that some step recorded writing.
    for (file, &owner_idx) in &written_for {
        let owner = &owners[owner_idx].1;
        let activity = &produced_by[file];
        let path = PathBuf::from(file);
        if !path.parent().is_some_and(|p| owner.dirs.iter().any(|d| d == p)) || !is_nifti(&path) || !path.exists() {
            continue;
        }
        let meta = owner.known.iter().find(|(p, _)| *p == path).map(|(_, m)| *m)
            .unwrap_or(owner.default_meta);
        let Some(sidecar) = entities::sidecar_path(&path) else {
            continue;
        };
        // Start from what is already there: the pipeline writes its own fields into this
        // sidecar (the QSM reference, for one), and clobbering them here would lose them.
        let mut obj: Map<String, Value> = std::fs::read_to_string(&sidecar).ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        // Required for derivative anat images (except mask).
        if let Some(ss) = meta.skull_stripped {
            obj.insert("SkullStripped".to_string(), Value::Bool(ss));
        }
        if meta.forward != Forward::Nothing {
            obj.insert("MagneticFieldStrength".to_string(), serde_json::json!(owner.field_strength));
        }
        // EchoTime remains valid only when a single echo was used.
        if meta.forward == Forward::FieldAndEcho && owner.echo_times.len() == 1 {
            obj.insert("EchoTime".to_string(), serde_json::json!(owner.echo_times[0]));
        }
        obj.insert("GeneratedBy".to_string(), Value::String(activity.clone()));
        obj.insert("QSMxTVersion".to_string(), Value::String(env!("CARGO_PKG_VERSION").to_string()));
        obj.insert("QSMrsVersion".to_string(), Value::String(QSM_RS_VERSION.to_string()));
        write_json(&sidecar, &Value::Object(obj))?;
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

    /// `STEPS` has to name every step the runner records, or that step's provenance is never
    /// read and the files it wrote get no `GeneratedBy` (#278). Read off the runner's source:
    /// literal step names, each two-pass step in both passes, and the few built at run time.
    #[test]
    fn steps_cover_every_recorded_step() {
        let src = include_str!("../pipeline/runner.rs");
        let src = &src[..src.find("#[cfg(test)]").unwrap()];
        let literal_after = |pat: &str| -> Vec<String> {
            src.match_indices(pat)
                .filter_map(|(i, _)| {
                    let rest = src[i + pat.len()..].trim_start();
                    let rest = rest.strip_prefix('"')?;
                    Some(rest[..rest.find('"')?].to_string())
                })
                .collect()
        };
        let mut names = literal_after("complete_step(");
        // The steps the engine computes, which the runner records under their engine names.
        use qsmxt_engine::plan::{Pass, Stage};
        for pass in [Pass::Main, Pass::Reliable] {
            for stage in [Stage::Mask(pass), Stage::BackgroundRemoval(pass), Stage::Inversion(pass),
                          Stage::Tgv(pass), Stage::Qsmart(pass)] {
                names.push(stage.name());
            }
        }
        for stage in [Stage::ScalePhase, Stage::Magnitude, Stage::FieldMap, Stage::TwoPassCombination,
                      Stage::Reference, Stage::SinglePassReference, Stage::Swi, Stage::Smwi,
                      Stage::Relaxometry, Stage::R2, Stage::ChiSeparation, Stage::Motion,
                      Stage::Segmentation, Stage::Analysis, Stage::OutputSpace] {
            names.push(stage.name());
        }
        for name in &names {
            assert!(STEPS.contains(&name.as_str()), "step {name:?} is recorded but missing from STEPS");
        }
        // Every call site whose name is not a literal is one of the cases handled above (the
        // engine's steps, recorded by the store). A new one has to be added here (and to STEPS).
        let dynamic = src.match_indices("complete_step(")
            .filter(|(i, pat)| {
                let rest = src[i + pat.len()..].trim_start();
                !rest.starts_with('"') && !rest.starts_with("&mut")
            })
            .count();
        assert_eq!(dynamic, 0, "a step name is built somewhere new: cover it in STEPS and in this test");
        for step in STEPS {
            assert!(names.iter().any(|n| n == step), "STEPS lists {step:?}, which no step records");
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
            coils: None,
            echoes: vec![EchoFiles {
                echo_number: 1,
                phase_nifti: raw_phase.clone(),
                phase_json: raw_phase.with_extension("json"),
                magnitude_nifti: None,
                magnitude_json: None,
                imaginary_nifti: None,
            }],
            magnetic_field_strength: 3.0,
            echo_times: vec![0.02],
            b0_dir: None,
            dims: (4, 4, 4),
            has_magnitude: false,
            mese: None,
        };

        write_provenance(deriv, &[run], &[], &output, "qsmxt run /data/bids --qsm-algorithm tkd").unwrap();

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
