//! Dismissing and applying findings.

use super::*;

// ─── Dismissing ───

/// Dismiss the finding of `rule` on `config` for the setting's current value: it is no longer
/// offered while the setting keeps that value. `Err` when the rule is a reference-pipeline note
/// ([`reference_rule_refusal`]), has no finding on `config`, or is `required` (the method cannot
/// run otherwise).
pub fn dismiss(config: &mut PipelineConfig, rule: &str, facts: &Facts) -> Result<(), String> {
    if is_reference_rule(rule) {
        return Err(reference_rule_refusal(rule));
    }
    let r = recommendations(config, facts).into_iter().find(|r| r.rule == rule)
        .ok_or_else(|| format!("no recommendation {rule} applies to this configuration"))?;
    if r.strength == Strength::Required {
        return Err(format!("{rule} is required: the method cannot run otherwise"));
    }
    let chosen = chosen_text(&r.current);
    config.dismissed_recommendations.retain(|d| d.rule != rule);
    config.dismissed_recommendations.push(Dismissal { rule: rule.to_string(), chosen });
    Ok(())
}

/// Why a reference-pipeline rule cannot be dismissed, as front ends say it.
pub fn reference_rule_refusal(rule: &str) -> String {
    format!("{rule} is a reference-pipeline note, not a recommendation; there is nothing to dismiss")
}

/// Take back the dismissal of `rule`.
pub fn undismiss(config: &mut PipelineConfig, rule: &str) {
    config.dismissed_recommendations.retain(|d| d.rule != rule);
}

/// `config`'s dismissals less those that no longer match a finding (the setting has changed, or
/// the rule no longer applies): what a front end keeps after a change, so going back to a value
/// brings its finding back. Scan-dependent findings are kept while the scan is unknown.
pub fn prune_dismissals(config: &mut PipelineConfig, facts: &Facts) {
    let all = reference_values(config, facts);
    config.dismissed_recommendations.retain(|d| {
        let rule = core::RULES.iter().find(|r| r.id == d.rule);
        let reference = rule.is_some_and(|r| r.strength == core::Strength::Reference);
        let silent_until_scan = rule.is_some_and(|r| r.needs_scan()) && facts.echo_count.is_none();
        !reference && (silent_until_scan || all.iter().any(|r| r.rule == d.rule && r.departs && r.dismissed))
    });
}

// ─── Applying ───

/// Whether a rule may change the setting at `path` (QSMxT's) in `config`, on a scan with `facts`:
/// only while the setting applies there ([`crate::schema::visible_settings`]). A hidden setting
/// does nothing for the run, and a value changed there would come back unasked when the setting
/// applies again (decision 194).
///
/// The mask recipe and the magnitude correction are excepted, as the TUI has always excepted them.
/// Every run that makes a mask makes it from them, so they always apply; and a mask rule changes
/// one part of the recipe (its generator, input, BET f or erosion) that no single row shows, so
/// whether a row is in view says nothing about whether the rule may change it.
pub fn rule_may_change(config: &PipelineConfig, path: &str, facts: &Facts) -> bool {
    if matches!(path, "masking.sections" | "masking.inhomogeneity_correction") {
        return true;
    }
    // What the scan facts say about which settings apply; the rest (coils, magnitude) unknown,
    // which shows every setting the data could need.
    let view = crate::schema::ViewFacts {
        num_echoes: facts.echo_count,
        field_strength: facts.field_strength,
        voxel_size: facts.voxel_size.map(|(x, y, z)| [x, y, z]),
        dims: facts.dims.map(|(x, y, z)| [x, y, z]),
        manufacturer: facts.manufacturer.clone(),
        measured_r2prime: facts.measured_r2prime,
        ..Default::default()
    };
    crate::schema::visible_settings(config, &view).into_iter().find(|(p, _)| *p == path).is_none_or(|(_, shown)| shown)
}

