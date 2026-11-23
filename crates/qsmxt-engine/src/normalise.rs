//! The configuration a run takes: the one it is given, with every output another setting needs
//! switched on ([`PipelineConfig::normalised`]), and those same needs as `required` findings for a
//! front end to offer ([`required_findings`]).
//!
//! Every entry point a front end calls with a configuration ([`crate::plan::plan`],
//! [`crate::run::run`] and [`crate::run::dry_run`], [`crate::generate_command`], the methods text,
//! the pipeline graph and [`crate::schema::settings_view`]) normalises it first, so a front end
//! passes the configuration as the user left it and gets `qsmxt run`'s behaviour. `qsmxt`
//! normalises the configuration it builds from its file and flags with the same function, once,
//! before any scan is read, and logs each change at info level: it is what `pipeline_config.toml`
//! holds and what the steps' cache keys are made from. Normalising a normalised configuration
//! changes nothing.
//!
//! **Two kinds of `required`.** These needs are QSMxT's wiring (an output another setting reads),
//! offered by a front end as [`Strength::Required`] findings of [`Basis::Wiring`] the moment the
//! setting that needs one is chosen ("Required change": apply it, or undo the change just made).
//! Left unresolved, the run applies them itself, says so in its log and records them in
//! [`PipelineConfig::required_changes`] and the methods text's Note. QSM.rs's `required` rules are
//! different: what a method cannot run with is refused before the run, never applied unasked
//! ([`crate::requirements`]).
//!
//! Normalisation reads only the configuration. What a scan's data rules out is the plan's to say
//! ([`crate::plan::Plan::notes`], [`crate::plan::Plan::check`]).
//!
//! It never changes a method's settings to suit another method: a run takes the methods and
//! settings it is given. Where a chosen method's reference pipeline uses something else, the
//! pipeline rules say so ([`crate::recommendations`]): the log, `--dry`, the methods Note and the
//! front ends' pop-ups. What a method cannot run with is refused before the run.

use serde::Serialize;
use serde_json::Value;

use crate::config::{
    enforce_analysis_dependencies, enforce_reference_dependencies, enforce_separation_dependencies,
    enforce_smwi_dependencies, PipelineConfig,
};
use crate::enums::SeparationAlgorithm;
use crate::recommendations::{Allowed, Basis, Kind, Named, Recommendation, Strength, Trigger};

/// The ids of the required changes [`PipelineConfig::normalised`] makes ([`Adjustment::rule`]).
pub const REQUIRED_RULES: &[&str] = &[
    "qsmxt.analysis-segmentation", "qsmxt.analysis-qsm", "qsmxt.smwi-qsm", "qsmxt.chisep-r2prime",
    "qsmxt.chisep-r2star", "qsmxt.r2prime-r2star", "qsmxt.r2prime-r2", "qsmxt.reference-segmentation",
    "qsmxt.grouping-magnitude", BIPOLAR_READOUT_RULE, BIPOLAR_OFFSET_REMOVAL_RULE,
];

/// The required change a bipolar readout makes ([`scan_adjustments`]): bipolar correction on.
pub const BIPOLAR_READOUT_RULE: &str = "qsmxt.bipolar-readout";
/// The required change that goes with it where bipolar correction would be skipped: phase-offset
/// removal on, of which bipolar correction is a part.
pub const BIPOLAR_OFFSET_REMOVAL_RULE: &str = "qsmxt.bipolar-offset-removal";

/// One change [`PipelineConfig::normalised`] made to a configuration, and why: what a front end
/// lists beside the plan, and offers beforehand as a required change ([`required_findings`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Adjustment {
    /// The setting that was changed, by its path in the configuration
    /// (`pipeline.do_segmentation`).
    pub setting: &'static str,
    /// The setting that needs the change.
    pub because: &'static str,
    /// The finding's id ([`required_findings`]): `qsmxt.reference-segmentation`.
    pub rule: &'static str,
    /// The output turned on, by name ("Segmentation", "The R2* map").
    pub output: &'static str,
    /// What needs it, by the setting's name and value ("Reference = ventricles").
    pub needed_by: String,
    /// Why, in a phrase ("the reference region is measured on ...").
    pub reason: String,
    /// The change and its reason, in a sentence that names settings rather than flags, as the
    /// log says it: "Segmentation turned on: Reference = ventricles needs it (...)".
    pub message: String,
    /// What the change rests on: [`Basis::Wiring`] (an output another setting needs) or
    /// [`Basis::Acquisition`] (what the scan's acquisition needs, [`scan_adjustments`]).
    pub basis: Basis,
}

