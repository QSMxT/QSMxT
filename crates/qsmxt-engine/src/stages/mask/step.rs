//! The mask steps of a run ([`crate::run`]): the brain mask and the reliable-phase mask, built
//! from the echoes in the store ([`super`]).

use log::Level;

use crate::plan::{Pass, Stage, Step};
use crate::record::{Choice, Decision, Topic};
use crate::run::{Done, Run};
use crate::stages;
use crate::store::{Data, Item, StageStore, Supplied};

impl<S: StageStore + ?Sized> Run<'_, S> {
    /// Every echo's phase, and every echo's magnitude (none for a phase-only run): what masking
    /// reads.
    pub(crate) fn echoes(&mut self) -> Result<stages::mask::Echoes, S::Error> {
        let n = self.n_echoes();
        let mut phases = Vec::with_capacity(n);
        for e in 1..=n {
            if self.store.has(&Item::Phase(e)) {
                phases.push(self.store.real(&Item::Phase(e))?);
            }
        }
        let mut magnitudes = Vec::with_capacity(n);
        for e in 1..=n {
            if !self.store.has(&Item::Magnitude(e)) {
                magnitudes.clear();
                break;
            }
            magnitudes.push(self.store.real(&Item::Magnitude(e))?);
        }
        Ok((phases, magnitudes))
    }

    /// Build a mask from `sections`, with the weights of the deep-learning `models` it runs ready
    /// first: every echo's phase, and every echo's magnitude (none for a phase-only run).
    /// Voxels where the scanner wrote no phase are left out ([`stages::mask::ScannerMasked`]), and
    /// how many is logged and, for the main mask, recorded.
    pub(crate) fn build_mask(&mut self, sections: &[crate::masking::MaskSection], combine: crate::masking::MaskCombine,
                  models: &[&'static str]) -> Result<Vec<u8>, S::Error> {
        self.build_mask_and_quality(sections, combine, models, false).map(|(mask, _)| mask)
    }

    /// [`Self::build_mask`], and with `quality` the phase-quality map masking computed
    /// ([`stages::mask::build_mask`]).
    pub(crate) fn build_mask_and_quality(&mut self, sections: &[crate::masking::MaskSection], combine: crate::masking::MaskCombine,
                              models: &[&'static str], quality: bool)
                              -> Result<(Vec<u8>, Option<stages::mask::Quality>), S::Error> {
        let (phases, mut magnitudes) = self.echoes()?;
        // The echoes the recipe reads, corrected: the inhomogeneity correction's, or corrected
        // here for a recipe it was not planned for (a supplied mask that could not be used).
        if self.config.masking.inhomogeneity_correction && !magnitudes.is_empty() {
            let n = magnitudes.len();
            for e in stages::magnitude::mask_echoes(sections, &self.config.masking.refinements, n) {
                magnitudes[e - 1] = if self.store.has(&Item::HomogeneousMagnitude(e)) {
                    self.store.real(&Item::HomogeneousMagnitude(e))?
                } else {
                    stages::magnitude::homogeneity_correct(&magnitudes[e - 1], &self.meta, self.config,
                                                           &format!("echo {e}'s magnitude"))?
                };
            }
        }
        self.store.models(models)?;
        let phase_refs: Vec<&[f64]> = phases.iter().map(|p| p.as_slice()).collect();
        let (built, quality) = stages::mask::build_mask_with_quality(sections, combine, self.config, &phase_refs, magnitudes, &self.meta, quality)?;
        let removed = built.scanner_masked_removed;
        if removed > 0 {
            let fov = built.mask.len().max(1);
            let without = built.scanner_masked_voxels;
            let kept = built.voxels;
            self.log(Level::Info, &format!(
                "{:.1} % of the field of view has no phase (written by the scanner as a constant): {removed} voxels \
                 ({:.1} % of the recipe's mask) left out of the mask",
                100.0 * without as f64 / fov as f64, 100.0 * removed as f64 / (kept + removed).max(1) as f64));
            if !self.decisions.iter().any(|d| d.topic == Topic::ScannerMaskedPhase) {
                self.decide(Decision::new(Topic::ScannerMaskedPhase, Choice::Partial,
                    "Voxels where the scanner recorded no phase were left out of the mask.")
                    .with_detail(format!("{removed} voxels ({:.1} % of the recipe's mask; {:.1} % of the field of view has no phase)",
                                         100.0 * removed as f64 / (kept + removed).max(1) as f64,
                                         100.0 * without as f64 / fov as f64)));
            }
        }
        Ok((built.mask, quality))
    }

    pub(crate) fn mask(&mut self, step: &Step) -> Result<(), S::Error> {
        // The plan leaves two-pass out when a mask is supplied ([`crate::plan::plan`]).
        if self.config.pipeline.do_qsm && self.config.masking.two_pass && !self.plan.two_pass {
            self.decide(Decision::new(Topic::TwoPass, Choice::NotApplied,
                "Two-pass artefact reduction was not applied: the mask was supplied, so there was no recipe to \
                 derive a reliable-phase mask from."));
        }
        let supplied: &[Item] = if self.config.masking.custom_mask_tool.is_some() { &[Item::SuppliedMask] } else { &[] };
        let params = self.with_supplied(stages::mask::params(self.config), supplied);
        let Some(key) = self.begin(Stage::Mask(Pass::Main), None, &params, "Skipping mask (cached)", "Creating mask") else { return Ok(()) };
        // A supplied mask, when one is asked for and found. One that cannot be used falls back to
        // the recipe, whose models the plan did not list.
        let mut models = step.models.clone();
        if let Some(tool) = self.config.masking.custom_mask_tool.clone() {
            match self.store.supplied_mask() {
                Supplied::Found { origin, data, dims, affine } => {
                    if let Some(mask) = stages::mask::conform_custom_mask(&data, dims, &affine, &self.meta) {
                        stages::mask::require_voxels(&mask, stages::mask::MaskSource::Supplied(&origin))?;
                        self.log(Level::Info, &format!("Using custom mask from {origin}"));
                        self.decide(Decision::new(Topic::MaskSource, Choice::Supplied,
                            "The brain mask was the one supplied with the scan.").with_detail(origin));
                        self.put(Item::BrainMask, Data::Mask(&mask))?;
                        return self.complete(Done {
                            stage: Stage::Mask(Pass::Main), algorithm: None, params: &params, outcome: None, key: &key,
                            outputs: &[Item::BrainMask],
                        }, "Mask (custom)");
                    }
                    self.log(Level::Warn, &format!(
                        "custom mask {origin} could not be conformed to the working grid — computing instead"));
                    self.decide(Decision::new(Topic::MaskSource, Choice::Computed,
                        "The supplied mask could not be brought onto the working grid, so the mask was computed.")
                        .with_detail(origin));
                    models = stages::mask::model_ids(&self.config.masking.sections, self.config);
                }
                Supplied::Unreadable { origin, error } => {
                    self.log(Level::Warn, &format!("could not read custom mask {origin} ({error}) — computing instead"));
                    self.decide(Decision::new(Topic::MaskSource, Choice::Computed,
                        "The supplied mask could not be read, so the mask was computed.")
                        .with_detail(format!("{origin}: {error}")));
                    models = stages::mask::model_ids(&self.config.masking.sections, self.config);
                }
                Supplied::Missing { searched } => {
                    self.log(Level::Info, &format!(
                        "no custom mask under derivatives (tool: {tool}) for {searched} — computing"));
                    self.decide(Decision::new(Topic::MaskSource, Choice::Computed,
                        "No supplied mask was found, so the mask was computed.").with_detail(searched));
                }
            }
        }
        self.log(Level::Info, &format!("Creating mask ({} section(s), combined with {})",
            self.config.masking.sections.len(), self.config.masking.combine));
        let mask = self.build_mask(&self.config.masking.sections.clone(), self.config.masking.combine, &models)?;
        stages::mask::require_voxels(&mask, stages::mask::MaskSource::Recipe(&self.config.masking))?;
        self.put(Item::BrainMask, Data::Mask(&mask))?;
        self.complete(Done {
            stage: Stage::Mask(Pass::Main), algorithm: None, params: &params, outcome: None, key: &key, outputs: &[Item::BrainMask],
        }, "Mask creation")
    }

    /// The reliable-phase mask of a two-pass run, restricted to the main mask.
    pub(crate) fn reliable_mask(&mut self, step: &Step) -> Result<(), S::Error> {
        let sections = self.config.masking.resolved_two_pass_sections();
        let params = stages::mask::reliable_params(self.config);
        let stage = Stage::Mask(Pass::Reliable);
        let Some(key) = self.begin(stage, None, &params, "Skipping reliable-phase mask (cached)",
                                   "Creating the reliable-phase mask") else { return Ok(()) };
        let combine = self.config.masking.resolved_two_pass_combine();
        self.log(Level::Info, &format!("Creating the reliable-phase mask ({} section(s), combined with {combine})",
                                       sections.len()));
        let local = self.config.masking.two_pass_mode == crate::enums::TwoPassMode::Local;
        let (reliable, quality) = self.build_mask_and_quality(&sections, combine, &step.models, local)?;
        let main_item = self.pass_mask(Pass::Main);
        let main = self.store.mask(&main_item)?;
        let restricted = match quality {
            Some(quality) => {
                // Only the holes at strong sources, inside where the main pass's map is defined.
                let support_item = self.support(Pass::Main);
                let support = self.store.mask(&support_item)?;
                let selected = stages::mask::source_holes(&reliable, &main, &support, &quality, &self.meta)?;
                self.log(Level::Info, &format!(
                    "Two-pass (local): {} of {} hole voxels inside the main pass's support kept, around {} strong source(s)",
                    selected.hole_voxels, selected.candidate_voxels, selected.sources));
                stages::mask::reliable_mask(&selected.reliable, &main)?
            }
            None => stages::mask::reliable_mask(&reliable, &main)?,
        };
        self.put(Item::ReliableMask, Data::Mask(&restricted))?;
        self.complete(Done {
            stage, algorithm: None, params: &params, outcome: None, key: &key, outputs: &[Item::ReliableMask],
        }, "Reliable-phase mask")
    }
}
