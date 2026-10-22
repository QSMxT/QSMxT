//! Which settings apply to a config: the TUI's rules for which rows it shows, as a function of
//! the config.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::PipelineConfig;
use crate::enums::*;
use crate::masking::{MaskOp, MaskSection};
use crate::requirements::Output;

use super::Section;

/// What the config cannot say but decides whether some settings apply: facts about the data.
/// The default is "not known", which shows every setting the data could need.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewFacts {
    /// The number of echoes, when every run has the same (`None` = not known, or mixed).
    pub num_echoes: Option<usize>,
    /// Whether some run is uncombined multi-coil data (`None` = not known).
    pub multi_coil: Option<bool>,
}

/// How a setting applies to a config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SettingView {
    pub path: &'static str,
    /// Whether the setting applies, and so is shown. A setting that does not apply does not
    /// change the run.
    pub visible: bool,
    /// For a setting another one forces on: that setting's path. The setting then reads as on
    /// whatever its own value says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forced_by: Option<&'static str>,
    /// What is wrong with the setting's value, for showing next to it: outside its range, not a
    /// finite number, or refused by a rule of the setting ("Every dimension must be a positive
    /// multiple of 16"); for a mask recipe, each step parameter outside its range ("Section 1,
    /// step 2 (Erode): Iterations must be at least 1"). Only a setting that applies has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// What may go wrong with a value the run accepts, for showing next to the setting: the mask
    /// recipe under an inversion that needs a tighter mask ([`crate::masking::mask_warning`]),
    /// or tiling that makes iQFM, iQSM or iQSM+ only approximate its whole-volume result. Only a
    /// setting that applies has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// The config as the run sees it, and the facts its settings' visibility is decided from.
pub(crate) struct Ctx<'a> {
    /// The config with every implied output switched on (what the run does).
    pub c: &'a PipelineConfig,
    /// QSM is reconstructed (asked for, or needed by another output).
    pub qsm: bool,
    pub grouped: bool,
    pub cosmos: bool,
    pub laplacian: bool,
    pub multi_coil: bool,
    /// A selected method only works on axial data, so oblique data is resampled to axial
    /// whatever the obliquity threshold says.
    pub axial_forced: bool,
    pub uses_bet: bool,
    pub tileable: bool,
    /// A χ map is read: χ-separation, the statistics or SMWI.
    pub reads_qsm: bool,
    /// An R2 map is read: asked for, or the measured route to a computed R2'.
    pub reads_r2: bool,
    /// An R2' map is read: asked for, or the selected χ-separation method needs it.
    pub reads_r2prime: bool,
    /// R2' is computed (wanted and not supplied).
    pub computes_r2prime: bool,
    /// A segmentation is made or read.
    pub segmentation_on: bool,
    inv: String,
    bf: String,
    sep: String,
    qsmart_inner: String,
}

fn name<T: Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(Value::String(s)) => s,
        _ => String::new(),
    }
}

impl Ctx<'_> {
    pub fn inv(&self, alg: &str) -> bool { self.inv == alg }
    pub fn bf(&self, alg: &str) -> bool { self.bf == alg }
    /// χ-separation runs with this method.
    pub fn sep(&self, alg: &str) -> bool { self.c.pipeline.do_chi_separation && self.sep == alg }
    /// QSMART runs with this inner inversion.
    pub fn qsmart_inner(&self, alg: &str) -> bool { self.inv("qsmart") && self.qsmart_inner == alg }
}

/// `config` with the outputs other outputs need switched on, as a run applies them
/// ([`PipelineConfig::normalised`]).
fn resolved(config: &PipelineConfig) -> PipelineConfig {
    config.normalised().0
}

fn uses_bet(c: &PipelineConfig) -> bool {
    let bet = |op: &MaskOp| matches!(op, MaskOp::Bet { .. });
    let in_section = |s: &MaskSection| s.all_ops().iter().any(&bet);
    c.masking.sections.iter().any(in_section)
        || c.masking.refinements.iter().any(bet)
        || (c.masking.two_pass && c.masking.resolved_two_pass_sections().iter().any(in_section))
}