impl Adjustment {
    fn new(rule: &'static str, setting: &'static str, because: &'static str, output: &'static str,
           needed_by: impl Into<String>, reason: impl Into<String>) -> Self {
        let (needed_by, reason) = (needed_by.into(), reason.into());
        let message = format!("{output} turned on: {needed_by} needs it ({reason})");
        Adjustment { setting, because, rule, output, needed_by, reason, message, basis: Basis::Wiring }
    }

    /// An [`Adjustment::new`] that the scan's acquisition needs ([`Basis::Acquisition`]).
    fn for_scan(rule: &'static str, setting: &'static str, because: &'static str, output: &'static str,
                needed_by: impl Into<String>, reason: impl Into<String>) -> Self {
        Adjustment { basis: Basis::Acquisition, ..Self::new(rule, setting, because, output, needed_by, reason) }
    }

    /// The finding a front end offers for this change before the run ([`required_findings`]).
    fn finding(&self, config: &Value) -> Recommendation {
        let named = |on: bool| Named { value: Value::Bool(on), name: if on { "on" } else { "off" }.into() };
        let trigger_value = if self.because == READOUT_FACT {
            Value::String(crate::scan::ReadoutPolarity::Bipolar.id().into())
        } else {
            self.because.split('.').try_fold(config, |v, k| v.get(k)).cloned().unwrap_or(Value::Null)
        };
        let scan = self.basis == Basis::Acquisition;
        Recommendation {
            rule: self.rule.into(),
            kind: Kind::Reasoned,
            strength: Strength::Required,
            basis: self.basis,
            setting: self.setting.into(),
            label: self.output.trim_start_matches("The ").to_string(),
            trigger: Trigger { setting: self.because.into(), value: trigger_value, name: self.needed_by.clone() },
            current: vec![named(false)],
            recommended: named(true),
            allowed: Allowed::OneOf { values: vec![named(true)] },
            reason: capitalised(&self.reason),
            source: if scan { BIPOLAR_SOURCE } else { "QSMxT (how its steps link together)" }.into(),
            needs_scan: scan,
            departs: true,
            dismissed: false,
            conflict: false,
        }
    }
}

/// The trigger of a change the scan's readout polarity makes ([`scan_adjustments`]): not a setting.
pub const READOUT_FACT: &str = "scan.readout_polarity";

const BIPOLAR_SOURCE: &str = "QSMxT; Eckstein 2021 (PhD thesis, TU Wien, Section 3.1.3); QSMxT/QSMxT#564 \
    (EMTP travelling heads, bipolar GE MR750w: without correction 89 % of voxels > 3 Hz off, χ scaled ×3.6–5.7)";

/// Whether bipolar correction is in play in `c` on a scan with `n_echoes` (when known): QSM is
/// reconstructed from a field map that combines the echoes (not per echo, not a total-field
/// inversion from the wrapped phase), as QSM.rs's rules see it.
fn bipolar_in_play(c: &PipelineConfig, n_echoes: Option<usize>) -> bool {
    use qsm_core::pipeline::rules as core;
    if !c.pipeline.do_qsm || n_echoes.is_some_and(|n| n < 3) {
        return false;
    }
    let pipeline = crate::bridge::to_qsm_pipeline_config(c);
    let facts = core::ScanFacts { echo_count: n_echoes, ..Default::default() };
    !core::current(&core::RuleInput::pipeline(&pipeline), &facts, "field_mapping.bipolar_correction")
        .unwrap_or_default().is_empty()
}