/// Change `config` so the finding of `rule` is resolved: to the rule's recommended value, or, with
/// `pick`, to another value it allows (one of [`Allowed::OneOf`]'s, or a number in its
/// [`Allowed::Range`]). Any dismissal of the rule is dropped. A setting that does not apply on a
/// scan with `facts` is refused and left as it is ([`rule_may_change`]); a required change (an
/// output another setting needs) is always made.
pub fn apply(config: &mut PipelineConfig, rule: &str, pick: Option<&Value>, facts: &Facts) -> Result<(), String> {
    if let Some(r) = core::RULES.iter().find(|r| r.id == rule) {
        let path = qsmxt_path(r.setting).unwrap_or(r.setting);
        if !rule_may_change(config, path, facts) {
            return Err(format!("{} does not apply in this configuration, so {rule} leaves it as it is", label_of(r.setting)));
        }
    }
    apply_rule(config, rule, pick)
}

/// [`apply`] whether or not the setting applies: for a pipeline, whose choices and the authors'
/// settings they bring apply whole ([`crate::pipelines`]), and for findings already checked.
pub(crate) fn apply_rule(config: &mut PipelineConfig, rule: &str, pick: Option<&Value>) -> Result<(), String> {
    // A required change (an output another setting needs): turn the output on.
    if crate::normalise::REQUIRED_RULES.contains(&rule) {
        if let Some(a) = config.normalised().1.into_iter().find(|a| a.rule == rule) {
            crate::schema::set(config, a.setting, Value::Bool(true))?;
        }
        return Ok(());
    }
    let r = core::RULES.iter().find(|r| r.id == rule).ok_or_else(|| format!("no rule {rule}"))?;
    let value = match (pick, r.expect) {
        (None, _) => core_json(&r.default),
        (Some(p), Test::OneOf { values }) => values.iter().map(core_json).find(|v| v == p)
            .ok_or_else(|| format!("{rule} does not allow {p}"))?,
        (Some(p), Test::Range { min, max }) => {
            let x = p.as_f64().ok_or_else(|| format!("{rule} takes a number, not {p}"))?;
            if min.is_some_and(|m| x < m) || max.is_some_and(|m| x > m) {
                return Err(format!("{x} is outside what {rule} allows"));
            }
            number(x)
        }
        (Some(p), Test::NoneOf { values }) => {
            if values.iter().map(core_json).any(|v| v == *p) {
                return Err(format!("{rule} discourages {p}"));
            }
            p.clone()
        }
    };
    set(config, r.setting, &value)?;
    undismiss(config, rule);
    Ok(())
}

/// Set `rule`'s setting back to `value`, one of its current values (QSM.rs's terms, as
/// [`Recommendation::current`] has them), whatever the rule allows: for a host keeping a setting
/// that a value applied to another part of it changed (a mask generator brings its preset's
/// input and erosion).
pub fn restore(config: &mut PipelineConfig, rule: &str, value: &Value) -> Result<(), String> {
    let r = core::RULES.iter().find(|r| r.id == rule).ok_or_else(|| format!("no rule {rule}"))?;
    set(config, r.setting, value)
}

