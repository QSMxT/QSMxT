//! The pipeline a run will take, as a graph: one box per step (its label, the algorithm it runs
//! and that algorithm's settings), and an arrow wherever one step reads what another wrote.
//!
//! [`pipeline_graph`] reads only the [`Plan`], the configuration and the scan's facts, so a front
//! end can draw it before anything runs, and since the plan is what `run` takes, the picture
//! cannot disagree with what runs. qsmbly draws it from [`PipelineGraph`]'s JSON; `qsmxt` writes
//! it as Mermaid ([`PipelineGraph::to_mermaid`]).
//!
//! The arrows are data flow: an item a step reads, from the step that last wrote it before it.
//! The load step stands for the scan: the acquired echoes and anything supplied with the scan (a
//! brain mask, a segmentation, a MESE) enter the pipeline there. Which steps a change makes stale
//! is a wider relation ([`crate::graph::downstream_of`]: every step reads the grid the load step
//! decides, say); every arrow here is also an edge of that one.

use serde::{Deserialize, Serialize};

use crate::bridge::skips_bgremove;
use crate::config::PipelineConfig;
use crate::enums::*;
use crate::items::ItemKind;
use crate::methods::words;
use crate::plan::{Pass, Plan, Stage};
use crate::requirements::{self, Output};
use crate::scan::{RunMetadata, ScanFacts};
use crate::store::{Contrast, Item};

/// Which reconstruction of a two-pass run a step belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Branch {
    /// The reconstruction on the brain mask.
    Main,
    /// The reconstruction on the reliable-phase mask.
    Reliable,
}

/// One step of the run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// The step's name, as the observer and the cache name it (`mask`, `invert-reliable`).
    pub id: String,
    /// What a front end calls it ("Background-field removal").
    pub label: String,
    /// The algorithm the step runs, as the observer reports it (`vsharp`, `rts`), when it has one.
    pub algorithm: Option<String>,
    /// The algorithm's name as the methods text gives it ("V-SHARP", "RTS").
    pub algorithm_label: Option<String>,
    /// The algorithm's settings worth showing beside it, in the methods text's words: what
    /// differs from the defaults, and what names the variant that ran (V-SHARP's radii).
    pub settings: Vec<String>,
    /// For a two-pass run's per-pass steps, which pass; none for a step both passes share.
    pub branch: Option<Branch>,
    /// The setting that asked for this step (`pipeline.do_swi`, `masking.two_pass`), by its path
    /// in the configuration; none for a step every run of the enabled outputs takes.
    pub enabled_by: Option<String>,
    /// When the step does anything at all, for a step whose need depends on the scan's header,
    /// which a graph drawn from the scan's facts alone does not know.
    pub only_when: Option<String>,
    /// The deep-learning models it runs, by QSM.rs model id.
    pub models: Vec<String>,
    /// The items it reads, by [`item_key`].
    pub reads: Vec<String>,
    /// The items it writes, by [`item_key`].
    pub writes: Vec<String>,
}

/// One step reading what another wrote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    /// What flows along it, by [`item_key`].
    pub items: Vec<String>,
}

/// An item of the graph, and what it is ([`Item::meta`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemInfo {
    /// [`item_key`].
    pub key: String,
    /// The item as the observer reports it (its serialised [`Item`]).
    pub item: serde_json::Value,
    pub label: String,
    pub units: Option<String>,
    pub kind: ItemKind,
    pub bids_name: Option<String>,
    pub published: bool,
    pub echo: Option<usize>,
    pub coil: Option<usize>,
}

/// A planned run as a graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipelineGraph {
    /// Every step that does something with the data, in the order the run takes them.
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Every item a step reads or writes, in the order they first appear.
    pub items: Vec<ItemInfo>,
    /// Two-pass artefact reduction runs: the steps with a [`Branch`] are its two
    /// reconstructions, joining at the two-pass combination.
    pub two_pass: bool,
}

/// A stable name for an item, used in the graph's JSON: its variant and arguments, as the
/// in-memory store also names it (`Phase(2)`, `LocalField(Reliable)`).
pub fn item_key(item: &Item) -> String {
    format!("{item:?}")
}

/// What the graph is drawn from.
struct Ctx<'a> {
    plan: &'a Plan,
    config: &'a PipelineConfig,
    facts: &'a ScanFacts,
}