/// The changes a scan's acquisition needs, made to `c` (a configuration already
/// [`PipelineConfig::normalised`]): with a **bipolar readout** (`readout`, from the sidecar's
/// `ReadoutPolarity`) and at least three echoes (or an echo count not known), bipolar correction
/// is turned on wherever it is in play ([`BIPOLAR_READOUT_RULE`]); where it would then be skipped
/// because phase-offset removal is off (STI Suite's, QSMART's and UK Biobank's pipelines; not the
/// complex fit, which is MEDI's bipolar fit then), phase-offset removal is turned on too
/// ([`BIPOLAR_OFFSET_REMOVAL_RULE`]), unless the Laplacian field is kept outside the mask, which
/// refuses it.
///
/// Decided 2026-11-23 (orchestrator decision 222(2), QSMxT#564): without correction the field of
/// a bipolar scan is wrong almost everywhere (on the EMTP travelling heads' bipolar GE MR750w, 89 %
/// of voxels more than 3 Hz off, χ scaled ×3.6–5.7 on the local protocol). A front end offers it as
/// a "Required change" ([`Basis::Acquisition`]); a run left without it applies it, says so in its
/// log and records it, as [`PipelineConfig::normalised`]'s changes (decision 183).
pub fn scan_adjustments(
    c: &mut PipelineConfig, readout: Option<crate::scan::ReadoutPolarity>, n_echoes: Option<usize>,
) -> Vec<Adjustment> {
    let mut out = Vec::new();
    if readout != Some(crate::scan::ReadoutPolarity::Bipolar) || !bipolar_in_play(c, n_echoes) {
        return out;
    }
    let fm = &mut c.field_mapping;
    if !fm.bipolar_correction {
        fm.bipolar_correction = true;
        out.push(Adjustment::for_scan(BIPOLAR_READOUT_RULE, "field_mapping.bipolar_correction", READOUT_FACT,
            "Bipolar correction", "the scan's bipolar readout",
            "its odd and even echoes carry opposite phase errors, which the field map would take for field"));
    }
    let complex_fit = fm.b0_estimation == crate::enums::B0Estimation::ComplexFit;
    if !fm.phase_offset_removal && !complex_fit && !fm.laplacian_unmasked && !fm.laplacian_keep_outside {
        fm.phase_offset_removal = true;
        out.push(Adjustment::for_scan(BIPOLAR_OFFSET_REMOVAL_RULE, "field_mapping.phase_offset_removal",
            "field_mapping.bipolar_correction", "Phase offset removal", "bipolar correction for the scan's bipolar readout",
            "bipolar correction runs as part of MCPC-3D-S phase-offset removal"));
    }
    for a in &out {
        if !c.required_changes.contains(&a.message) {
            c.required_changes.push(a.message.clone());
        }
    }
    out
}

/// [`scan_adjustments`] as `required` findings on `config` as given, for a front end that knows
/// the scan's readout polarity (`facts`).
pub fn scan_required_findings(config: &PipelineConfig, facts: &crate::recommendations::Facts) -> Vec<Recommendation> {
    let mut c = config.normalised().0;
    let json = serde_json::to_value(config).unwrap_or(Value::Null);
    scan_adjustments(&mut c, facts.readout_polarity, facts.echo_count).iter().map(|a| a.finding(&json)).collect()
}

/// `s` with its first letter in capitals, if it is a Latin one ("χ-separation" stays as it is).
fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| std::iter::once(f.to_ascii_uppercase()).chain(c).collect())
}

/// The outputs `config` leaves off that another of its settings needs, as `required` findings
/// ([`Basis::Wiring`]): what a front end offers as a "Required change" when that setting is
/// chosen, and marks while it is left unresolved. A run applies them itself
/// ([`PipelineConfig::normalised`]).
pub fn required_findings(config: &PipelineConfig) -> Vec<Recommendation> {
    let json = serde_json::to_value(config).unwrap_or(Value::Null);
    let (_, adjustments) = config.normalised();
    adjustments.iter().map(|a| a.finding(&json)).collect()
}