/// Set the QSM.rs setting `core_path` to `value` (QSM.rs's terms) in `config`.
pub(crate) fn set(config: &mut PipelineConfig, core_path: &str, value: &Value) -> Result<(), String> {
    let (path, kind) = setting_of(core_path).ok_or_else(|| format!("QSMxT has no setting for {core_path}"))?;
    let id = value.as_str().unwrap_or_default();
    match kind {
        Mapping::Same => {
            crate::schema::set(config, path, value.clone())?;
        }
        Mapping::MaskGenerator => {
            // The preset whose sections all start with that generator (`bet` → BET on the
            // magnitude, then erode).
            let (_, recipe) = crate::masking::mask_presets().into_iter()
                .find(|(_, r)| !r.sections.is_empty() && r.sections.iter().all(|s| op_id(&s.generator) == id))
                .ok_or_else(|| format!("no mask preset starts with {id}"))?;
            config.masking.sections = recipe.sections;
            config.masking.combine = recipe.combine;
            config.masking.refinements = recipe.refinements;
        }
        Mapping::HdBetInference => {
            let patch = match id {
                "full-volume" => None,
                "sliding-window" => Some(MaskOp::hd_bet_default_patch()),
                "low-memory" => Some(crate::masking::hd_bet_low_memory_patch()),
                other => return Err(format!("HD-BET inference {other} is not one QSMxT can set")),
            };
            for s in &mut config.masking.sections {
                for op in std::iter::once(&mut s.generator).chain(s.refinements.iter_mut()) {
                    if let MaskOp::HdBet { patch: p, .. } = op {
                        *p = patch;
                    }
                }
            }
        }
        Mapping::LaplacianSolver => {
            let fm = &mut config.field_mapping;
            fm.laplacian_solver = match id {
                "dct" => LaplacianSolver::Dct,
                "mirror" => LaplacianSolver::Mirror,
                "mgpcg" => LaplacianSolver::Mgpcg,
                "fft" => LaplacianSolver::Fft,
                "fft-12" => {
                    let qsm_core::unwrap::LaplacianSolver::Fft { pad } = qsm_core::unwrap::LaplacianSolver::FFT_DEFAULT_PAD
                    else { unreachable!("FFT_DEFAULT_PAD is an FFT solver") };
                    fm.laplacian_fft_pad = pad;
                    LaplacianSolver::Fft
                }
                "fft-64" => {
                    let qsm_core::unwrap::LaplacianSolver::Fft { pad } = qsm_core::unwrap::LaplacianSolver::FFT_UKB_PAD
                    else { unreachable!("FFT_UKB_PAD is an FFT solver") };
                    fm.laplacian_fft_pad = pad;
                    LaplacianSolver::Fft
                }
                other => return Err(format!("unknown Laplacian solver {other}")),
            };
        }
        Mapping::VsharpGrid => {
            let v = &mut config.bg_removal.vsharp;
            v.grid = match id {
                "native" => VsharpGrid::Native,
                "isotropic-finest" => VsharpGrid::Finest,
                "isotropic-1mm" => {
                    v.grid_spacing = 1.0;
                    VsharpGrid::Isotropic
                }
                "isotropic" => VsharpGrid::Isotropic,
                other => return Err(format!("unknown V-SHARP grid {other}")),
            };
        }
        Mapping::SynthSegInference => {
            config.segmentation.patch = crate::config::synthseg_patch(match id {
                "full-volume" => qsm_core::segment::SynthSegInference::FullVolume,
                "low-memory" => qsm_core::segment::SynthSegInference::LOW_MEMORY,
                other => return Err(format!("SynthSeg inference {other} needs a window size")),
            });
        }
        Mapping::SynthSegPatch => {
            config.segmentation.patch = Some(value.as_u64().ok_or_else(|| format!("{value} is not a window size"))? as usize);
        }
        Mapping::MaskInput => {
            let input = crate::masking::parse_masking_input(id).ok_or_else(|| format!("unknown mask input {id}"))?;
            for s in &mut config.masking.sections {
                s.input = input;
            }
        }
        Mapping::Inhomogeneity => {
            let m = &mut config.masking;
            match id {
                "none" => m.inhomogeneity_correction = false,
                "makehomogeneous" => {
                    m.inhomogeneity_correction = true;
                    m.inhomogeneity_method = crate::enums::InhomogeneityMethod::Makehomogeneous;
                }
                "n4" => {
                    m.inhomogeneity_correction = true;
                    m.inhomogeneity_method = crate::enums::InhomogeneityMethod::N4;
                }
                other => return Err(format!("unknown inhomogeneity correction {other}")),
            }
        }
        Mapping::BetFraction => {
            let f = value.as_f64().ok_or_else(|| format!("{value} is not a fractional intensity"))?;
            for s in &mut config.masking.sections {
                for op in std::iter::once(&mut s.generator).chain(s.refinements.iter_mut()) {
                    if let MaskOp::Bet { fractional_intensity, .. } = op {
                        *fractional_intensity = f;
                    }
                }
            }
        }
        Mapping::Erosion => {
            let n = value.as_u64().or_else(|| value.as_f64().map(|v| v as u64))
                .ok_or_else(|| format!("{value} is not a number of voxels"))? as usize;
            for s in &mut config.masking.sections {
                s.refinements.retain(|op| !matches!(op, MaskOp::Erode { .. }));
                if n > 0 {
                    s.refinements.push(MaskOp::Erode { iterations: n });
                }
            }
        }
        Mapping::MinIntensity => {
            let v = value.as_f64().filter(|v| v.is_finite() && *v >= 0.0)
                .ok_or_else(|| format!("{value} is not an intensity"))?;
            for s in &mut config.masking.sections {
                s.refinements.retain(|op| !matches!(op, MaskOp::MinIntensity { .. }));
                if v > 0.0 {
                    s.refinements.insert(0, MaskOp::MinIntensity { value: v });
                }
            }
        }
        Mapping::Majority => {
            let on = value.as_bool().ok_or_else(|| format!("{value} is not on or off"))?;
            for s in &mut config.masking.sections {
                s.refinements.retain(|op| !matches!(op, MaskOp::Majority));
                if on {
                    let at = s.refinements.iter().take_while(|op| matches!(op, MaskOp::MinIntensity { .. })).count();
                    s.refinements.insert(at, MaskOp::Majority);
                }
            }
        }
        Mapping::ThresholdMethod => {
            use crate::masking::MaskThresholdMethod as T;
            let method = match id {
                "otsu" => T::Otsu,
                "fixed" => T::Fixed,
                "percentile" => T::Percentile,
                "sti-gui" => T::StiGui,
                other => return Err(format!("threshold method {other} is not one QSMxT can set")),
            };
            let value = match method { T::Fixed => Some(0.5), T::Percentile => Some(75.0), _ => None };
            // STI Suite's threshold is a whole mask of its own (QSM_GUI thresholds one image), so a
            // recipe with another generator (BET, which QSM.rs reads as threshold method `none`)
            // becomes one threshold section; its input and refinements are rules of their own.
            if method == T::StiGui && config.masking.sections.iter().any(|s| !matches!(s.generator, MaskOp::Threshold { .. })) {
                let input = config.masking.sections.first().map_or(crate::masking::MaskingInput::Magnitude, |s| s.input);
                config.masking.sections = vec![crate::masking::MaskSection {
                    input, generator: MaskOp::Threshold { method, value }, refinements: vec![],
                }];
                config.masking.refinements.clear();
            }
            for s in &mut config.masking.sections {
                match &mut s.generator {
                    MaskOp::Threshold { method: m, value: v } => {
                        if *m != method {
                            *m = method;
                            *v = value;
                            if method == T::StiGui {
                                s.refinements.clear();
                            }
                        }
                    }
                    // Another method on a section that is not a threshold: that section's
                    // generator becomes the threshold, its refinements kept.
                    g => *g = MaskOp::Threshold { method, value },
                }
            }
        }
        Mapping::LargestComponent => {
            let on = value.as_bool().ok_or_else(|| format!("{value} is not on or off"))?;
            for s in &mut config.masking.sections {
                s.refinements.retain(|op| !matches!(op, MaskOp::LargestComponent));
                if on {
                    let at = s.refinements.iter().position(|op| matches!(op, MaskOp::Imfill)).unwrap_or(s.refinements.len());
                    s.refinements.insert(at, MaskOp::LargestComponent);
                }
            }
        }
        Mapping::Imfill => {
            let on = value.as_bool().ok_or_else(|| format!("{value} is not on or off"))?;
            for s in &mut config.masking.sections {
                s.refinements.retain(|op| !matches!(op, MaskOp::Imfill));
                if on {
                    s.refinements.push(MaskOp::Imfill);
                }
            }
        }
        Mapping::CloseRadius => {
            let r = value.as_u64().or_else(|| value.as_f64().filter(|v| *v >= 0.0).map(|v| v as u64))
                .ok_or_else(|| format!("{value} is not a radius in voxels"))? as usize;
            for s in &mut config.masking.sections {
                if r == 0 {
                    s.refinements.retain(|op| !matches!(op, MaskOp::Close { .. }));
                } else if let Some(MaskOp::Close { radius }) = s.refinements.iter_mut().find(|op| matches!(op, MaskOp::Close { .. })) {
                    *radius = r;
                } else {
                    let at = s.refinements.iter()
                        .take_while(|op| matches!(op, MaskOp::MinIntensity { .. } | MaskOp::Majority)).count();
                    s.refinements.insert(at, MaskOp::Close { radius: r });
                }
            }
        }
        Mapping::QsmartFitThreshold => {
            use crate::enums::QsmartFitThresholdMode as M;
            let fm = &mut config.field_mapping;
            match id {
                "absolute-40" => {
                    fm.qsmart_fit_threshold_mode = M::Absolute;
                    fm.qsmart_fit_threshold = qsm_core::utils::QSMART_FIT_THRESHOLD;
                }
                "percentile-99" => {
                    fm.qsmart_fit_threshold_mode = M::Percentile;
                    fm.qsmart_fit_percentile = qsm_core::utils::QSMART_FIT_PERCENTILE;
                }
                other => return Err(format!("QSMART fit threshold {other} needs its value")),
            }
        }
        Mapping::RomeoWeights => {
            use qsm_core::unwrap::{RomeoParams, RomeoWeightType as W};
            let preset = match id {
                "romeo4" => W::Romeo4,
                "romeo3" => W::Romeo3,
                "romeo2" => W::Romeo2,
                "romeo6" => W::Romeo6,
                "bestpath" => W::BestPath,
                other => return Err(format!("ROMEO weights {other} are not a preset QSMxT can set")),
            };
            let p = RomeoParams::from_weight_type(preset);
            let r = &mut config.field_mapping.romeo;
            r.phase_coherence = p.phase_coherence;
            r.phase_gradient_coherence = p.phase_gradient_coherence;
            r.phase_linearity = p.phase_linearity;
            r.mag_coherence = p.mag_coherence;
            r.mag_weight = p.mag_weight;
            r.mag_weight2 = p.mag_weight2;
            r.bestpath = p.bestpath;
        }
        Mapping::RomeoTemplate => {
            let echo = value.as_u64().or_else(|| value.as_f64().filter(|v| *v >= 1.0 && v.fract() == 0.0).map(|v| v as u64))
                .filter(|&e| e >= 1)
                .ok_or_else(|| format!("{value} is not an echo, counted from 1"))?;
            config.field_mapping.romeo.template = echo as usize - 1;
        }
    }
    Ok(())
}

