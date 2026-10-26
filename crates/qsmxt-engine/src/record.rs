//! What a run did: each step of its plan computed, reused from the cache or skipped, and what the
//! run decided on the way that the plan could not know (a supplied mask that could not be used and
//! was computed instead, an R2′ measured rather than predicted, the box the reconstruction ran in).
//!
//! [`crate::run::run`] returns a [`RunRecord`] for every store. A step reused from the cache
//! reports what was decided when it was computed: the store keeps a step's decisions with its
//! record ([`crate::store::Completed::decisions`]). The pipeline graph after a run shows the record
//! on its boxes ([`crate::pipeline_graph::pipeline_graph_after`]), and the methods text says what
//! was done from it ([`crate::methods::methods_for_run`]).
//!
//! A dataset's runs are merged into one [`DatasetRecord`] ([`merge`]): each step once, with how
//! many runs took it and how ("Mask: computed 318, supplied 2"). The dataset's methods text comes
//! from that ([`crate::methods::methods_for_dataset`]).

use serde::{Deserialize, Serialize};

/// What happened to a step of the plan in a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    /// Computed in this run.
    Computed,
    /// Its earlier output was still valid, so it was reused.
    Cached,
    /// It did nothing with the data in this run; its decisions say why, unless the plan already
    /// said so (motion correction that is off).
    Skipped,
}

/// What a run-time decision is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Topic {
    /// Whether the step did anything: [`Choice::Skipped`], with the reason.
    Step,
    /// The grid the run was processed on: resampled to axial, or the acquired one.
    Grid,
    /// A gap between the acquisition's slices, reconstructed across.
    SliceGap,
    /// The sign of the phase converted from real and imaginary parts.
    PhaseSign,
    /// Slice-to-slice phase alternation found in real and imaginary input.
    SliceChopping,
    /// Where the brain mask came from.
    MaskSource,
    /// Two-pass artefact reduction asked for but not applied.
    TwoPass,
    /// The box the FFT-based reconstruction ran in, from the mask.
    ReconstructionBox,
    /// Where the segmentation came from.
    SegmentationSource,
    /// Where the R2 map came from, if anywhere.
    R2Source,
    /// The part of the brain mask R2 was measured over.
    R2Coverage,
    /// Where the R2′ map came from, if anywhere.
    R2primeSource,
    /// The part of the brain mask R2′ was measured over.
    R2primeCoverage,
    /// R2PRIMEnet run off the field strength it was trained at.
    R2primenetCalibration,
    /// R2PRIMEnet run on a grid whose B0 is not along the slice normal.
    R2primenetGrid,
    /// Where the susceptibility map χ-separation separated came from.
    QsmSource,
    /// The maps the region statistics summarised.
    AnalysisMaps,
    /// An output left where it was when the others returned to the acquired grid.
    OutputGrid,
    /// Whether MEDI's CSF zero reference (MEDI+0) was applied: it needs at least three echoes.
    MediCsf,
    /// Whether motion correction was applied: it needs at least two echoes with magnitude images.
    MotionCorrection,
}

/// What was decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    /// The step did nothing ([`Topic::Step`]).
    Skipped,
    /// Resampled to axial: the acquisition was more oblique than the threshold.
    Resampled,
    /// Resampled to axial because an algorithm the run uses assumes B0 along the slice normal.
    ResampledForAlgorithm,
    /// Processed on the acquired grid.
    Acquired,
    /// Reconstructed across a slice gap ([`Topic::SliceGap`]).
    Ignored,
    /// The phase was negated ([`Topic::PhaseSign`]).
    Negated,
    /// Corrected ([`Topic::SliceChopping`]).
    Corrected,
    /// Taken from what was supplied with the scan.
    Supplied,
    /// Computed by the run.
    Computed,
    /// Measured (R2 by the EPG fit of a MESE, R2′ as R2* − R2).
    Measured,
    /// Predicted by a network (R2′ by R2PRIMEnet).
    Predicted,
    /// Not made: nothing to make it from.
    Unavailable,
    /// Over part of the brain mask only ([`Topic::R2Coverage`], [`Topic::R2primeCoverage`]).
    Partial,
    /// Uncalibrated ([`Topic::R2primenetCalibration`]).
    Uncalibrated,
    /// Off the network's training distribution ([`Topic::R2primenetGrid`]).
    OffDistribution,
    /// Not applied ([`Topic::TwoPass`]).
    NotApplied,
    /// A box around the mask ([`Topic::ReconstructionBox`]).
    Cropped,
    /// The grid zero-padded to an FFT-efficient size ([`Topic::ReconstructionBox`]).
    Padded,
    /// The whole grid: the box would not have changed it ([`Topic::ReconstructionBox`]).
    Full,
    /// Kept on its own grid ([`Topic::OutputGrid`]).
    Kept,
    /// Summarised ([`Topic::AnalysisMaps`]).
    Summarised,
    /// Applied ([`Topic::MediCsf`], [`Topic::MotionCorrection`]).
    Applied,
}