impl PipelineConfig {
    /// This configuration as a run takes it, and every change that made to it. Each change is
    /// also added to [`PipelineConfig::required_changes`], for the run's provenance.
    ///
    /// The rules, applied in this order:
    /// - per-structure statistics need a segmentation (unless one is supplied) and some map to
    ///   summarise (QSM, when nothing else would make one);
    /// - SMWI needs a susceptibility map (QSM, unless one is supplied);
    /// - χ-separation needs the relaxometry maps its method reads, and a computed R2′ needs R2*
    ///   (and R2, where R2′ may be measured);
    /// - a region reference is measured on the segmentation;
    /// - multi-orientation grouping registers the orientations on their published combined
    ///   magnitude, so it is published.
    ///
    /// Idempotent: normalising the result changes nothing and returns no adjustments.
    pub fn normalised(&self) -> (PipelineConfig, Vec<Adjustment>) {
        let mut c = self.clone();
        let mut out = Vec::new();

        let before = c.clone();
        enforce_analysis_dependencies(&mut c);
        if turned_on(&before, &c, |c| c.pipeline.do_segmentation) {
            out.push(Adjustment::new("qsmxt.analysis-segmentation", "pipeline.do_segmentation", "pipeline.do_analysis",
                "Segmentation", "the per-structure statistics", "they are measured over the segmentation"));
        }
        if turned_on(&before, &c, |c| c.pipeline.do_qsm) {
            out.push(Adjustment::new("qsmxt.analysis-qsm", "pipeline.do_qsm", "pipeline.do_analysis",
                "QSM", "the per-structure statistics", "they need a map to summarise, and no other is computed"));
        }

        let before = c.clone();
        enforce_smwi_dependencies(&mut c);
        if turned_on(&before, &c, |c| c.pipeline.do_qsm) {
            out.push(Adjustment::new("qsmxt.smwi-qsm", "pipeline.do_qsm", "pipeline.do_smwi",
                "QSM", "SMWI", "SMWI weights the magnitude by the susceptibility map"));
        }

        separation_dependencies(&mut c, &mut out);

        let before = c.clone();
        enforce_reference_dependencies(&mut c);
        if turned_on(&before, &c, |c| c.pipeline.do_segmentation) {
            let region = c.qsm.reference_region.as_deref().unwrap_or_default().trim().to_string();
            out.push(Adjustment::new("qsmxt.reference-segmentation", "pipeline.do_segmentation", "qsm.reference",
                "Segmentation", format!("Reference = {region}"),
                "the reference region is measured on a SynthSeg segmentation of the magnitude, which is \
                 saved with the outputs"));
        }

        if c.multi_orientation.enabled() && !c.pipeline.do_magnitude {
            c.pipeline.do_magnitude = true;
            out.push(Adjustment::new("qsmxt.grouping-magnitude", "pipeline.do_magnitude", "multi_orientation.group_by",
                "The combined magnitude", "orientation grouping",
                "the orientations are registered on their published combined magnitude"));
        }

        for a in &out {
            if !c.required_changes.contains(&a.message) {
                c.required_changes.push(a.message.clone());
            }
        }
        (c, out)
    }

    /// [`Self::normalised`], for a run on the scan `facts` describes: what [`crate::plan::plan`]
    /// plans and the run takes. χ-sepnet's toolbox route estimates its own R2′ unless this scan
    /// can measure one; where it can, the R2′ and R2 maps are computed for it, and kept with the
    /// run's intermediates unless they were asked for ([`Self::computes_unasked_r2prime`]). Those
    /// are not required changes: no setting is changed for them.
    pub fn normalised_for(&self, facts: &crate::scan::ScanFacts) -> (PipelineConfig, Vec<Adjustment>) {
        let (mut c, mut out) = self.normalised();
        out.extend(scan_adjustments(&mut c, facts.readout_polarity, Some(facts.n_echoes)));
        if crate::requirements::measures_r2prime_for_chisepnet(facts, &c) {
            measure_r2prime_for_chisepnet(&mut c);
        }
        (c, out)
    }

    /// [`Self::normalised_for`] the scan `plan` was made for ([`crate::plan::Plan::config`]).
    pub fn normalised_as_planned(&self, plan: &crate::plan::Plan) -> PipelineConfig {
        let (mut c, _) = self.normalised();
        if plan.config.pipeline.do_r2primemap && crate::config::chisepnet_route_can_estimate_r2prime(&c) {
            measure_r2prime_for_chisepnet(&mut c);
        }
        // The changes the scan's acquisition needed ([`scan_adjustments`]), as the plan made them.
        for a in plan.adjustments.iter().filter(|a| a.basis == Basis::Acquisition) {
            let _ = crate::schema::set(&mut c, a.setting, Value::Bool(true));
            if !c.required_changes.contains(&a.message) {
                c.required_changes.push(a.message.clone());
            }
        }
        c
    }

    /// The R2′ (and R2) maps a run of this configuration computes on a scan that can measure R2′
    /// only for χ-sepnet's toolbox route, not because they were asked for: kept with the run's
    /// intermediates, not published. `self` is the configuration as normalised before any scan
    /// ([`Self::normalised`]); returns (R2′, R2).
    pub fn computes_unasked_r2prime(&self) -> (bool, bool) {
        if !crate::config::chisepnet_route_can_estimate_r2prime(self) {
            return (false, false);
        }
        (!self.pipeline.do_r2primemap, !self.pipeline.do_r2map)
    }
}