/// QSM.rs's id of a mask op (`MaskOp::id`): the step's `op`.
fn op_id(op: &MaskOp) -> &'static str {
    let kind = crate::schema::mask::step_kind(op);
    mask_steps().iter().find(|s| s.kind == kind).map_or("", |s| s.op)
}

/// What [`accept`] applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Accept {
    /// The reasoned findings (`--accept-recommendations`).
    pub recommendations: bool,
    /// The original authors' settings (`--accept-reference-settings`).
    pub reference: bool,
}

impl Accept {
    pub fn any(self) -> bool {
        self.recommendations || self.reference
    }
}

/// What findings chosen together come to, setting by setting ([`settle`]). Every host decides this
/// here (#493): `qsmxt run` through [`accept`], qsmbly through [`apply_all`], the TUI through
/// [`settle`] on its pop-up's picks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settled {
    /// The findings whose value the setting takes: the strongest on each setting, with any weaker
    /// one that wants the same value.
    pub apply: Vec<Recommendation>,
    /// The findings left because they are as strong as each other on one setting and want
    /// different values, or because another chosen method's authors disagree with them
    /// ([`disagreeing`]). The setting keeps its value for the user to choose.
    pub conflicting: Vec<Recommendation>,
    /// The findings left because a stronger one on the setting wants another value.
    pub outranked: Vec<Recommendation>,
}

