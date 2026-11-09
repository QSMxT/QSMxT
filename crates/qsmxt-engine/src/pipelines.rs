//! Pipelines to start from: QSMxT's defaults and the original toolboxes' pipelines
//! (`qsm_core::pipeline::reference_pipelines`), in QSMxT's settings.
//!
//! A front end lists [`pipelines`]; choosing one ([`choose`]) sets the stages it covers to it, and
//! [`changes`] says beforehand what that changes. [`status`] says which pipeline a config follows
//! and where it departs from it. Every front end gets the same answers from here: the TUI's
//! Pipeline tab, qsmbly, and `qsmxt run --pipeline`.
//!
//! - **A toolbox pipeline is QSM.rs's**: its starting choices, then every original authors'
//!   setting (`reference` rule) those choices trigger, followed through ([`target`]). QSMxT adds
//!   no settings of its own to a pipeline.
//! - **The stages it covers start from QSMxT's defaults** ([`STAGES`]): every setting of a covered
//!   stage goes back to its default before the pipeline's settings apply, so a value changed
//!   earlier (MEDI's λ) is not kept under the toolbox's name. A setting the pipeline sets outside
//!   those stages (the χ-separation toolbox's Tukey window) is set on its own. Everything else
//!   stays as it is.
//! - **Reasoned recommendations are not part of a pipeline.** A pipeline is what the authors ran;
//!   a recommendation its choices trigger (PDF's measured iteration count) is still offered by
//!   [`crate::recommendations`] as usual.
//! - **Nothing changes unasked.** [`changes`] lists every change before [`choose`] makes them.
//! - **The chosen pipeline is remembered** in [`PipelineConfig::reference_pipeline`], a label
//!   that changes nothing a run computes: the status of a config that departs from it names it
//!   ("MEDI toolbox, 1 departure") as long as its choices still hold. Without a label, a config
//!   that matches a pipeline exactly follows it; any other is custom.
//! - **The command line** of a config that follows a pipeline is `--pipeline <id>` plus the flags
//!   it departs by ([`crate::command`]).

use serde::Serialize;
use serde_json::Value;

use qsm_core::pipeline::reference_pipelines::{ReferencePipeline, REFERENCE_PIPELINES};

use crate::config::PipelineConfig;
use crate::recommendations::{self as rec, Facts, Strength};
use crate::schema::{self, Section, ViewFacts};

/// The id of QSMxT's own defaults, listed first.
pub const QSMXT_DEFAULTS: &str = "qsmxt-defaults";

/// The stages a pipeline can cover: those whose settings a pipeline resets to the defaults
/// before its own apply.
pub const STAGES: [Section; 6] =
    [Section::Masking, Section::FieldMap, Section::Background, Section::Inversion, Section::Reference, Section::Separation];

/// A pipeline a front end lists.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pipeline {
    pub id: &'static str,
    /// "MEDI toolbox".
    pub name: &'static str,
    /// The toolbox and version the pipeline is taken from ("MEDI toolbox 2024.11.26"); none for
    /// QSMxT's defaults.
    pub toolbox: Option<&'static str>,
    /// Where the pipeline is written down.
    pub source: Option<&'static str>,
    /// Steps of the toolbox's pipeline QSMxT does not reproduce.
    pub not_reproduced: Vec<&'static str>,
}

impl Pipeline {
    fn defaults() -> Self {
        Pipeline { id: QSMXT_DEFAULTS, name: "QSMxT defaults", toolbox: None, source: None, not_reproduced: vec![] }
    }

    fn of(p: &'static ReferencePipeline) -> Self {
        Pipeline { id: p.id, name: p.name, toolbox: Some(p.toolbox), source: Some(p.source), not_reproduced: p.not_reproduced.to_vec() }
    }

    /// Whether this is one of the original toolboxes' pipelines (not QSMxT's defaults).
    pub fn is_toolbox(&self) -> bool {
        self.id != QSMXT_DEFAULTS
    }
}

