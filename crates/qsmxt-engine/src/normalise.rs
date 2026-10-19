//! The configuration a run takes: the one it is given, with every output another output needs
//! switched on ([`PipelineConfig::normalised`]).
//!
//! Every entry point a front end calls with a configuration ([`crate::plan::plan`],
//! [`crate::run::run`] and [`crate::run::dry_run`], [`crate::generate_command`], the methods text,
//! the pipeline graph and [`crate::schema::settings_view`]) normalises it first, so a front end
//! passes the configuration as the user left it and gets `qsmxt run`'s behaviour. `qsmxt`
//! normalises the configuration it builds from its file and flags with the same function, once,
//! before any scan is read: it is what `pipeline_config.toml` holds and what the steps' cache keys
//! are made from. Normalising a normalised configuration changes nothing.
//!
//! Normalisation reads only the configuration. What a scan's data rules out is the plan's to say
//! ([`crate::plan::Plan::notes`], [`crate::plan::Plan::check`]).

use serde::Serialize;

use crate::config::{
    enforce_analysis_dependencies, enforce_reference_dependencies, enforce_separation_dependencies,
    enforce_smwi_dependencies, PipelineConfig,
};
use crate::enums::{QsmReference, SeparationAlgorithm};

/// One change [`PipelineConfig::normalised`] made to a configuration, and why: what a front end
/// lists beside the plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Adjustment {
    /// The setting that was changed, by its path in the configuration
    /// (`pipeline.do_segmentation`).
    pub setting: &'static str,
    /// The setting that needs the change.
    pub because: &'static str,
    /// The change and its reason, in a sentence that names settings rather than flags.
    pub message: String,
}

impl PipelineConfig {
    /// This configuration as a run takes it, and every change that made to it.
    ///
    /// The rules, applied in this order:
    /// - per-structure statistics need a segmentation (unless one is supplied) and some map to
    ///   summarise (QSM, when nothing else would make one);
    /// - SMWI needs a susceptibility map (QSM, unless one is supplied);
    /// - χ-separation needs the relaxometry maps its method reads, and a computed R2′ needs R2*
    ///   (and R2, where R2′ may be measured);
    /// - a region reference is measured on the segmentation, and a region reference that names
    ///   no region is the mean.
    ///
    /// Idempotent: normalising the result changes nothing and returns no adjustments.
    pub fn normalised(&self) -> (PipelineConfig, Vec<Adjustment>) {
        let mut c = self.clone();
        let mut out = Vec::new();

        let before = c.clone();
        enforce_analysis_dependencies(&mut c);
        if turned_on(&before, &c, |c| c.pipeline.do_segmentation) {
            out.push(note("pipeline.do_segmentation", "pipeline.do_analysis",
                "Segmentation is on: the per-structure statistics are measured over it"));
        }
        if turned_on(&before, &c, |c| c.pipeline.do_qsm) {
            out.push(note("pipeline.do_qsm", "pipeline.do_analysis",
                "QSM is computed: the per-structure statistics need a map, and no other is computed"));
        }

        let before = c.clone();
        enforce_smwi_dependencies(&mut c);
        if turned_on(&before, &c, |c| c.pipeline.do_qsm) {
            out.push(note("pipeline.do_qsm", "pipeline.do_smwi",
                "QSM is computed: SMWI weights the magnitude by the susceptibility map"));
        }

        let before = c.clone();
        enforce_separation_dependencies(&mut c);
        let method = || crate::methods::separation_name(c.separation.algorithm);
        if turned_on(&before, &c, |c| c.pipeline.do_r2primemap) {
            out.push(note("pipeline.do_r2primemap", "pipeline.do_chi_separation",
                format!("The R2′ map is computed: χ-separation by {} needs it", method())));
        }
        if turned_on(&before, &c, |c| c.pipeline.do_r2starmap) {
            out.push(if c.pipeline.do_chi_separation && c.separation.algorithm == SeparationAlgorithm::R2starQsm {
                note("pipeline.do_r2starmap", "pipeline.do_chi_separation",
                    format!("The R2* map is computed: χ-separation by {} needs it", method()))
            } else {
                note("pipeline.do_r2starmap", "pipeline.do_r2primemap",
                    "The R2* map is computed: R2′ is computed from it")
            });
        }
        if turned_on(&before, &c, |c| c.pipeline.do_r2map) {
            out.push(note("pipeline.do_r2map", "pipeline.do_r2primemap",
                "The R2 map is computed: R2′ is measured as R2* − R2 where it can be"));
        }

        let before = c.clone();
        enforce_reference_dependencies(&mut c);
        if before.qsm.reference == QsmReference::Region && c.qsm.reference != QsmReference::Region {
            out.push(note("qsm.reference", "qsm.reference_region",
                "QSM is referenced to its mean: the region reference names no region"));
        }
        if turned_on(&before, &c, |c| c.pipeline.do_segmentation) {
            out.push(note("pipeline.do_segmentation", "qsm.reference",
                "Segmentation is on: the reference region is measured on it"));
        }
        (c, out)
    }
}

fn turned_on(before: &PipelineConfig, after: &PipelineConfig, get: impl Fn(&PipelineConfig) -> bool) -> bool {
    !get(before) && get(after)
}

fn note(setting: &'static str, because: &'static str, message: impl Into<String>) -> Adjustment {
    Adjustment { setting, because, message: message.into() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::R2PrimeStrategy;

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
                ("qsm.reference", c.qsm.reference != n.qsm.reference),
            ].into_iter().filter(|(_, d)| *d).map(|(s, _)| s).collect();
            let mut said: Vec<&str> = adjustments.iter().map(|a| a.setting).collect();
            said.sort();
            let mut changed = changed;
            changed.sort();
            assert_eq!(said, changed, "{:?}", c.pipeline);
            for a in &adjustments {
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
        c.qsm.reference_region = Some(" ".into());
        assert_eq!(because(&c), [("qsm.reference", "qsm.reference_region")]);
        assert_eq!(c.normalised().0.qsm.reference, QsmReference::Mean);

        assert!(PipelineConfig::default().normalised().1.is_empty());
    }
}
