//! Which settings apply to a config: the TUI's rules for which rows it shows, as a function of
//! the config.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{
    enforce_analysis_dependencies, enforce_reference_dependencies, enforce_separation_dependencies,
    enforce_smwi_dependencies, PipelineConfig,
};
use crate::enums::*;
use crate::masking::{MaskOp, MaskSection};
use crate::requirements::{implied_by, Output};

use super::{table, Section};

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

/// `config` with the outputs other outputs need switched on, as a run applies them.
fn resolved(config: &PipelineConfig) -> PipelineConfig {
    let mut c = config.clone();
    enforce_analysis_dependencies(&mut c);
    enforce_smwi_dependencies(&mut c);
    enforce_separation_dependencies(&mut c);
    enforce_reference_dependencies(&mut c);
    c
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
    let by = implied_by(c, out)?;
    Some(match by.split_whitespace().next().unwrap_or("") {
        "--do-analysis" => "pipeline.do_analysis",
        "--qsm-reference" => "qsm.reference",
        "--do-chisep" => "pipeline.do_chi_separation",
        "--do-r2primemap" => "pipeline.do_r2primemap",
        _ => return None,
    })
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
        tileable: matches!(c.inversion.algorithm,
            QsmAlgorithm::Xqsm | QsmAlgorithm::Qsmnet | QsmAlgorithm::QsmnetPlus | QsmAlgorithm::Ir2qsm
            | QsmAlgorithm::Lpcnn | QsmAlgorithm::ModlQsm | QsmAlgorithm::Nextqsm),
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
    table::table()
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
            SettingView { path: d.path, visible, forced_by }
        })
        .collect()
}

/// `config` with every setting that does not apply to it (see [`settings_view`]) put back to
/// QSMxT's default, so a value left behind by an earlier choice cannot change the run.
pub fn prune_hidden(config: &PipelineConfig, facts: &ViewFacts) -> PipelineConfig {
    let defaults = PipelineConfig::default();
    let mut out = config.clone();
    for v in settings_view(config, facts) {
        if !v.visible {
            if let Some(d) = super::get(&defaults, v.path) {
                // A default always round-trips.
                let _ = super::set(&mut out, v.path, d);
            }
        }
    }
    out
}