/// Every pipeline, QSMxT's defaults first, then the toolboxes' in QSM.rs's order.
pub fn pipelines() -> Vec<Pipeline> {
    std::iter::once(Pipeline::defaults()).chain(REFERENCE_PIPELINES.iter().map(Pipeline::of)).collect()
}

/// The pipeline with this id.
pub fn pipeline(id: &str) -> Option<Pipeline> {
    pipelines().into_iter().find(|p| p.id == id)
}

/// The ids of every pipeline, for a CLI's choices.
pub fn ids() -> Vec<&'static str> {
    pipelines().iter().map(|p| p.id).collect()
}

/// One setting a pipeline sets differently from a config.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Change {
    /// The setting's config path.
    pub path: &'static str,
    /// Its display name ("Background removal").
    pub label: &'static str,
    /// The sub-tab it is on.
    pub section: Section,
    /// The config's value, and how it reads ("V-SHARP").
    pub from: Value,
    pub from_text: String,
    /// The pipeline's value, and how it reads ("PDF").
    pub to: Value,
    pub to_text: String,
}

/// A pipeline as it applies to a config: its settings, and the settings the config must reset.
struct Applied {
    config: PipelineConfig,
    /// The stages it covers.
    stages: Vec<Section>,
    /// Settings it sets outside those stages.
    extra: Vec<&'static str>,
}

fn unknown(id: &str) -> String {
    format!("no pipeline {id}: choose one of {}", ids().join(", "))
}

/// The config path and section of a rule setting.
fn section_of(path: &str) -> Option<(&'static str, Section)> {
    schema::setting(path).map(|s| (s.path, s.section))
}

/// The value at `path` in a serialised config (`null` for an unset optional value).
fn json_get(v: &Value, path: &str) -> Value {
    path.split('.').try_fold(v, |v, k| v.get(k)).cloned().unwrap_or(Value::Null)
}

/// `config` with every setting of `stages` back to its default, in one pass over the serialised
/// config ([`schema::set`] serialises the whole config for each setting).
fn reset(config: &mut PipelineConfig, stages: &[Section]) -> Result<(), String> {
    let d = serde_json::to_value(PipelineConfig::default()).map_err(|e| e.to_string())?;
    let mut v = serde_json::to_value(&*config).map_err(|e| e.to_string())?;
    for s in schema::settings().iter().filter(|s| stages.contains(&s.section)) {
        let (parent, key) = s.path.rsplit_once('.').unwrap_or(("", s.path));
        let Some(map) = parent.split('.').filter(|k| !k.is_empty())
            .try_fold(&mut v, |v, k| v.get_mut(k)).and_then(Value::as_object_mut) else { continue };
        match json_get(&d, s.path) {
            // An unset optional value is left out of the serialised config.
            Value::Null => { map.remove(key); }
            x => { map.insert(key.to_string(), x); }
        }
    }
    *config = serde_json::from_value(v).map_err(|e| e.to_string())?;
    Ok(())
}

/// Set a pipeline's choices and follow the reference rules they trigger: each rule whose trigger
/// the pipeline set, on a setting it has not set yet, until nothing changes. Returns the config
/// paths it set, in order.
fn follow(config: &mut PipelineConfig, p: &ReferencePipeline, facts: &Facts) -> Result<Vec<&'static str>, String> {
    let mut set: Vec<&'static str> = Vec::new();
    for (core_path, value) in p.choose {
        rec::set(config, core_path, &rec::core_json(value))?;
        let path = rec::qsmxt_path(core_path).ok_or_else(|| format!("QSMxT has no setting for {core_path}"))?;
        set.push(path);
        // A χ-separation method is chosen with χ-separation on.
        if path == "separation.algorithm" && !config.pipeline.do_chi_separation {
            config.pipeline.do_chi_separation = true;
            set.push("pipeline.do_chi_separation");
        }
    }
    loop {
        let view = rec::reference_values(config, facts);
        let next: Vec<_> = view.iter()
            .filter(|r| r.strength == Strength::Reference && set.contains(&r.trigger.setting.as_str()))
            .filter(|r| !set.contains(&r.setting.as_str()))
            .collect();
        let Some(first) = next.first() else { break };
        // One setting at a time, the first rule on it in QSM.rs's order: what it sets can change
        // which rules apply next.
        let path = section_of(&first.setting).map(|(p, _)| p).ok_or_else(|| format!("no setting {}", first.setting))?;
        if first.departs {
            rec::apply(config, &first.rule, None)?;
        }
        set.push(path);
    }
    Ok(set)
}