/// The setting a finding is about, by QSM.rs's path where it has one (QSMxT's own findings by
/// QSMxT's).
pub fn rule_setting(r: &Recommendation) -> &str {
    core_setting(&r.rule).unwrap_or(&r.setting)
}

/// The precedence of findings chosen together, in any order and with nothing changed: on each
/// setting, the strongest finding that is open (departed from and not dismissed) takes it, so a
/// reasoned finding outranks an authors' setting, unless two as strong disagree, when none is
/// taken. An authors' setting another chosen method's authors disagree with (its
/// [`Recommendation::conflict`], whether or not that other one is among `findings`) is not taken
/// either: the setting stays as it is for the user to choose. A host passing a value the user
/// picked between the authors clears its `conflict`. Findings on other settings are unaffected.
pub fn settle(findings: &[Recommendation]) -> Settled {
    settle_where(findings, |_| true)
}

/// [`settle`] over the findings `takes` lets in.
pub(super) fn settle_where(findings: &[Recommendation], takes: impl Fn(&Recommendation) -> bool) -> Settled {
    let mut settled = Settled::default();
    // By QSM.rs's setting: several are parts of one QSMxT setting (the mask recipe's generator,
    // input, BET f and erosion are all `masking.sections`), and findings on different parts do not
    // compete.
    let mut settings: Vec<&str> = Vec::new();
    for r in findings {
        if !settings.contains(&rule_setting(r)) {
            settings.push(rule_setting(r));
        }
    }
    for s in settings {
        let open: Vec<&Recommendation> = findings.iter()
            .filter(|r| rule_setting(r) == s && r.departs && !r.dismissed && takes(r)).collect();
        let Some(top) = open.iter().map(|r| r.strength).max() else { continue };
        let strongest: Vec<&Recommendation> = open.iter().copied().filter(|r| r.strength == top).collect();
        let value = &strongest[0].recommended.value;
        let authors_disagree = strongest.iter().any(|r| r.conflict && r.strength == Strength::Reference);
        if authors_disagree || strongest.iter().any(|r| r.recommended.value != *value) {
            let (level, weaker): (Vec<_>, Vec<_>) = open.into_iter().partition(|r| r.strength == top);
            settled.conflicting.extend(level.into_iter().cloned());
            settled.outranked.extend(weaker.into_iter().cloned());
            continue;
        }
        let (same, other): (Vec<_>, Vec<_>) = open.into_iter().partition(|r| r.recommended.value == *value);
        settled.apply.extend(same.into_iter().cloned());
        settled.outranked.extend(other.into_iter().cloned());
    }
    settled
}

