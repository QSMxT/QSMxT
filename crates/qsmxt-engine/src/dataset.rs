//! What a run, and a dataset's runs together, did: the methods text and the pipeline graph made
//! from their run records ([`crate::record`]), and which runs differ from the dataset.
//!
//! A front end writes these where it keeps a run's and a dataset's files; `qsmxt run` writes a
//! run's in its workflow directory, the dataset's beside `methods.md`, and a run that differs from
//! the dataset in `changes/`.

use std::collections::BTreeSet;

use crate::config::PipelineConfig;
use crate::methods::{methods_for_dataset, methods_for_run, Methods};
use crate::pipeline_graph::{
    pipeline_graph_after, pipeline_graph_dataset, pipeline_graph_on, PipelineGraph,
};
use crate::plan::Plan;
use crate::record::{merge, DatasetRecord, RunRecord, StepState};
use crate::scan::ScanFacts;

/// One run, as it ran.
#[derive(Debug, Clone)]
pub struct RunSummary {
    /// Its methods text ([`methods_for_run`]).
    pub methods: Methods,
    /// The graph of its plan, on the grid the run took ([`pipeline_graph_on`]).
    pub planned: PipelineGraph,
    /// The graph as it ran ([`pipeline_graph_after`]).
    pub after: PipelineGraph,
    /// The run went otherwise than planned ([`PipelineGraph::differs_from`]).
    pub differs: bool,
}

/// What a run of `plan` (from `config` and the scan's `facts`) that did what `record` says did.
pub fn summarise_run(
    config: &PipelineConfig,
    plan: &Plan,
    facts: &ScanFacts,
    record: &RunRecord,
    tool: &str,
) -> RunSummary {
    let planned = pipeline_graph_on(plan, config, facts, record.resampled);
    let after = pipeline_graph_after(plan, config, facts, record);
    let differs = after.differs_from(&planned);
    RunSummary {
        methods: methods_for_run(config, plan, record, tool),
        planned,
        after,
        differs,
    }
}

/// A dataset's runs, as they ran.
#[derive(Debug, Clone)]
pub struct DatasetSummary {
    /// The runs' records, merged.
    pub record: DatasetRecord,
    /// The dataset's methods text ([`methods_for_dataset`]).
    pub methods: Methods,
    /// The dataset's graph as its runs took it ([`pipeline_graph_dataset`]).
    pub graph: PipelineGraph,
    /// Each run, in the order given.
    pub runs: Vec<RunSummary>,
    /// The runs (by their place in `runs`) that differ from the dataset ([`summarise`]).
    pub changed: Vec<usize>,
}