/// What choosing `id` makes of `config`, on a scan with `facts`.
fn applied(config: &PipelineConfig, id: &str, facts: &ViewFacts) -> Result<Applied, String> {
    let facts = Facts::of_view(facts);
    if id == QSMXT_DEFAULTS {
        let mut c = config.clone();
        reset(&mut c, &STAGES)?;
        return Ok(Applied { config: c, stages: STAGES.to_vec(), extra: vec![] });
    }
    let p = REFERENCE_PIPELINES.iter().find(|p| p.id == id).ok_or_else(|| unknown(id))?;
    // Which stages it covers: those of the settings it sets, found from a config with every
    // stage at its default, so the user's other settings do not decide its extent.
    let mut probe = config.clone();
    reset(&mut probe, &STAGES)?;
    let set = follow(&mut probe, p, &facts)?;
    let mut stages: Vec<Section> = Vec::new();
    let mut extra: Vec<&'static str> = Vec::new();
    for path in set {
        match section_of(path) {
            Some((_, s)) if STAGES.contains(&s) => if !stages.contains(&s) { stages.push(s) },
            Some((p, _)) if !extra.contains(&p) => extra.push(p),
            _ => {}
        }
    }
    let mut c = config.clone();
    reset(&mut c, &stages)?;
    follow(&mut c, p, &facts)?;
    Ok(Applied { config: c, stages, extra })
}

/// The stages choosing `id` resets to the defaults ([`STAGES`]), and the settings outside them
/// it sets: what a front end resets before it sets the [`changes`].
pub fn covers(config: &PipelineConfig, id: &str, facts: &ViewFacts) -> Result<(Vec<Section>, Vec<&'static str>), String> {
    let a = applied(config, id, facts)?;
    Ok((a.stages, a.extra))
}

/// `config` with pipeline `id` chosen, on a scan with `facts`: what [`choose`] sets.
pub fn target(config: &PipelineConfig, id: &str, facts: &ViewFacts) -> Result<PipelineConfig, String> {
    let mut c = applied(config, id, facts)?.config;
    c.reference_pipeline = (id != QSMXT_DEFAULTS).then(|| id.to_string());
    Ok(c)
}

/// How a setting's value reads ("PDF", "BET", "1000").
pub fn value_text(path: &str, value: &Value) -> String {
    if path == "masking.sections" {
        let names: Vec<String> = value.as_array().into_iter().flatten().filter_map(|s| {
            let op: crate::masking::MaskOp = serde_json::from_value(s.get("generator")?.clone()).ok()?;
            schema::mask::step(schema::mask::step_kind(&op)).map(|st| st.name.to_string())
        }).collect();
        if !names.is_empty() {
            return names.join(" + ");
        }
    }
    if let Some(s) = schema::setting(path) {
        if let Some(o) = s.value_type.options().into_iter().find(|o| o.value == *value) {
            return o.name.clone();
        }
    }
    match value {
        Value::Bool(true) => "on".into(),
        Value::Bool(false) => "off".into(),
        Value::Null => "auto".into(),
        Value::String(s) => s.clone(),
        Value::Array(xs) if !xs.is_empty() && xs.iter().all(|x| Some(x) == xs.first()) => value_text("", &xs[0]),
        Value::Array(xs) => xs.iter().map(|x| value_text("", x)).collect::<Vec<_>>().join(" "),
        other => other.to_string(),
    }
}