impl Ctx<'_> {
    fn n(&self) -> usize {
        self.facts.n_echoes
    }

    fn echoes(&self, item: fn(usize) -> Item) -> Vec<Item> {
        (1..=self.n()).map(item).collect()
    }

    /// Every echo's magnitude on the working grid, when the scan has magnitude.
    fn magnitudes(&self) -> Vec<Item> {
        if self.facts.has_magnitude { self.echoes(Item::Magnitude) } else { Vec::new() }
    }

    fn mask_item(&self) -> Item {
        if self.plan.motion_corrected { Item::CoverageMask } else { Item::BrainMask }
    }

    fn pass_mask(&self, pass: Pass) -> Item {
        match pass {
            Pass::Main => self.mask_item(),
            Pass::Reliable => Item::ReliableMask,
        }
    }

    fn support(&self, pass: Pass) -> Item {
        if skips_bgremove(self.config) { self.pass_mask(pass) } else { Item::LocalFieldMask(pass) }
    }

    fn echo_phases(&self) -> Vec<Item> {
        if self.plan.motion_corrected { self.echoes(Item::CorrectedPhase) } else { self.echoes(Item::Phase) }
    }

    fn echo_magnitudes(&self) -> Vec<Item> {
        match (self.facts.has_magnitude, self.plan.motion_corrected) {
            (false, _) => Vec::new(),
            (true, true) => self.echoes(Item::CorrectedMagnitude),
            (true, false) => self.echoes(Item::Magnitude),
        }
    }

    fn supplied_mask(&self) -> bool {
        self.config.masking.custom_mask_tool.is_some() && self.facts.custom_mask
    }

    fn supplied_dseg(&self) -> bool {
        self.config.segmentation.custom_dseg_tool.is_some() && self.facts.custom_dseg
    }

    /// The segmentation a region is measured on: the run's own, or the one supplied.
    fn segmentation(&self) -> Option<Item> {
        if self.plan.has(Stage::Segmentation) {
            Some(Item::Segmentation)
        } else if self.facts.custom_dseg && self.config.segmentation.custom_dseg_tool.is_some() {
            Some(Item::SuppliedSegmentation)
        } else {
            None
        }
    }

    fn mese(&self) -> Vec<Item> {
        let k = self.facts.mese_echo_times.as_ref().map_or(0, Vec::len);
        (1..=k).map(Item::MeseMagnitude).collect()
    }

    /// The items `stage` reads and writes, as the run reads and writes them.
    fn io(&self, stage: Stage, written: &[Item]) -> (Vec<Item>, Vec<Item>) {
        let (c, f) = (self.config, self.facts);
        let has = |i: &Item| written.contains(i);
        let mut reads = Vec::new();
        let mut writes = Vec::new();
        match stage {
            Stage::Load => {
                if f.n_coils > 1 {
                    for coil in 1..=f.n_coils {
                        for e in 1..=self.n() {
                            writes.extend([Item::CoilPhase(coil, e), Item::CoilMagnitude(coil, e)]);
                        }
                    }
                } else {
                    writes.extend(self.echoes(Item::AcquiredPhase));
                    if f.real_imaginary {
                        writes.extend(self.echoes(Item::AcquiredImaginary));
                    } else if f.has_magnitude {
                        writes.extend(self.echoes(Item::AcquiredMagnitude));
                    }
                }
                let s = &c.separation;
                for (supplied, item) in [
                    (self.supplied_mask(), Item::SuppliedMask),
                    (self.supplied_dseg(), Item::SuppliedSegmentation),
                    (s.custom_r2_tool.is_some() && f.custom_r2, Item::SuppliedR2),
                    (s.custom_r2prime_tool.is_some() && f.custom_r2prime, Item::SuppliedR2prime),
                    (s.custom_qsm_tool.is_some() && f.custom_qsm, Item::SuppliedChi),
                ] {
                    if supplied {
                        writes.push(item);
                    }
                }
                writes.extend(self.mese());
            }
            Stage::ScalePhase => {
                if f.n_coils > 1 {
                    reads.extend(written.iter().copied().filter(|i| matches!(i, Item::CoilPhase(..) | Item::CoilMagnitude(..))));
                    writes.extend(self.echoes(Item::Phase));
                    writes.extend(self.echoes(Item::Magnitude));
                    writes.extend(self.echoes(Item::CombinedEchoPhase));
                    writes.extend(self.echoes(Item::CombinedEchoMagnitude));
                    writes.push(Item::CoilCombinationMask);
                } else {
                    reads.extend(self.echoes(Item::AcquiredPhase));
                    if f.real_imaginary {
                        reads.extend(self.echoes(Item::AcquiredImaginary));
                        writes.extend(self.echoes(Item::Phase));
                        writes.extend(self.echoes(Item::Magnitude));
                    } else {
                        if f.has_magnitude {
                            reads.extend(self.echoes(Item::AcquiredMagnitude));
                        }
                        writes.extend(self.echoes(Item::Phase));
                        writes.extend(self.magnitudes());
                    }
                }
            }
            Stage::Magnitude => {
                reads.extend(self.magnitudes());
                writes.push(Item::CombinedMagnitude);
            }
            Stage::Mask(Pass::Main) => {
                if self.supplied_mask() {
                    reads.push(Item::SuppliedMask);
                } else {
                    reads.extend(self.echoes(Item::Phase));
                    reads.extend(self.magnitudes());
                }
                writes.push(Item::BrainMask);
            }
            Stage::Mask(Pass::Reliable) => {
                reads.extend(self.echoes(Item::Phase));
                reads.extend(self.magnitudes());
                reads.push(self.mask_item());
                writes.push(Item::ReliableMask);
            }
            Stage::Motion => {
                reads.extend(self.echoes(Item::Phase));
                reads.extend(self.magnitudes());
                reads.push(Item::BrainMask);
                writes.extend(self.echoes(Item::CorrectedPhase));
                writes.extend(self.echoes(Item::CorrectedMagnitude));
                writes.extend([Item::CoverageMask, Item::MotionReport]);
            }
            Stage::FieldMap => {
                reads.extend(self.echo_phases());
                reads.extend(self.echo_magnitudes());
                reads.push(self.mask_item());
                writes.extend([Item::FieldPpm, Item::TotalFieldHz]);
                if f.has_magnitude {
                    writes.push(Item::TotalFieldMagnitude);
                }
                if c.field_mapping.b0_estimation == B0Estimation::RobustFit && self.n() > 1 {
                    writes.push(Item::RobustFitReport);
                }
            }
            Stage::BackgroundRemoval(p) => {
                if c.bg_removal.algorithm == BfAlgorithm::Iqfm {
                    reads.extend(self.echo_phases());
                    reads.extend(self.echo_magnitudes());
                } else {
                    reads.push(Item::FieldPpm);
                }
                reads.push(self.pass_mask(p));
                writes.extend([Item::LocalField(p), Item::LocalFieldMask(p)]);
            }
            Stage::Inversion(p) => {
                if crate::stages::inversion::iqsm_algorithm(c).is_some() {
                    reads.extend(self.echo_phases());
                    reads.extend(self.echo_magnitudes());
                    reads.push(self.pass_mask(p));
                } else {
                    if skips_bgremove(c) {
                        reads.extend([Item::FieldPpm, self.pass_mask(p)]);
                    } else {
                        reads.extend([Item::LocalField(p), Item::LocalFieldMask(p)]);
                    }
                    // The inversions that weight their data by the combined magnitude.
                    let weighted = matches!(c.inversion.algorithm,
                        QsmAlgorithm::Medi | QsmAlgorithm::Tfi | QsmAlgorithm::AmpPe | QsmAlgorithm::Lsqr | QsmAlgorithm::Heidi);
                    if weighted && has(&Item::CombinedMagnitude) {
                        reads.push(Item::CombinedMagnitude);
                    }
                }
                writes.push(Item::RawChi(p));
            }
            Stage::Tgv(p) => {
                reads.push(self.pass_mask(p));
                if self.n() > 1 {
                    reads.push(Item::FieldPpm);
                } else {
                    reads.extend(self.echo_phases().into_iter().take(1));
                }
                writes.push(Item::RawChi(p));
            }
            Stage::Qsmart(p) => {
                reads.extend([Item::FieldPpm, self.pass_mask(p)]);
                if has(&Item::CombinedMagnitude) {
                    reads.push(Item::CombinedMagnitude);
                }
                writes.push(Item::RawChi(p));
                use crate::stages::qsmart::outputs as q;
                for name in [q::VESSEL_MASK, q::LOCAL_FIELD_STAGE1, q::CHI_STAGE1, q::LOCAL_FIELD_STAGE2, q::CHI_STAGE2] {
                    writes.push(Item::QsmartIntermediate(p, name));
                }
            }
            Stage::TwoPassCombination => {
                reads.extend([Item::RawChi(Pass::Reliable), Item::RawChi(Pass::Main), self.support(Pass::Reliable),
                              self.pass_mask(Pass::Main)]);
                writes.push(Item::CombinedRawChi);
            }
            Stage::Reference | Stage::SinglePassReference => {
                let single = stage == Stage::SinglePassReference;
                if single || !self.plan.two_pass {
                    reads.extend([Item::RawChi(Pass::Main), self.support(Pass::Main)]);
                } else {
                    reads.extend([Item::CombinedRawChi, self.support(Pass::Main), self.support(Pass::Reliable)]);
                }
                if c.qsm.reference == QsmReference::Region {
                    reads.extend(self.segmentation());
                }
                if single {
                    writes.push(Item::SinglePassChi);
                } else {
                    writes.extend([Item::Chi, Item::ChiSupport]);
                }
            }
            Stage::Swi => {
                reads.extend(self.echo_phases());
                reads.extend(self.echo_magnitudes());
                writes.extend([Item::Swi, Item::SwiMip]);
            }
            Stage::Smwi => {
                reads.extend([Item::Chi, Item::CombinedMagnitude]);
                reads.push(if has(&Item::ChiSupport) { Item::ChiSupport } else { self.mask_item() });
                for contrast in Contrast::BOTH {
                    writes.extend([Item::Smwi(contrast), Item::SmwiMip(contrast)]);
                }
            }
            Stage::Relaxometry => {
                reads.push(self.mask_item());
                reads.extend(self.echo_magnitudes());
                writes.extend([Item::R2star, Item::T2star]);
            }
            Stage::R2 => {
                let s = &c.separation;
                reads.push(self.mask_item());
                let r2 = if s.custom_r2_tool.is_some() {
                    has(&Item::SuppliedR2).then_some(Item::SuppliedR2)
                } else {
                    let mese = self.mese();
                    let usable = !mese.is_empty() && requirements::mese_problem(f).is_none();
                    reads.extend(mese);
                    usable.then_some(Item::R2)
                };
                if let Some(supplied @ Item::SuppliedR2) = r2 {
                    reads.push(supplied);
                }
                if r2.is_some() {
                    writes.push(Item::R2);
                }
                if c.pipeline.do_r2primemap {
                    if s.custom_r2prime_tool.is_some() {
                        if has(&Item::SuppliedR2prime) {
                            reads.push(Item::SuppliedR2prime);
                            writes.push(Item::R2prime);
                        }
                    } else if has(&Item::R2star) {
                        reads.push(Item::R2star);
                        let measured = r2.is_some() && s.r2prime_strategy != R2PrimeStrategy::R2primenet;
                        if measured {
                            writes.push(Item::R2primeCoverage);
                        }
                        if measured || s.r2prime_strategy != R2PrimeStrategy::Mese {
                            writes.push(Item::R2prime);
                        }
                    }
                }
            }
            Stage::ChiSeparation => {
                let chi = if c.separation.custom_qsm_tool.is_some() { Item::SuppliedChi } else { Item::Chi };
                reads.extend([self.mask_item(), chi]);
                if chi == Item::Chi && has(&Item::ChiSupport) {
                    reads.push(Item::ChiSupport);
                }
                reads.extend(self.echo_magnitudes());
                let r2prime = if c.separation.custom_r2prime_tool.is_some() { Item::SuppliedR2prime } else { Item::R2prime };
                for item in [Item::R2star, r2prime, Item::LocalField(Pass::Main)] {
                    if has(&item) {
                        reads.push(item);
                    }
                }
                reads.extend(self.mese());
                writes.extend([Item::SeparatedChi(Contrast::Paramagnetic), Item::SeparatedChi(Contrast::Diamagnetic),
                               Item::SeparatedTotalChi]);
            }
            Stage::Segmentation => {
                if self.supplied_dseg() {
                    reads.push(Item::SuppliedSegmentation);
                    writes.extend([Item::Segmentation, Item::SegmentationLabels]);
                } else {
                    reads.push(Item::CombinedMagnitude);
                    writes.extend([Item::Segmentation, Item::SegmentationLabels, Item::SegmentationVolumes]);
                }
            }
            Stage::Analysis => {
                reads.extend(self.segmentation());
                let mut maps: Vec<Item> = crate::stages::analysis::MAPS.iter().map(|(i, ..)| *i).filter(|i| has(i)).collect();
                if c.separation.custom_qsm_tool.is_some() && !maps.contains(&Item::Chi) && has(&Item::SuppliedChi) {
                    maps.insert(0, Item::SuppliedChi);
                }
                reads.extend(maps);
                if has(&Item::SegmentationVolumes) {
                    reads.push(Item::SegmentationVolumes);
                }
                writes.push(Item::RegionStats);
            }
            Stage::OutputSpace => {
                let published: Vec<Item> = crate::stages::output_space::published(self.n()).into_iter()
                    .map(|(i, _)| i).filter(|i| has(i)).collect();
                reads.extend(published.iter().copied());
                writes.extend(published);
            }
        }
        (reads, writes)
    }

    /// The algorithm a step runs, as the observer reports it, and its name and settings in the
    /// methods text's words.
    fn algorithm(&self, stage: Stage) -> (Option<String>, Option<String>, Vec<String>) {
        let c = self.config;
        let some = |id: &str, label: &str| (Some(id.to_string()), Some(label.to_string()));
        let ((id, label), settings) = match stage {
            Stage::ScalePhase if self.facts.n_coils > 1 => (some("mcpc3ds", "MCPC-3D-S"), vec![]),
            Stage::Magnitude => {
                let s = if c.masking.inhomogeneity_correction { vec!["inhomogeneity correction".to_string()] } else { vec![] };
                (some("rss", "root sum of squares"), s)
            }
            Stage::Mask(pass) => {
                let sections = match pass {
                    Pass::Main => c.masking.sections.clone(),
                    Pass::Reliable => c.masking.resolved_two_pass_sections(),
                };
                let s = if pass == Pass::Main && self.supplied_mask() {
                    vec![format!("supplied ({})", c.masking.custom_mask_tool.as_deref().unwrap_or_default())]
                } else {
                    let mut lines = words::mask_sections(&sections);
                    if pass == Pass::Main && sections.len() > 1 {
                        lines.push(format!("sections combined by {}", c.masking.combine));
                    }
                    lines
                };
                ((None, None), s)
            }
            Stage::Motion => ((None, None), words::registration_settings(c)),
            Stage::FieldMap => {
                let fm = &c.field_mapping;
                let removes_offsets = self.n() > 1 && fm.phase_offset_removal
                    && fm.unwrapping_algorithm != UnwrappingAlgorithm::Laplacian;
                let id = if removes_offsets { "phase_offset_removal".to_string() } else { fm.unwrapping_algorithm.to_string() };
                let label = match fm.unwrapping_algorithm {
                    UnwrappingAlgorithm::Romeo => "ROMEO",
                    UnwrappingAlgorithm::Laplacian => "Laplacian",
                };
                let mut s = Vec::new();
                if removes_offsets {
                    s.push("phase offsets removed".to_string());
                }
                s.extend(words::unwrapping_settings(c));
                if self.n() > 1 {
                    s.push(format!("B0 by {}", match fm.b0_estimation {
                        B0Estimation::WeightedAvg => "weighted average",
                        B0Estimation::LinearFit => "linear fit",
                        B0Estimation::RobustFit => "robust fit",
                    }));
                }
                ((Some(id), Some(label.to_string())), s)
            }
            Stage::BackgroundRemoval(_) => {
                let alg = c.bg_removal.algorithm;
                ((Some(alg.to_string()), Some(words::bg_removal(alg))), words::bg_removal_settings(c))
            }
            Stage::Inversion(_) => match crate::stages::inversion::iqsm_algorithm(c) {
                Some(id) => ((Some(id.to_string()), Some(words::inversion(c.inversion.algorithm))), vec![]),
                None => {
                    let alg = c.inversion.algorithm;
                    ((Some(alg.to_string()), Some(words::inversion(alg))), words::inversion_settings(c, alg))
                }
            },
            Stage::Tgv(_) => (some("tgv", "TGV"), words::inversion_settings(c, QsmAlgorithm::Tgv)),
            Stage::Qsmart(_) => {
                let mut s = vec![format!("{} inversion", words::inversion(c.inversion.qsmart.inversion))];
                s.extend(words::qsmart_settings(c));
                (some("qsmart", "QSMART"), s)
            }
            Stage::Reference | Stage::SinglePassReference => {
                let label = match c.qsm.reference {
                    QsmReference::Mean => "mean".to_string(),
                    QsmReference::None => "none".to_string(),
                    QsmReference::Region => format!("region: {}", c.qsm.reference_region.as_deref().unwrap_or_default()),
                };
                ((Some(c.qsm.reference.to_string()), Some(label)), vec![])
            }
            Stage::Swi => (some("clear-swi", "CLEAR-SWI"), vec![]),
            Stage::Smwi => ((Some("smwi".into()), None), vec![]),
            Stage::Relaxometry => (some("arlo", "ARLO"), vec![]),
            Stage::R2 => {
                let label = if self.plan.steps.iter().any(|s| s.stage == Stage::R2 && !s.models.is_empty()) {
                    "EPG fit; R2PRIMEnet"
                } else {
                    "EPG fit"
                };
                (some("epg", label), vec![])
            }
            Stage::ChiSeparation => {
                let alg = c.separation.algorithm;
                ((Some(alg.to_string()), Some(words::separation(alg))), words::separation_settings(c))
            }
            Stage::Segmentation => {
                let label = format!("SynthSeg {}", c.segmentation.version);
                let s = if self.supplied_dseg() {
                    vec![format!("supplied ({})", c.segmentation.custom_dseg_tool.as_deref().unwrap_or_default())]
                } else {
                    vec![]
                };
                ((Some("synthseg".into()), Some(label)), s)
            }
            Stage::Load | Stage::ScalePhase | Stage::TwoPassCombination | Stage::Analysis | Stage::OutputSpace =>
                ((None, None), vec![]),
        };
        (id, label, settings)
    }

    /// The setting that asked for `stage`, for a step that runs only because one did.
    fn enabled_by(&self, stage: Stage) -> Option<String> {
        let wanted = |o: Output| requirements::wanted(self.facts, self.config, o);
        let first = |outputs: &[Output]| outputs.iter().copied().find(|&o| wanted(o)).map(|o| o.setting().to_string());
        match stage {
            Stage::Swi => first(&[Output::Swi]),
            Stage::Smwi => first(&[Output::Smwi]),
            Stage::Segmentation => first(&[Output::Segmentation]),
            Stage::Relaxometry => first(&[Output::T2star, Output::R2star]),
            // Planned when either is asked for; the data decides which it can make.
            Stage::R2 => first(&[Output::R2, Output::R2prime])
                .or_else(|| Some(if self.config.pipeline.do_r2map { Output::R2 } else { Output::R2prime }.setting().to_string())),
            Stage::ChiSeparation => Some(Output::ChiSeparation.setting().to_string()),
            Stage::Analysis => Some(Output::Analysis.setting().to_string()),
            Stage::Motion => Some("motion_correction.enabled".into()),
            Stage::Mask(Pass::Reliable) | Stage::BackgroundRemoval(Pass::Reliable) | Stage::Inversion(Pass::Reliable)
            | Stage::Tgv(Pass::Reliable) | Stage::Qsmart(Pass::Reliable) | Stage::TwoPassCombination
            | Stage::SinglePassReference => Some("masking.two_pass".into()),
            _ => None,
        }
    }
}