/// Something a run decided while it ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub topic: Topic,
    pub choice: Choice,
    /// What was done and why, in plain words: what the step's box says after the run. The same
    /// words for every run the same thing happened to, so a dataset's runs can be counted by them.
    pub reason: String,
    /// What this run's data said, where there is more to say (a file, a share of the mask, a box's
    /// size).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Decision {
    pub fn new(topic: Topic, choice: Choice, reason: impl Into<String>) -> Self {
        Decision {
            topic,
            choice,
            reason: reason.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// The reason, and the detail after it.
    pub fn note(&self) -> String {
        match &self.detail {
            Some(d) => format!("{} ({d})", self.reason),
            None => self.reason.clone(),
        }
    }
}

/// One step of the plan, as the run took it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepRecord {
    /// The step's name ([`crate::plan::Stage::name`]): the id of its box in the pipeline graph.
    pub id: String,
    /// What a front end calls it ([`crate::plan::Stage::label`]).
    pub label: String,
    pub state: StepState,
    /// What was decided in it, in the order it was decided.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<Decision>,
}

impl StepRecord {
    /// The decision about `topic`, if one was made.
    pub fn decision(&self, topic: Topic) -> Option<&Decision> {
        self.decisions.iter().find(|d| d.topic == topic)
    }
}

/// What a run did, step by step, in the order of its plan.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RunRecord {
    pub steps: Vec<StepRecord>,
    /// The run was resampled to axial, so its outputs were returned to the acquired grid (or left
    /// on the axial one); `None` before the load step has run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resampled: Option<bool>,
    /// What the run's config departs from in its methods' reference pipelines, on its scan
    /// ([`crate::recommendations::recorded`]), and what became of each finding: applied before
    /// the run, dismissed, or kept.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recommendations: Vec<crate::recommendations::Recorded>,
}

impl RunRecord {
    pub fn step(&self, id: &str) -> Option<&StepRecord> {
        self.steps.iter().find(|s| s.id == id)
    }

    /// Every decision of every step, with the step it was made in.
    pub fn decisions(&self) -> impl Iterator<Item = (&StepRecord, &Decision)> {
        self.steps
            .iter()
            .flat_map(|s| s.decisions.iter().map(move |d| (s, d)))
    }

    /// Whether anything was decided at run time.
    pub fn decided(&self) -> bool {
        self.decisions().next().is_some()
    }
}

/// How many of a dataset's runs made a decision, and how.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionCount {
    pub topic: Topic,
    pub choice: Choice,
    pub reason: String,
    /// The runs that made it.
    pub runs: usize,
}

/// One step across a dataset's runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetStep {
    pub id: String,
    pub label: String,
    /// The runs whose plan had the step.
    pub runs: usize,
    pub computed: usize,
    pub cached: usize,
    pub skipped: usize,
    /// Each decision once per topic, choice and reason, with how many runs made it, in the order
    /// first made.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<DecisionCount>,
}

impl DatasetStep {
    /// The runs that did something in the step: computed it, or reused what an earlier run of it
    /// computed.
    pub fn ran(&self) -> usize {
        self.computed + self.cached
    }