/// The settings `a` and `b` differ in, among those `applied` covers, that apply in either.
fn differences(a: &PipelineConfig, b: &PipelineConfig, applied: &Applied, facts: &ViewFacts) -> Vec<Change> {
    let shown = |c: &PipelineConfig| -> std::collections::HashSet<&'static str> {
        schema::settings_view(c, facts).into_iter().filter(|v| v.visible).map(|v| v.path).collect()
    };
    let (shown_a, shown_b) = (shown(a), shown(b));
    let (Ok(va), Ok(vb)) = (serde_json::to_value(a), serde_json::to_value(b)) else { return vec![] };
    schema::settings().iter()
        .filter(|s| applied.stages.contains(&s.section) || applied.extra.contains(&s.path))
        .filter(|s| shown_a.contains(s.path) || shown_b.contains(s.path))
        .filter_map(|s| {
            let (from, to) = (json_get(&va, s.path), json_get(&vb, s.path));
            (from != to).then(|| Change {
                path: s.path, label: s.label, section: s.section,
                from_text: value_text(s.path, &from), to_text: value_text(s.path, &to), from, to,
            })
        })
        .collect()
}

/// What choosing `id` would change in `config`, on a scan with `facts`, in settings order.
pub fn changes(config: &PipelineConfig, id: &str, facts: &ViewFacts) -> Result<Vec<Change>, String> {
    let a = applied(config, id, facts)?;
    Ok(differences(config, &a.config, &a, facts))
}

/// Choose pipeline `id`: set every setting [`changes`] lists, and remember the choice.
pub fn choose(config: &mut PipelineConfig, id: &str, facts: &ViewFacts) -> Result<(), String> {
    *config = target(config, id, facts)?;
    Ok(())
}

/// Which pipeline a config follows, and where it departs from it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Status {
    /// The pipeline, or none: custom.
    pub pipeline: Option<Pipeline>,
    /// Where the config departs from it ([`Change::from`] is the config's value, [`Change::to`]
    /// the pipeline's).
    pub departures: Vec<Change>,
}

impl Status {
    /// How a front end shows it: "MEDI toolbox", "MEDI toolbox · 1 departure", "Custom".
    pub fn text(&self) -> String {
        match (&self.pipeline, self.departures.len()) {
            (None, _) => "Custom".into(),
            (Some(p), 0) => p.name.into(),
            (Some(p), 1) => format!("{} · 1 departure", p.name),
            (Some(p), n) => format!("{} · {n} departures", p.name),
        }
    }
}

/// Whether `config` still makes the choices that start toolbox pipeline `id`.
pub fn chosen(config: &PipelineConfig, id: &str) -> bool {
    let Some(p) = REFERENCE_PIPELINES.iter().find(|p| p.id == id) else { return false };
    let mut probe = config.clone();
    p.choose.iter().all(|(core_path, value)| {
        let Some(path) = rec::qsmxt_path(core_path) else { return false };
        let before = schema::get(config, path);
        rec::set(&mut probe, core_path, &rec::core_json(value)).is_ok() && schema::get(&probe, path) == before
    }) && (!p.choose.iter().any(|(s, _)| *s == "separation.algorithm") || config.pipeline.do_chi_separation)
}

/// Which pipeline `config` follows on a scan with `facts`: the one it remembers while its choices
/// hold, with its departures; else the first that it matches exactly; else none.
pub fn status(config: &PipelineConfig, facts: &ViewFacts) -> Status {
    let follows = |id: &str| -> Option<Status> {
        let a = applied(config, id, facts).ok()?;
        Some(Status { pipeline: pipeline(id), departures: differences(config, &a.config, &a, facts) })
    };
    if let Some(id) = config.reference_pipeline.as_deref().filter(|id| chosen(config, id)) {
        if let Some(s) = follows(id) {
            return s;
        }
    }
    ids().into_iter()
        .filter(|id| *id == QSMXT_DEFAULTS || chosen(config, id))
        .filter_map(follows)
        .find(|s| s.departures.is_empty())
        .unwrap_or(Status { pipeline: None, departures: vec![] })
}