/// The step's pass, for the steps two-pass runs once per pass.
fn branch(stage: Stage, two_pass: bool) -> Option<Branch> {
    let pass = match stage {
        Stage::Mask(p) | Stage::BackgroundRemoval(p) | Stage::Inversion(p) | Stage::Tgv(p) | Stage::Qsmart(p) => p,
        _ => return None,
    };
    two_pass.then_some(match pass {
        Pass::Main => Branch::Main,
        Pass::Reliable => Branch::Reliable,
    })
}

/// Whether the graph leaves out `stage`, a step of `plan` that does nothing with the data:
/// motion correction when it is off (the step only forgets an earlier run's output), and the
/// return to the acquired grid of a run that keeps its working grid or, when `resampled` is known
/// (from the run's grid), is not resampled.
pub(crate) fn left_out(plan: &Plan, config: &PipelineConfig, stage: Stage, resampled: Option<bool>) -> bool {
    match stage {
        Stage::Motion => !plan.motion_corrected,
        Stage::OutputSpace => config.pipeline.output_space != crate::OutputSpace::Acquired || resampled == Some(false),
        _ => false,
    }
}

/// The graph of a run of `plan` (made by [`crate::plan::plan`] from `config` and `facts`).
///
/// `grid` is the run's grid, when it is known ([`crate::scan::resolve_geometry`] on the first
/// echo's header): it decides whether the run is resampled, and so whether its outputs are
/// returned to the acquired grid. Without it, that step is drawn with [`Node::only_when`] set.
///
/// A step the plan lists that does nothing with the data is left out: motion correction when it
/// is off (the step only forgets an earlier run's output), and the return to the acquired grid
/// of a run that is not resampled or keeps its working grid.
pub fn pipeline_graph(plan: &Plan, config: &PipelineConfig, facts: &ScanFacts, grid: Option<&RunMetadata>) -> PipelineGraph {
    let ctx = Ctx { plan, config, facts };
    let mut nodes: Vec<Node> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();
    let mut items: Vec<Item> = Vec::new();
    // Every item written so far, with the step that last wrote it.
    let mut writer: Vec<(Item, String)> = Vec::new();
    for step in &plan.steps {
        let stage = step.stage;
        let resampled = grid.map(|g| g.source_geometry.is_some());
        if left_out(plan, config, stage, resampled) {
            continue;
        }
        let only_when = (stage == Stage::OutputSpace && resampled.is_none()).then(|| "the run is resampled".to_string());
        let written: Vec<Item> = writer.iter().map(|(i, _)| *i).collect();
        let (reads, writes) = ctx.io(stage, &written);
        let id = stage.name();
        let mut incoming: Vec<Edge> = Vec::new();
        for item in &reads {
            let Some((_, from)) = writer.iter().find(|(i, _)| i == item) else { continue };
            match incoming.iter_mut().find(|e| &e.from == from) {
                Some(e) => e.items.push(item_key(item)),
                None => incoming.push(Edge { from: from.clone(), to: id.clone(), items: vec![item_key(item)] }),
            }
        }
        edges.extend(incoming);
        for item in reads.iter().chain(&writes) {
            if !items.contains(item) {
                items.push(*item);
            }
        }
        for item in &writes {
            match writer.iter_mut().find(|(i, _)| i == item) {
                Some(slot) => slot.1 = id.clone(),
                None => writer.push((*item, id.clone())),
            }
        }
        let (algorithm, algorithm_label, settings) = ctx.algorithm(stage);
        nodes.push(Node {
            label: stage.label(),
            algorithm, algorithm_label, settings,
            branch: branch(stage, plan.two_pass),
            enabled_by: ctx.enabled_by(stage),
            only_when,
            models: step.models.iter().map(|m| m.to_string()).collect(),
            reads: reads.iter().map(item_key).collect(),
            writes: writes.iter().map(item_key).collect(),
            id,
        });
    }
    let items = items.iter().map(|item| {
        let m = item.meta();
        ItemInfo {
            key: item_key(item), item: serde_json::to_value(item).unwrap_or_default(),
            label: m.label, units: m.units.map(str::to_string), kind: m.kind, bids_name: m.bids_name,
            published: m.published, echo: m.echo, coil: m.coil,
        }
    }).collect();
    PipelineGraph { nodes, edges, items, two_pass: plan.two_pass }
}