    /// How the runs that decided about `topic` split between its choices, each choice once, in
    /// the order first made: "Mask: computed 318, supplied 2" is `[(Computed, 318), (Supplied, 2)]`.
    pub fn split(&self, topic: Topic) -> Vec<(Choice, usize)> {
        let mut out: Vec<(Choice, usize)> = Vec::new();
        for d in self.decisions.iter().filter(|d| d.topic == topic) {
            match out.iter_mut().find(|(c, _)| *c == d.choice) {
                Some((_, n)) => *n += d.runs,
                None => out.push((d.choice, d.runs)),
            }
        }
        out
    }
}

/// Every run of a dataset, merged.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DatasetRecord {
    /// The runs merged.
    pub runs: usize,
    /// Every step any run's plan had, each once: in plan order, a step only some runs took
    /// placed after the step it followed in the first run that took it.
    pub steps: Vec<DatasetStep>,
    /// The runs resampled to axial.
    pub resampled: usize,
    /// Every finding any run kept ([`RunRecord::recommendations`]), once, with how many runs kept
    /// it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recommendations: Vec<RecommendationCount>,
}

/// A finding of a dataset's runs, and how many runs it was found in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecommendationCount {
    #[serde(flatten)]
    pub recorded: crate::recommendations::Recorded,
    pub runs: usize,
}

impl DatasetRecord {
    pub fn step(&self, id: &str) -> Option<&DatasetStep> {
        self.steps.iter().find(|s| s.id == id)
    }

    /// The choices made about `topic` in any step, with how many runs made each.
    pub fn split(&self, topic: Topic) -> Vec<(Choice, usize)> {
        let mut out: Vec<(Choice, usize)> = Vec::new();
        for (choice, n) in self.steps.iter().flat_map(|s| s.split(topic)) {
            match out.iter_mut().find(|(c, _)| *c == choice) {
                Some((_, m)) => *m += n,
                None => out.push((choice, n)),
            }
        }
        out
    }

    /// The runs that chose `choice` about `topic`.
    pub fn count(&self, topic: Topic, choice: Choice) -> usize {
        self.split(topic)
            .into_iter()
            .filter(|(c, _)| *c == choice)
            .map(|(_, n)| n)
            .sum()
    }

    /// Whether any run decided about `topic`, and every run that did chose one of `choices`.
    pub fn only(&self, topic: Topic, choices: &[Choice]) -> bool {
        let split = self.split(topic);
        !split.is_empty() && split.iter().all(|(c, _)| choices.contains(c))
    }

    /// Whether the plan of some run had step `id` and no run did anything in it.
    pub fn none_ran(&self, id: &str) -> bool {
        self.step(id).is_some_and(|s| s.ran() == 0)
    }
}

impl From<&RunRecord> for DatasetRecord {
    fn from(record: &RunRecord) -> Self {
        merge(std::slice::from_ref(record))
    }
}