/// The toolbox pipeline `config` was chosen from, for its command line: the one it remembers,
/// while its choices hold. A cheap check, unlike [`status`], so a front end can render the
/// command on every change; a config that matches a pipeline without remembering it is written
/// out in full.
pub fn followed(config: &PipelineConfig, _facts: &ViewFacts) -> Option<&'static str> {
    let id = config.reference_pipeline.as_deref().filter(|id| chosen(config, id))?;
    REFERENCE_PIPELINES.iter().find(|p| p.id == id).map(|p| p.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> ViewFacts {
        ViewFacts { num_echoes: Some(4), field_strength: Some(3.0), ..Default::default() }
    }

    #[test]
    fn the_defaults_follow_the_defaults() {
        let s = status(&PipelineConfig::default(), &facts());
        assert_eq!(s.pipeline.map(|p| p.id), Some(QSMXT_DEFAULTS));
        assert!(s.departures.is_empty());
        assert_eq!(pipelines()[0].id, QSMXT_DEFAULTS);
    }

    /// Choosing each pipeline gives a config that follows it with no departures, and choosing it
    /// again changes nothing.
    #[test]
    fn every_pipeline_is_followed_once_chosen() {
        for p in pipelines() {
            let mut c = PipelineConfig::default();
            choose(&mut c, p.id, &facts()).unwrap_or_else(|e| panic!("{}: {e}", p.id));
            let s = status(&c, &facts());
            assert_eq!(s.pipeline.as_ref().map(|x| x.id), Some(p.id), "{}: {}", p.id, s.text());
            assert!(s.departures.is_empty(), "{}: {:?}", p.id, s.departures);
            assert!(changes(&c, p.id, &facts()).unwrap().is_empty(), "{}", p.id);
        }
    }

    /// MEDI's pipeline: the toolbox's mask, field map, unwrapping and background removal, and λ
    /// back to its default.
    #[test]
    fn the_medi_toolbox_pipeline() {
        let mut c = PipelineConfig::default();
        c.inversion.medi.lambda = 500.0;
        let ch = changes(&c, "medi-toolbox", &facts()).unwrap();
        let to = |path: &str| ch.iter().find(|x| x.path == path).map(|x| x.to_text.clone());
        assert_eq!(to("inversion.algorithm").as_deref(), Some("MEDI"), "{ch:#?}");
        assert_eq!(to("bg_removal.algorithm").as_deref(), Some("PDF"));
        assert!(to("masking.sections").is_some_and(|t| t.contains("BET")), "{ch:#?}");
        assert!(to("field_mapping.b0_estimation").is_some());
        assert!(to("field_mapping.unwrapping_algorithm").is_some());
        // λ is not a MEDI setting while iLSQR is chosen, so it is not listed; choosing resets it.
        choose(&mut c, "medi-toolbox", &facts()).unwrap();
        assert_eq!(c.inversion.medi.lambda, PipelineConfig::default().inversion.medi.lambda);
        assert_eq!(c.reference_pipeline.as_deref(), Some("medi-toolbox"));
        // PDF's measured recommendation is not part of it.
        assert_eq!(c.bg_removal.pdf.n_cg, PipelineConfig::default().bg_removal.pdf.n_cg);
    }

    /// A departure keeps the pipeline's name while its choices hold; changing a choice makes it
    /// custom.
    #[test]
    fn departures_and_custom() {
        let mut c = PipelineConfig::default();
        choose(&mut c, "medi-toolbox", &facts()).unwrap();
        c.inversion.medi.lambda = 500.0;
        let s = status(&c, &facts());
        assert_eq!(s.text(), "MEDI toolbox · 1 departure");
        assert_eq!(s.departures[0].path, "inversion.medi.lambda");
        c.inversion.algorithm = crate::enums::QsmAlgorithm::Tkd;
        assert_eq!(status(&c, &facts()).text(), "Custom");
        assert_eq!(followed(&c, &facts()), None);
    }

    /// Settings outside the stages a pipeline covers stay as they are.
    #[test]
    fn other_stages_are_kept() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_swi = true;
        choose(&mut c, "sti-suite-star", &facts()).unwrap();
        assert!(c.pipeline.do_swi);
    }

    #[test]
    fn an_unknown_pipeline_names_the_choices() {
        let e = changes(&PipelineConfig::default(), "nope", &facts()).unwrap_err();
        assert!(e.contains("medi-toolbox"), "{e}");
    }
}