/// What one of a plan's steps runs, reads and writes: what [`crate::plan::Step`] carries
/// beside its stage.
pub(crate) struct StepDetails {
    pub algorithm: Option<String>,
    pub algorithm_label: Option<String>,
    pub reads: Vec<Item>,
    pub writes: Vec<Item>,
}

/// [`StepDetails`] for every step of `plan`, in its order. A step that does nothing with the
/// data on this scan (motion correction when it is off) reads and writes nothing.
pub(crate) fn step_details(plan: &Plan, config: &PipelineConfig, facts: &ScanFacts) -> Vec<StepDetails> {
    let ctx = Ctx { plan, config, facts };
    let mut written: Vec<Item> = Vec::new();
    plan.steps.iter().map(|step| {
        let (algorithm, algorithm_label, _) = ctx.algorithm(step.stage);
        let (reads, writes) = if step.stage == Stage::Motion && !plan.motion_corrected {
            (Vec::new(), Vec::new())
        } else {
            ctx.io(step.stage, &written)
        };
        written.extend(writes.iter().copied().filter(|i| !written.contains(i)).collect::<Vec<_>>());
        StepDetails { algorithm, algorithm_label, reads, writes }
    }).collect()
}

/// `text` inside a Mermaid quoted label: quotes and angle brackets as entities.
fn mermaid_text(text: &str) -> String {
    text.replace('&', "#amp;").replace('"', "#quot;").replace('<', "#lt;").replace('>', "#gt;")
}

