use super::*;

/// Per-structure statistics need a parcellation to average over and at least one map to average,
/// so asking for the analysis forces both. A bring-your-own dseg satisfies the parcellation.
///
/// The map side is satisfied by *any* quantitative map, not χ specifically: the table summarises
/// everything the run produced, so `--no-qsm --do-r2starmap --do-analysis` is a perfectly good
/// request and must not have QSM forced back on behind the user's back. Reconstruction is only
/// implied when nothing else would produce a map at all — the difference between filling in an
/// obvious omission and overriding a stated choice.
pub fn enforce_analysis_dependencies(config: &mut PipelineConfig) {
    if config.pipeline.do_analysis {
        if config.segmentation.custom_dseg_tool.is_none() {
            config.pipeline.do_segmentation = true;
        }
        if !produces_a_quantitative_map(config) {
            config.pipeline.do_qsm = true;
        }
    }
}

/// Whether the run will produce anything the per-structure table can summarise.
///
/// Chi-separation counts because it writes χ+/χ−, and a custom Chimap counts because the analysis
/// reads it from derivatives. The custom R2/R2' tools do not count on their own: those maps are
/// only pulled in when their own toggle asks for them.
fn produces_a_quantitative_map(config: &PipelineConfig) -> bool {
    config.pipeline.do_qsm
        || config.pipeline.do_t2starmap
        || config.pipeline.do_r2starmap
        || config.pipeline.do_r2map
        || config.pipeline.do_r2primemap
        || config.pipeline.do_chi_separation
        || config.separation.custom_qsm_tool.is_some()
}

/// A region reference is measured on the parcellation, so asking for one asks for a segmentation.
///
/// A required change, not an error: a front end offers it as one when the reference is chosen
/// ([`crate::normalise::required_findings`]), and a run turns it on itself and says so, which
/// keeps `--qsm-reference thalamus` a complete instruction. A bring-your-own dseg satisfies it the
/// same way it satisfies the statistics.
///
/// A `Region` with nothing to resolve is a configuration error ([`crate::requirements`]), not
/// rewritten here.
pub fn enforce_reference_dependencies(config: &mut PipelineConfig) {
    if config.qsm.reference != QsmReference::Region {
        return;
    }
    if config.qsm.reference_region.as_ref().is_none_or(|r| r.trim().is_empty()) {
        return;
    }
    if config.pipeline.do_qsm && config.segmentation.custom_dseg_tool.is_none() {
        config.pipeline.do_segmentation = true;
    }
}

/// SMWI weights the magnitude by the susceptibility map, so it cannot run without one.
///
/// Turning it on forces QSM, the same way chi-separation forces the relaxometry maps it consumes.
/// A custom QSM from derivatives counts: that is a susceptibility map too.
pub fn enforce_smwi_dependencies(config: &mut PipelineConfig) {
    if config.pipeline.do_smwi && config.separation.custom_qsm_tool.is_none() {
        config.pipeline.do_qsm = true;
    }
}

/// χ-separation by χ-sepnet's toolbox route that may estimate its own R2′: with no R2′ supplied
/// and R2′ not restricted to being measured. The route runs R2PRIMEnet on R2* itself, in the
/// network's training frame, unless it is given an R2′ measured as R2* − R2. So it needs R2*, and
/// a run's R2′ map only where this scan can measure one
/// ([`crate::requirements::measures_r2prime_for_chisepnet`]); otherwise the map is the user's
/// output to ask for, and is not what the route uses.
pub fn chisepnet_route_can_estimate_r2prime(config: &PipelineConfig) -> bool {
    let s = &config.separation;
    config.pipeline.do_chi_separation
        && s.algorithm == SeparationAlgorithm::ChiSepNet
        && s.chi_sepnet.route == ChiSepNetRoute::Toolbox
        && s.custom_r2prime_tool.is_none()
        && s.r2prime_strategy != R2PrimeStrategy::Mese
}

/// Force the relaxometry outputs the *selected* chi-separation method depends on.
///
/// Requirements are method-specific: `r2star-qsm` needs R2*; `decompose` needs neither; the
/// R2'-based methods (chi-sep-ilsqr/medi, wavesep, hc-chisep) need R2' (= R2* − R2), which in turn
/// needs R2 and R2* unless a custom R2'/R2 map is supplied. Enabling chi-separation turns on exactly
/// those maps. Keeps a `--do-chisep` run self-consistent, and is what the TUI validates against (it
/// must not let the user disable a map the current method requires).
pub fn enforce_separation_dependencies(config: &mut PipelineConfig) {
    if chisepnet_route_can_estimate_r2prime(config) {
        config.pipeline.do_r2starmap = true;
    } else if config.pipeline.do_chi_separation {
        let algorithm = config.separation.algorithm;
        if algorithm == SeparationAlgorithm::R2starQsm {
            config.pipeline.do_r2starmap = true;
        } else if algorithm.reads_r2prime() && config.separation.custom_r2prime_tool.is_none() {
            config.pipeline.do_r2primemap = true;
        }
    }
    // Computing R2' (no custom map) always needs R2*: it is the minuend of R2* − R2, and it is
    // also R2PRIMEnet's only input. R2 is needed only on the routes that can measure R2' —
    // predicting it from R2* does not subtract anything.
    if config.pipeline.do_r2primemap && config.separation.custom_r2prime_tool.is_none() {
        config.pipeline.do_r2starmap = true;
        let may_measure = config.separation.r2prime_strategy != R2PrimeStrategy::R2primenet;
        if may_measure && config.separation.custom_r2_tool.is_none() {
            config.pipeline.do_r2map = true;
        }
    }
}