/// What `--accept-recommendations` and `--accept-reference-settings` change: the findings of the
/// kinds `which` takes, as [`settle`] decides among them, on settings that apply on a scan with
/// `facts` ([`rule_may_change`]: a finding on a hidden setting is neither applied nor left).
/// Returns the findings applied (as they were before the change) and those left because they
/// conflict.
pub fn accept(config: &mut PipelineConfig, findings: &[Recommendation], which: Accept, facts: &Facts)
              -> (Vec<Recommendation>, Vec<Recommendation>) {
    let settled = settle_where(findings, |r| rule_may_change(config, &r.setting, facts) && match r.kind {
        Kind::Reasoned => which.recommendations && r.actionable() && r.strength.preselected(),
        Kind::Reference => which.reference,
    });
    let mut applied = Vec::new();
    for r in settled.apply {
        match apply_rule(config, &r.rule, None) {
            Ok(()) => applied.push(r),
            Err(e) => log::warn!(target: "qsmxt_engine::recommendations", "could not apply {}: {e}", r.rule),
        }
    }
    (applied, settled.conflicting)
}

/// Every recommendation for a config across the scans it runs on: those that do not depend on
/// the scan once, and each scan-dependent one once for each different finding, in QSM.rs's order.
pub fn across_scans(config: &PipelineConfig, scans: &[Facts]) -> Vec<Recommendation> {
    let mut out: Vec<Recommendation> = Vec::new();
    let unknown = [Facts::default()];
    for facts in if scans.is_empty() { &unknown[..] } else { scans } {
        for r in recommendations(config, facts) {
            if !out.contains(&r) {
                out.push(r);
            }
        }
    }
    let order = |r: &Recommendation| core::RULES.iter().position(|x| x.id == r.rule).unwrap_or(usize::MAX);
    out.sort_by_key(order);
    out
}