/// Whether the settings of a section apply at all: the QSM sub-tabs only while QSM is
/// reconstructed, and only for the stages the inversion runs.
fn section_applies(section: Section, x: &Ctx, facts: &ViewFacts) -> bool {
    let c = x.c;
    match section {
        Section::FieldMap => {
            let counts = match facts.num_echoes { Some(n) => vec![n], None => vec![1, 2] };
            x.qsm && counts.iter().any(|&n| crate::bridge::field_mapping_runs(c, n))
        }
        Section::Background => x.qsm && !crate::bridge::skips_bgremove(c),
        Section::Inversion | Section::Reference => x.qsm,
        Section::MultiOrientation => x.qsm && x.grouped,
        _ => true,
    }
}

/// The setting that switches `out` on, when another one does.
fn forcing_setting(c: &PipelineConfig, out: Output) -> Option<&'static str> {
    crate::requirements::implied_by_setting(c, out)
}

/// How each setting of [`schema`](super::schema) applies to `config`, in schema order: whether
/// it is shown, and which setting forces it on. The TUI's rules, from the config rather than
/// from its form.
pub fn settings_view(config: &PipelineConfig, facts: &ViewFacts) -> Vec<SettingView> {
    let c = resolved(config);
    let grouped = c.multi_orientation.enabled();
    let p = &c.pipeline;
    let s = &c.separation;
    let inv = name(&c.inversion.algorithm);
    let sep = name(&s.algorithm);
    let r2prime_based = !matches!(s.algorithm, SeparationAlgorithm::R2starQsm | SeparationAlgorithm::Decompose);
    let reads_r2prime = p.do_r2primemap || (p.do_chi_separation && r2prime_based);
    let computes_r2prime = reads_r2prime && s.custom_r2prime_tool.is_none();
    let x = Ctx {
        c: &c,
        qsm: p.do_qsm,
        grouped,
        cosmos: c.multi_orientation.algorithm == crate::config::MultiOrientAlgorithm::Cosmos,
        laplacian: c.field_mapping.unwrapping_algorithm == UnwrappingAlgorithm::Laplacian,
        multi_coil: facts.multi_coil.unwrap_or(true),
        axial_forced: !crate::bridge::axial_only_algorithms(
            &c, crate::bridge::RunOrientationFacts { has_measured_r2: false }).is_empty(),
        uses_bet: uses_bet(&c),
        tileable: c.runs_a_tileable_network(),
        reads_qsm: p.do_chi_separation || p.do_analysis || p.do_smwi,
        reads_r2: p.do_r2map || (computes_r2prime && s.r2prime_strategy != R2PrimeStrategy::R2primenet),
        reads_r2prime,
        computes_r2prime,
        segmentation_on: p.do_segmentation || c.segmentation.custom_dseg_tool.is_some(),
        inv,
        bf: name(&c.bg_removal.algorithm),
        sep,
        qsmart_inner: name(&c.inversion.qsmart.inversion),
    };
    let mut views: Vec<SettingView> = super::defs()
        .iter()
        .map(|d| {
            // Compute QSM is the switch that turns its sections on, so it is shown either way.
            let visible = d.path == "pipeline.do_qsm"
                || section_applies(d.section, &x, facts) && d.when.is_none_or(|f| f(&x));
            let forced_by = match d.path {
                // QSM another output needs runs whatever Compute QSM says.
                "pipeline.do_qsm" if !config.pipeline.do_qsm && c.pipeline.do_qsm => Some(
                    if config.pipeline.do_smwi && config.separation.custom_qsm_tool.is_none() {
                        "pipeline.do_smwi"
                    } else {
                        "pipeline.do_analysis"
                    }),
                "pipeline.do_segmentation" => forcing_setting(&c, Output::Segmentation),
                "pipeline.do_r2starmap" => forcing_setting(&c, Output::R2star),
                "pipeline.do_r2map" => forcing_setting(&c, Output::R2),
                "pipeline.do_r2primemap" => forcing_setting(&c, Output::R2prime),
                _ => None,
            };
            let warning = match d.path {
                "masking.sections" if visible => crate::masking::mask_warning(&c).map(String::from),
                _ => None,
            };
            SettingView { path: d.path, visible, forced_by, error: None, warning }
        })
        .collect();
    let visible: std::collections::HashSet<&str> = views.iter().filter(|v| v.visible).map(|v| v.path).collect();
    for (path, message) in errors(config, |p| visible.contains(p)) {
        if let Some(v) = views.iter_mut().find(|v| v.path == path) {
            v.error.get_or_insert(message);
        }
    }
    if let Some(note) = tiling_note(&c) {
        if let Some(v) = views.iter_mut().find(|v| v.path == "inversion.tile_size" && v.visible) {
            v.warning = Some(note);
        }
    }
    views
}