/// A step name as a Mermaid node id (no `-`, which Mermaid reads as part of an arrow).
fn mermaid_id(id: &str) -> String {
    id.replace('-', "_")
}

impl PipelineGraph {
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn item(&self, key: &str) -> Option<&ItemInfo> {
        self.items.iter().find(|i| i.key == key)
    }

    /// Whether `from` feeds `to` directly.
    pub fn has_edge(&self, from: &str, to: &str) -> bool {
        self.edges.iter().any(|e| e.from == from && e.to == to)
    }

    /// The graph as a Mermaid flowchart: one box per step (label, algorithm, settings), each
    /// arrow labelled with what flows along it (an echo's items once, for all echoes). A two-pass
    /// run's passes are subgraphs; a step a setting asked for is dashed, and one that may do
    /// nothing is dotted.
    pub fn to_mermaid(&self) -> String {
        let mut out = String::from("flowchart TD\n");
        let node_line = |n: &Node| {
            let mut text = mermaid_text(&n.label);
            if let Some(alg) = &n.algorithm_label {
                text.push_str(&format!("<br/><i>{}</i>", mermaid_text(alg)));
            }
            for s in &n.settings {
                text.push_str(&format!("<br/><small>{}</small>", mermaid_text(s)));
            }
            if let Some(when) = &n.only_when {
                text.push_str(&format!("<br/><small>only when {}</small>", mermaid_text(when)));
            }
            format!("{}[\"{text}\"]", mermaid_id(&n.id))
        };
        for n in self.nodes.iter().filter(|n| n.branch.is_none()) {
            out.push_str(&format!("    {}\n", node_line(n)));
        }
        for (branch, id, title) in [(Branch::Main, "main_pass", "Main pass"), (Branch::Reliable, "reliable_pass", "Reliable pass")] {
            let members: Vec<&Node> = self.nodes.iter().filter(|n| n.branch == Some(branch)).collect();
            if members.is_empty() {
                continue;
            }
            out.push_str(&format!("    subgraph {id}[\"{title}\"]\n"));
            for n in members {
                out.push_str(&format!("        {}\n", node_line(n)));
            }
            out.push_str("    end\n");
        }
        for e in &self.edges {
            let mut labels: Vec<String> = Vec::new();
            for key in &e.items {
                let label = self.item(key).map_or_else(|| key.clone(), |i| i.label.clone());
                if !labels.contains(&label) {
                    labels.push(label);
                }
            }
            out.push_str(&format!("    {} -->|\"{}\"| {}\n", mermaid_id(&e.from), mermaid_text(&labels.join(", ")), mermaid_id(&e.to)));
        }
        let optional: Vec<String> = self.nodes.iter().filter(|n| n.enabled_by.is_some()).map(|n| mermaid_id(&n.id)).collect();
        let conditional: Vec<String> = self.nodes.iter().filter(|n| n.only_when.is_some()).map(|n| mermaid_id(&n.id)).collect();
        if !optional.is_empty() {
            out.push_str("    classDef optional stroke-dasharray: 6 4\n");
            out.push_str(&format!("    class {} optional\n", optional.join(",")));
        }
        if !conditional.is_empty() {
            out.push_str("    classDef conditional stroke-dasharray: 2 3\n");
            out.push_str(&format!("    class {} conditional\n", conditional.join(",")));
        }
        out
    }
}

#[cfg(test)]
mod tests;