fn turned_on(before: &PipelineConfig, after: &PipelineConfig, get: impl Fn(&PipelineConfig) -> bool) -> bool {
    !get(before) && get(after)
}

/// [`enforce_separation_dependencies`], saying what it turned on.
fn separation_dependencies(c: &mut PipelineConfig, out: &mut Vec<Adjustment>) {
    let before = c.clone();
    enforce_separation_dependencies(c);
    let method = crate::methods::separation_name(c.separation.algorithm);
    let chisep = method.to_string();
    if turned_on(&before, c, |c| c.pipeline.do_r2primemap) {
        out.push(Adjustment::new("qsmxt.chisep-r2prime", "pipeline.do_r2primemap", "pipeline.do_chi_separation",
            "The R2′ map", chisep.clone(), "it separates χ with R2′"));
    }
    if turned_on(&before, c, |c| c.pipeline.do_r2starmap) {
        let separation_reads_it = c.pipeline.do_chi_separation
            && (c.separation.algorithm == SeparationAlgorithm::R2starQsm
                || crate::config::chisepnet_route_can_estimate_r2prime(c));
        out.push(if separation_reads_it {
            Adjustment::new("qsmxt.chisep-r2star", "pipeline.do_r2starmap", "pipeline.do_chi_separation",
                "The R2* map", chisep.clone(), "it reads R2*")
        } else {
            Adjustment::new("qsmxt.r2prime-r2star", "pipeline.do_r2starmap", "pipeline.do_r2primemap",
                "The R2* map", "the R2′ map", "R2′ is computed from R2*")
        });
    }
    if turned_on(&before, c, |c| c.pipeline.do_r2map) {
        out.push(Adjustment::new("qsmxt.r2prime-r2", "pipeline.do_r2map", "pipeline.do_r2primemap",
            "The R2 map", "the R2′ map", "R2′ is measured as R2* − R2 where it can be"));
    }
}