/// The warning on the tiling setting when it tiles a network that reconstructs from the wrapped
/// phase (iQFM, iQSM, iQSM+): each tile unwraps and removes the background on its own, so the
/// result differs noticeably from the whole-volume one.
fn tiling_note(c: &PipelineConfig) -> Option<String> {
    c.dl_tile()?;
    let network = crate::methods::tiled_phase_network(c)?;
    Some(format!(
        "{network} runs in overlapping tiles, which approximates its whole-volume result: \
         values can differ noticeably from a whole-volume run. Turn tiling off for the \
         whole-volume result where memory allows."
    ))
}

/// What is wrong with each checked setting: its range, and the rules a setting's own config
/// section checks.
fn errors(config: &PipelineConfig, checked: impl Fn(&str) -> bool) -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = super::out_of_range(config, &checked)
        .into_iter().map(|p| (p.path, p.message)).collect();
    let rules: [(&'static str, Result<(), String>); 4] = [
        ("field_mapping.b0_estimation", config.field_mapping.validate()),
        ("separation.r2primenet.patch", config.separation.r2primenet.validate()),
        ("separation.chi_sepnet.patch", config.separation.chi_sepnet.validate()),
        ("separation.susep_net.patch", config.separation.susep_net.validate()),
    ];
    for (path, result) in rules {
        if let (true, Err(e)) = (checked(path), result) {
            out.push((path, sentence(&e)));
        }
    }
    // The mask recipes' steps, against the mask schema's parameter ranges.
    let m = &config.masking;
    let recipes: [(&'static str, Vec<String>); 3] = [
        ("masking.sections", super::mask::check_sections(&m.sections)),
        ("masking.refinements", super::mask::check_steps(&m.refinements, "Combined mask")),
        ("masking.two_pass_sections", m.two_pass_sections.as_deref().map(super::mask::check_sections).unwrap_or_default()),
    ];
    for (path, problems) in recipes {
        if checked(path) && !problems.is_empty() {
            out.push((path, problems.join("; ")));
        }
    }
    out
}

/// A rule's message as a field's: what follows its subject ("R2PRIMEnet patch [..]: every
/// dimension ..."), capitalised.
fn sentence(message: &str) -> String {
    let rest = message.split_once(": ").map_or(message, |(_, r)| r);
    let mut chars = rest.chars();
    chars.next().map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// `config` with every setting that does not apply to it (see [`settings_view`]) put back to
/// QSMxT's default, so a value left behind by an earlier choice cannot change the run or show
/// up in its command.
pub fn prune_hidden(config: &PipelineConfig, facts: &ViewFacts) -> PipelineConfig {
    let hidden: Vec<&str> = settings_view(config, facts).into_iter()
        .filter(|v| !v.visible).map(|v| v.path).collect();
    if hidden.is_empty() {
        return config.clone();
    }
    let (Ok(defaults), Ok(mut out)) = (serde_json::to_value(PipelineConfig::default()), serde_json::to_value(config)) else {
        return config.clone();
    };
    for path in hidden {
        let (parent, key) = path.rsplit_once('.').unwrap_or(("", path));
        let default = super::lookup(&defaults, path).cloned().unwrap_or(Value::Null);
        let mut obj = &mut out;
        for k in parent.split('.').filter(|k| !k.is_empty()) {
            obj = &mut obj[k];
        }
        if let Some(map) = obj.as_object_mut() {
            map.insert(key.to_string(), default);
        }
    }
    // Every value written is a default the config already had, so this cannot fail.
    serde_json::from_value(out).unwrap_or_else(|_| config.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stages::field::map_field;

    /// What the command now leaves out does not change the run: the field map from a config with
    /// a hidden value left behind is the field map from the same config pruned, bit for bit.
    /// A tiled iQFM, iQSM or iQSM+ carries a warning on the tiling setting; other networks do not.
    #[test]
    fn tiling_the_phase_networks_carries_a_note() {
        let note = |c: &PipelineConfig| {
            settings_view(c, &ViewFacts::default()).into_iter()
                .find(|v| v.path == "inversion.tile_size").and_then(|v| v.warning)
        };
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Iqsm;
        assert_eq!(note(&c), None, "whole volume: nothing to note");
        c.inversion.tile_size = Some(56);
        let n = note(&c).expect("tiled iQSM has a note");
        assert!(n.starts_with("iQSM runs in overlapping tiles, which approximates its whole-volume result"), "{n}");
        c.inversion.algorithm = QsmAlgorithm::Rts;
        c.bg_removal.algorithm = crate::enums::BfAlgorithm::Iqfm;
        assert!(note(&c).is_some_and(|n| n.starts_with("iQFM")), "tiled iQFM");
        c.bg_removal.algorithm = crate::enums::BfAlgorithm::Vsharp;
        c.inversion.algorithm = QsmAlgorithm::Xqsm;
        assert_eq!(note(&c), None, "xQSM");
    }

    #[test]
    fn a_pruned_setting_does_not_change_the_field_map() {
        let mut meta = crate::stages::magnitude::tests::meta_4x4x4();
        meta.n_echoes = 4;
        meta.echo_times = vec![0.005, 0.010, 0.015, 0.020];
        let n = 64;
        // A smooth field, wrapped per echo.
        let phases: Vec<Vec<f64>> = meta.echo_times.iter().map(|te| {
            (0..n).map(|i| {
                let p = 2.0 * std::f64::consts::PI * 40.0 * te * (1.0 + (i % 4) as f64 * 0.3);
                (p + std::f64::consts::PI).rem_euclid(2.0 * std::f64::consts::PI) - std::f64::consts::PI
            }).collect()
        }).collect();
        let mags: Vec<Vec<f64>> = (0..4).map(|e| vec![1.0 - 0.1 * e as f64; n]).collect();
        let (p, m): (Vec<&[f64]>, Vec<&[f64]>) = (phases.iter().map(|v| &v[..]).collect(), mags.iter().map(|v| &v[..]).collect());
        let mask = vec![1u8; n];

        let mut cases = Vec::new();
        // --b0-weight-type under the linear and robust fits.
        for est in [B0Estimation::LinearFit, B0Estimation::RobustFit] {
            let mut c = PipelineConfig::default();
            c.field_mapping.b0_estimation = est;
            c.field_mapping.b0_weight_type = B0WeightType::Average;
            cases.push(c);
        }
        // --no-romeo-individual (and its template) under Laplacian unwrapping.
        let mut c = PipelineConfig::default();
        c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        c.field_mapping.romeo.individual = false;
        c.field_mapping.romeo.template = 1;
        cases.push(c);

        for c in cases {
            let pruned = prune_hidden(&c, &ViewFacts::default());
            assert_ne!(serde_json::to_value(&c).unwrap(), serde_json::to_value(&pruned).unwrap(), "something was pruned");
            let a = map_field(&p, &m, &mask, &meta, &c).unwrap();
            let b = map_field(&p, &m, &mask, &meta, &pruned).unwrap();
            assert!(a.b0_field_ppm.iter().zip(&b.b0_field_ppm).all(|(x, y)| x.to_bits() == y.to_bits()));
        }
    }
}