/// What `runs` (each a run's plan, its scan's facts and its record) did together, under `config`.
///
/// A run differs from the dataset when it is an exception among its runs:
///
/// - its methods text is not the one most runs have (on a tie, the dataset's, else the first
///   run's), or
/// - its graph as it ran, with the steps it skipped, is not the one most runs with the same
///   planned graph have (on a tie, the planned one).
///
/// A choice every run made is the dataset's (a supplied mask no run could use, say), so no run
/// differs for it, and the dataset's text, which covers every run ("where available, …"), is not
/// held against the runs that made the usual choice. `None` for no runs.
pub fn summarise(
    config: &PipelineConfig,
    runs: &[(&Plan, &ScanFacts, &RunRecord)],
    tool: &str,
) -> Option<DatasetSummary> {
    if runs.is_empty() {
        return None;
    }
    let summaries: Vec<RunSummary> = runs
        .iter()
        .map(|(plan, facts, record)| summarise_run(config, plan, facts, record, tool))
        .collect();
    let records: Vec<RunRecord> = runs.iter().map(|(_, _, r)| (*r).clone()).collect();
    let record = merge(&records);
    let methods = methods_for_dataset(config, &record, tool);
    let afters: Vec<PipelineGraph> = summaries.iter().map(|s| s.after.clone()).collect();
    let graph = pipeline_graph_dataset(&afters, &record)?;

    // What each run's graph looked like as it ran, and the one most runs of the same plan took.
    type Shape = (PipelineGraph, BTreeSet<String>);
    let shape = |g: &PipelineGraph| -> Shape {
        let skipped = g
            .nodes
            .iter()
            .filter(|n| {
                n.run
                    .as_ref()
                    .is_some_and(|r| r.state == StepState::Skipped)
            })
            .map(|n| n.id.clone())
            .collect();
        (g.without_runs(), skipped)
    };
    let shapes: Vec<Shape> = summaries.iter().map(|s| shape(&s.after)).collect();
    // The runs planned alike, and the shape most of them took: on a tie, the planned one, else
    // the first.
    let mut groups: Vec<(PipelineGraph, Vec<usize>)> = Vec::new();
    for (i, s) in summaries.iter().enumerate() {
        let planned = s.planned.without_runs();
        match groups.iter_mut().find(|(g, _)| *g == planned) {
            Some((_, members)) => members.push(i),
            None => groups.push((planned, vec![i])),
        }
    }
    let mut usual: Vec<usize> = vec![0; runs.len()];
    for (planned, members) in &groups {
        let mut counts: Vec<(usize, usize)> = Vec::new(); // (a run of the shape, how many)
        for &i in members {
            match counts.iter_mut().find(|(j, _)| shapes[*j] == shapes[i]) {
                Some((_, n)) => *n += 1,
                None => counts.push((i, 1)),
            }
        }
        let as_planned = |j: usize| shapes[j].0 == *planned && shapes[j].1.is_empty();
        let mut best = counts[0];
        for &c in &counts[1..] {
            if c.1 > best.1 || (c.1 == best.1 && as_planned(c.0) && !as_planned(best.0)) {
                best = c;
            }
        }
        for &i in members {
            usual[i] = best.0;
        }
    }
    // The text most runs have.
    let dataset_text = methods.to_markdown();
    let texts: Vec<String> = summaries.iter().map(|s| s.methods.to_markdown()).collect();
    let mut counts: Vec<(usize, usize)> = Vec::new(); // (a run with the text, how many)
    for (i, t) in texts.iter().enumerate() {
        match counts.iter_mut().find(|(j, _)| texts[*j] == *t) {
            Some((_, n)) => *n += 1,
            None => counts.push((i, 1)),
        }
    }
    let mut usual_text = counts[0];
    for &c in &counts[1..] {
        if c.1 > usual_text.1
            || (c.1 == usual_text.1
                && texts[c.0] == dataset_text
                && texts[usual_text.0] != dataset_text)
        {
            usual_text = c;
        }
    }
    let changed = (0..runs.len())
        .filter(|&i| texts[i] != texts[usual_text.0] || shapes[i] != shapes[usual[i]])
        .collect();
    Some(DatasetSummary {
        record,
        methods,
        graph,
        runs: summaries,
        changed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::tests::facts;
    use crate::record::{Choice, Decision, StepRecord, Topic};

    /// A record of a run of `plan` that computed every step and decided nothing, but what `decide`
    /// adds.
    fn record(plan: &Plan, decide: &[(&str, Decision)]) -> RunRecord {
        RunRecord {
            steps: plan
                .steps
                .iter()
                .map(|s| StepRecord {
                    id: s.stage.name(),
                    label: s.stage.label(),
                    state: StepState::Computed,
                    decisions: decide
                        .iter()
                        .filter(|(id, _)| *id == s.stage.name())
                        .map(|(_, d)| d.clone())
                        .collect(),
                })
                .collect(),
            resampled: Some(false),
        }
    }

    fn unusable() -> Decision {
        Decision::new(
            Topic::MaskSource,
            Choice::Computed,
            "The supplied mask could not be read, so the mask was computed.",
        )
    }

    /// Runs that decided nothing: the dataset's methods are the configuration's, its graph is
    /// each run's, with nothing to say on the boxes, and no run differs.
    #[test]
    fn runs_that_decided_nothing_are_the_configuration() {
        let config = PipelineConfig::default();
        let f = facts();
        let plan = crate::plan::plan(&config, &f);
        let r = record(&plan, &[]);
        let s = summarise(&config, &[(&plan, &f, &r), (&plan, &f, &r)], "qsmxt").unwrap();
        assert_eq!(
            s.methods.to_markdown(),
            crate::methods::generate_methods_for(&config, "qsmxt")
        );
        assert!(s.changed.is_empty());
        assert_eq!(s.graph.without_runs(), s.runs[0].after.without_runs());
        assert!(s
            .graph
            .nodes
            .iter()
            .all(|n| n.dataset.as_ref().is_some_and(|d| d.summary().is_empty())));
        assert!(!s.graph.to_mermaid().contains("<b>"));
        assert!(summarise(&config, &[], "qsmxt").is_none());
    }

    /// A supplied mask that could not be used in one of three runs: that run differs, and the
    /// dataset's mask box counts it. When no run could use it, that is the dataset: no run differs.
    #[test]
    fn a_run_that_went_otherwise_differs_and_a_choice_every_run_made_is_the_datasets() {
        let mut config = PipelineConfig::default();
        config.masking.custom_mask_tool = Some("bet".into());
        let mut f = facts();
        f.custom_mask = true;
        let plan = crate::plan::plan(&config, &f);
        let used = record(
            &plan,
            &[(
                "mask",
                Decision::new(
                    Topic::MaskSource,
                    Choice::Supplied,
                    "Taken from the supplied mask.",
                ),
            )],
        );
        let bad = record(&plan, &[("mask", unusable())]);
        let s = summarise(
            &config,
            &[(&plan, &f, &used), (&plan, &f, &bad), (&plan, &f, &used)],
            "qsmxt",
        )
        .unwrap();
        assert_eq!(s.changed, [1]);
        // The mask box: each way the runs set it up, in place of its settings and the sentences.
        let node = s.graph.node("mask").unwrap();
        let mask = node.dataset.clone().unwrap();
        let ways: Vec<String> = mask.alternatives.iter().map(|a| mask.alternative_text(a)).collect();
        assert_eq!(ways[0], "2 of 3: supplied (bet)");
        assert!(ways[1].starts_with("1 of 3: phase quality (Otsu threshold"), "{ways:?}");
        assert!(node.settings.is_empty() && mask.decisions.is_empty());
        assert!(!s.graph.to_mermaid().contains("could not be read"));

        let s = summarise(&config, &[(&plan, &f, &bad), (&plan, &f, &bad)], "qsmxt").unwrap();
        assert!(s.changed.is_empty(), "{:?}", s.changed);
        assert!(s.runs.iter().all(|r| r.differs));
        assert_eq!(s.methods.to_markdown(), s.runs[0].methods.to_markdown());
        assert!(!s.methods.to_markdown().contains("Where available"));
    }

    /// A step one run skipped: that run differs, though the methods may say the same.
    #[test]
    fn a_skipped_step_differs() {
        let mut config = PipelineConfig::default();
        config.pipeline.do_swi = true;
        let f = facts();
        let plan = crate::plan::plan(&config, &f);
        let ok = record(&plan, &[]);
        let mut skipped = ok.clone();
        let swi = skipped.steps.iter_mut().find(|s| s.id == "swi").unwrap();
        swi.state = StepState::Skipped;
        let s = summarise(
            &config,
            &[(&plan, &f, &ok), (&plan, &f, &skipped), (&plan, &f, &ok)],
            "qsmxt",
        )
        .unwrap();
        assert_eq!(s.changed, [1]);
        assert_eq!(
            s.graph
                .node("swi")
                .unwrap()
                .dataset
                .as_ref()
                .unwrap()
                .summary(),
            ["Skipped in 1 of 3 runs"]
        );
    }
}