/// Merge a dataset's runs: each step once, counting the runs that took it, computed, reused or
/// skipped it, and that made each decision.
pub fn merge(records: &[RunRecord]) -> DatasetRecord {
    let mut out = DatasetRecord {
        runs: records.len(),
        ..Default::default()
    };
    for record in records {
        if record.resampled == Some(true) {
            out.resampled += 1;
        }
        for r in &record.recommendations {
            match out.recommendations.iter_mut().find(|c| c.recorded == *r) {
                Some(c) => c.runs += 1,
                None => out.recommendations.push(RecommendationCount { recorded: r.clone(), runs: 1 }),
            }
        }
        // Where the next step of this run not merged yet goes: after the last one it shares.
        let mut at = 0;
        for step in &record.steps {
            let i = match out.steps.iter().position(|s| s.id == step.id) {
                Some(i) => i,
                None => {
                    out.steps.insert(
                        at,
                        DatasetStep {
                            id: step.id.clone(),
                            label: step.label.clone(),
                            runs: 0,
                            computed: 0,
                            cached: 0,
                            skipped: 0,
                            decisions: Vec::new(),
                        },
                    );
                    at
                }
            };
            at = i + 1;
            let merged = &mut out.steps[i];
            merged.runs += 1;
            match step.state {
                StepState::Computed => merged.computed += 1,
                StepState::Cached => merged.cached += 1,
                StepState::Skipped => merged.skipped += 1,
            }
            for d in &step.decisions {
                match merged
                    .decisions
                    .iter_mut()
                    .find(|c| c.topic == d.topic && c.choice == d.choice && c.reason == d.reason)
                {
                    Some(c) => c.runs += 1,
                    None => merged.decisions.push(DecisionCount {
                        topic: d.topic,
                        choice: d.choice,
                        reason: d.reason.clone(),
                        runs: 1,
                    }),
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(id: &str, state: StepState, decisions: Vec<Decision>) -> StepRecord {
        StepRecord {
            id: id.into(),
            label: id.into(),
            state,
            decisions,
        }
    }

    fn supplied() -> Decision {
        Decision::new(
            Topic::MaskSource,
            Choice::Supplied,
            "Taken from the supplied mask.",
        )
    }

    fn computed() -> Decision {
        Decision::new(
            Topic::MaskSource,
            Choice::Computed,
            "No supplied mask was found, so the mask was computed.",
        )
    }

    /// The record serialises as the bindings pass it on: snake_case states and decisions, no
    /// empty fields.
    #[test]
    fn a_record_serialises_plainly() {
        let record = RunRecord {
            steps: vec![
                step("load", StepState::Cached, vec![]),
                step(
                    "mask",
                    StepState::Computed,
                    vec![computed().with_detail("derivatives/bet")],
                ),
            ],
            resampled: Some(false),
            recommendations: vec![],
        };
        let json = serde_json::to_value(&record).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "steps": [
                    { "id": "load", "label": "load", "state": "cached" },
                    { "id": "mask", "label": "mask", "state": "computed", "decisions": [{
                        "topic": "mask_source", "choice": "computed",
                        "reason": "No supplied mask was found, so the mask was computed.",
                        "detail": "derivatives/bet",
                    }] },
                ],
                "resampled": false,
            })
        );
        assert_eq!(serde_json::from_value::<RunRecord>(json).unwrap(), record);
        assert!(record.decided());
        assert_eq!(
            record.step("mask").unwrap().decisions[0].note(),
            "No supplied mask was found, so the mask was computed. (derivatives/bet)"
        );
    }

    /// Merged, each step is counted once per run, a step only some runs took keeps its place, and
    /// the runs that decided differently are split.
    #[test]
    fn merging_counts_runs_steps_and_splits() {
        let a = RunRecord {
            steps: vec![
                step("load", StepState::Computed, vec![]),
                step("mask", StepState::Computed, vec![computed()]),
                step("invert", StepState::Computed, vec![]),
            ],
            resampled: Some(true),
            recommendations: vec![],
        };
        let b = RunRecord {
            steps: vec![
                step("load", StepState::Cached, vec![]),
                step("mask", StepState::Computed, vec![supplied()]),
                step("r2_r2prime", StepState::Skipped, vec![]),
                step("invert", StepState::Cached, vec![]),
            ],
            resampled: Some(false),
            recommendations: vec![],
        };
        let m = merge(&[a.clone(), a, b]);
        assert_eq!(m.runs, 3);
        assert_eq!(m.resampled, 2);
        let ids: Vec<&str> = m.steps.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["load", "mask", "r2_r2prime", "invert"]);
        let mask = m.step("mask").unwrap();
        assert_eq!((mask.runs, mask.computed, mask.ran()), (3, 3, 3));
        assert_eq!(
            mask.split(Topic::MaskSource),
            [(Choice::Computed, 2), (Choice::Supplied, 1)]
        );
        assert_eq!(m.count(Topic::MaskSource, Choice::Supplied), 1);
        assert!(!m.only(Topic::MaskSource, &[Choice::Supplied]));
        assert!(m.only(Topic::MaskSource, &[Choice::Supplied, Choice::Computed]));
        let r2 = m.step("r2_r2prime").unwrap();
        assert_eq!((r2.runs, r2.skipped), (1, 1));
        assert!(m.none_ran("r2_r2prime") && !m.none_ran("invert") && !m.none_ran("swi"));
        let invert = m.step("invert").unwrap();
        assert_eq!((invert.computed, invert.cached), (2, 1));
    }
}