/// The scan can measure R2′, and χ-sepnet's toolbox route takes a measured R2′ when it has one:
/// the maps it is measured from are computed ([`PipelineConfig::computes_unasked_r2prime`]).
fn measure_r2prime_for_chisepnet(c: &mut PipelineConfig) {
    c.pipeline.do_r2primemap = true;
    enforce_separation_dependencies(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::R2PrimeStrategy;
    use crate::enums::{QsmAlgorithm, QsmReference};

    /// Every combination of the settings the rules read, each normalised twice.
    fn configs() -> Vec<PipelineConfig> {
        let algorithms = [
            SeparationAlgorithm::R2starQsm, SeparationAlgorithm::Decompose, SeparationAlgorithm::ChiSepIlsqr,
            SeparationAlgorithm::SusepNet,
        ];
        let strategies = [R2PrimeStrategy::Auto, R2PrimeStrategy::Mese, R2PrimeStrategy::R2primenet];
        let mut out = Vec::new();
        for bits in 0u32..(1 << 12) {
            let on = |i: u32| bits & (1 << i) != 0;
            let mut c = PipelineConfig::default();
            c.pipeline.do_qsm = on(0);
            c.pipeline.do_analysis = on(1);
            c.pipeline.do_smwi = on(2);
            c.pipeline.do_chi_separation = on(3);
            c.pipeline.do_r2primemap = on(4);
            c.pipeline.do_r2starmap = on(5);
            c.segmentation.custom_dseg_tool = on(6).then(|| "mine".into());
            c.separation.custom_qsm_tool = on(7).then(|| "mine".into());
            c.separation.custom_r2prime_tool = on(8).then(|| "mine".into());
            c.separation.custom_r2_tool = on(9).then(|| "mine".into());
            if on(10) {
                c.qsm.reference = QsmReference::Region;
                c.qsm.reference_region = on(11).then(|| "thalamus".into());
            }
            c.separation.algorithm = algorithms[bits as usize % algorithms.len()];
            c.separation.r2prime_strategy = strategies[bits as usize % strategies.len()];
            out.push(c);
        }
        out
    }

    #[test]
    fn normalising_twice_is_normalising_once() {
        for c in configs() {
            let (once, _) = c.normalised();
            let (twice, again) = once.normalised();
            assert_eq!(twice, once, "{:?}", c.pipeline);
            assert!(again.is_empty(), "{again:?}");
        }
    }

    /// The adjustments are exactly the settings that changed, each said once, in plain words.
    #[test]
    fn every_change_is_an_adjustment_and_names_no_flag() {
        let paths: Vec<&str> = crate::schema::schema().iter().map(|s| s.path).collect();
        for c in configs() {
            let (n, adjustments) = c.normalised();
            let changed: Vec<&str> = [
                ("pipeline.do_qsm", c.pipeline.do_qsm != n.pipeline.do_qsm),
                ("pipeline.do_segmentation", c.pipeline.do_segmentation != n.pipeline.do_segmentation),
                ("pipeline.do_r2starmap", c.pipeline.do_r2starmap != n.pipeline.do_r2starmap),
                ("pipeline.do_r2map", c.pipeline.do_r2map != n.pipeline.do_r2map),
                ("pipeline.do_r2primemap", c.pipeline.do_r2primemap != n.pipeline.do_r2primemap),
            ].into_iter().filter(|(_, d)| *d).map(|(s, _)| s).collect();
            let mut said: Vec<&str> = adjustments.iter().map(|a| a.setting).collect();
            said.sort();
            let mut changed = changed;
            changed.sort();
            assert_eq!(said, changed, "{:?}", c.pipeline);
            assert_eq!(n.qsm.reference, c.qsm.reference, "the reference is never rewritten");
            assert_eq!(n.required_changes, adjustments.iter().map(|a| a.message.clone()).collect::<Vec<_>>(),
                       "each change is recorded once");
            for a in &adjustments {
                assert!(REQUIRED_RULES.contains(&a.rule), "{} is listed", a.rule);
                assert!(!a.message.contains("--"), "{}", a.message);
                assert!(paths.contains(&a.setting) && paths.contains(&a.because), "{a:?} names settings the schema has");
            }
        }
    }

    /// The command, the methods text and which settings apply, of a configuration as given, are
    /// those of its normalised configuration.
    #[test]
    fn the_command_methods_and_settings_view_take_the_normalised_configuration() {
        let facts = crate::schema::ViewFacts::default();
        let mut cases = Vec::new();
        for edit in [
            (|c| c.pipeline.do_r2primemap = true) as fn(&mut PipelineConfig),
            |c| c.pipeline.do_analysis = true,
            |c| { c.pipeline.do_qsm = false; c.pipeline.do_smwi = true; },
            |c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = SeparationAlgorithm::WaveSep; },
            |c| { c.qsm.reference = QsmReference::Region; c.qsm.reference_region = Some("thalamus".into()); },
        ] {
            let mut c = PipelineConfig::default();
            edit(&mut c);
            cases.push(c);
        }
        for raw in cases {
            let n = raw.normalised().0;
            assert_ne!(n, raw);
            assert_eq!(crate::generate_command(&raw), crate::generate_command(&n));
            assert_eq!(crate::generate_methods(&raw), crate::generate_methods(&n));
            // Which settings apply; which are forced on is said of the configuration as given.
            let visible = |c: &PipelineConfig| crate::schema::settings_view(c, &facts).into_iter()
                .map(|v| (v.path, v.visible)).collect::<Vec<_>>();
            assert_eq!(visible(&raw), visible(&n));
        }
    }

    #[test]
    fn each_rule_says_what_it_changed_and_why() {
        let because = |c: &PipelineConfig| -> Vec<(&str, &str)> {
            c.normalised().1.iter().map(|a| (a.setting, a.because)).collect()
        };
        let mut c = PipelineConfig::default();
        c.pipeline.do_analysis = true;
        assert_eq!(because(&c), [("pipeline.do_segmentation", "pipeline.do_analysis")]);
        c.pipeline.do_qsm = false;
        assert_eq!(because(&c), [("pipeline.do_segmentation", "pipeline.do_analysis"),
                                 ("pipeline.do_qsm", "pipeline.do_analysis")]);

        let mut c = PipelineConfig::default();
        c.pipeline.do_qsm = false;
        c.pipeline.do_smwi = true;
        assert_eq!(because(&c), [("pipeline.do_qsm", "pipeline.do_smwi")]);

        let mut c = PipelineConfig::default();
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr;
        assert_eq!(because(&c), [("pipeline.do_r2primemap", "pipeline.do_chi_separation"),
                                 ("pipeline.do_r2starmap", "pipeline.do_r2primemap"),
                                 ("pipeline.do_r2map", "pipeline.do_r2primemap")]);
        c.separation.algorithm = SeparationAlgorithm::R2starQsm;
        assert_eq!(because(&c), [("pipeline.do_r2starmap", "pipeline.do_chi_separation")]);

        let mut c = PipelineConfig::default();
        c.qsm.reference = QsmReference::Region;
        c.qsm.reference_region = Some("thalamus".into());
        assert_eq!(because(&c), [("pipeline.do_segmentation", "qsm.reference")]);
        // A region reference that names no region is a configuration error, not rewritten.
        c.qsm.reference_region = Some(" ".into());
        assert!(because(&c).is_empty());
        assert_eq!(c.normalised().0.qsm.reference, QsmReference::Region);

        let mut c = PipelineConfig::default();
        c.pipeline.do_magnitude = false;
        c.multi_orientation.group_by = "acq".into();
        assert_eq!(because(&c), [("pipeline.do_magnitude", "multi_orientation.group_by")]);

        assert!(PipelineConfig::default().normalised().1.is_empty());
    }
    /// Choosing a method changes no other setting: MEDI keeps the general background removal,
    /// QSMART the general mask and field mapping. Their references are the rules' to offer.
    #[test]
    fn a_method_changes_no_other_setting() {
        for alg in [QsmAlgorithm::Medi, QsmAlgorithm::Qsmart, QsmAlgorithm::Tfi, QsmAlgorithm::Star] {
            let mut c = PipelineConfig::default();
            c.inversion.algorithm = alg;
            let (n, adjustments) = c.normalised();
            assert_eq!(n, c, "{alg:?}");
            assert!(adjustments.is_empty(), "{alg:?}: {adjustments:?}");
        }
        // QSMART's echo fit under another inversion keeps its unwrapping: ROMEO with it is refused
        // before the run, not replaced.
        let mut c = PipelineConfig::default();
        c.field_mapping.b0_estimation = crate::enums::B0Estimation::QsmartEchofit;
        assert_eq!(c.normalised().0, c);
    }
    /// χ-sepnet's toolbox route needs R2* and says so, leaves R2′ as the user set it, and asks for
    /// the map only on a scan that can measure it.
    #[test]
    fn the_toolbox_route_says_only_what_it_needs() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = SeparationAlgorithm::ChiSepNet;
        let (n, notes) = c.normalised();
        assert!(!n.pipeline.do_r2primemap && !n.pipeline.do_r2map && n.pipeline.do_r2starmap);
        let notes: Vec<_> = notes.iter().map(|a| (a.setting, a.because, a.message.as_str())).collect();
        assert_eq!(notes, [("pipeline.do_r2starmap", "pipeline.do_chi_separation",
            "The R2* map turned on: χ-sepnet (deep-learning source separation) needs it (it reads R2*)")]);

        let mut facts = crate::scan::ScanFacts {
            dims: (8, 8, 8), voxel_size: None, n_echoes: 3, echo_times: vec![0.01, 0.02, 0.03], has_magnitude: true,
            n_coils: 1, real_imaginary: false, field_strength: 3.0, nominal_field_strength: None, imaging_frequency: None, b0_dir: None, mese_echo_times: None,
            custom_mask: false, custom_dseg: false, custom_qsm: false, custom_r2: false, custom_r2prime: false,
            manufacturer: None, readout_polarity: None,
        };
        assert_eq!(c.normalised_for(&facts).0, n, "nothing to measure from");
        facts.mese_echo_times = Some(vec![0.01, 0.02, 0.03, 0.04]);
        let (measured, notes) = c.normalised_for(&facts);
        assert!(measured.pipeline.do_r2primemap && measured.pipeline.do_r2map);
        // Computed for the route, not asked for: no required change, and kept as intermediates.
        assert!(!notes.iter().any(|a| a.setting == "pipeline.do_r2primemap"), "{notes:?}");
        assert!(!measured.required_changes.iter().any(|m| m.contains("R2′ map")), "{:?}", measured.required_changes);
        assert_eq!(n.computes_unasked_r2prime(), (true, true));
        assert_eq!(measured.normalised_for(&facts).0, measured);
        assert!(measured.normalised_for(&facts).1.is_empty());

        // A saved config with the toggle on is the user's request, on any scan.
        let mut saved = n.clone();
        saved.pipeline.do_r2primemap = true;
        let (kept, _) = saved.normalised();
        assert!(kept.pipeline.do_r2primemap);
    }
}
