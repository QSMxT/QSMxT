//! The background-removal step of a run ([`crate::run`]), for either pass ([`super`]).

use log::Level;
use serde_json::json;

use crate::plan::{Pass, Step};
use crate::run::{Done, Run, pass_label};
use crate::stages;
use crate::store::{Data, Item, StageStore};

impl<S: StageStore + ?Sized> Run<'_, S> {
    /// Background-field removal for one pass, from the total field — or, for iQFM, straight from
    /// the echoes' wrapped phase (joint unwrapping and removal).
    pub(crate) fn background_removal(&mut self, step: &Step, pass: Pass) -> Result<(), S::Error> {
        let stage = step.stage;
        let label = pass_label(pass);
        let iqfm = self.config.bg_removal.algorithm == crate::enums::BfAlgorithm::Iqfm;
        let algorithm = format!("{}", self.config.bg_removal.algorithm);
        let mut params = stages::background::params(self.config);
        if self.per_echo() {
            params["per_echo"] = json!(true);
        }
        let (skipping, doing) = if iqfm {
            ("Skipping iQFM field preparation (cached)", format!("iQFM field preparation{label}"))
        } else {
            ("Skipping bgremove (cached)", format!("Background field removal{label}"))
        };
        let Some(key) = self.begin(stage, Some(&algorithm), &params, skipping, &doing) else { return Ok(()) };
        let mask_item = self.pass_mask(pass);
        let outputs = [Item::LocalField(pass), Item::LocalFieldMask(pass)];
        // UK Biobank's mask trimming reads reliability maps of the first and last echo's phase
        // over the untrimmed mask (bg_removal.ukb_mask_trim).
        let reliability = if self.config.bg_removal.ukb_mask_trim && !iqfm {
            let (phases, _) = self.phase_echoes()?;
            let mask = self.store.mask(&mask_item)?;
            let refs: Vec<&[f64]> = phases.iter().map(|p| p.as_slice()).collect();
            stages::background::ukb_phase_reliability(&refs, &mask, &self.meta, self.config)?
        } else {
            None
        };
        if self.per_echo() {
            // Each echo's field on its own (iQFM, from every echo's phase, has no per-echo form).
            let mask = self.store.mask(&mask_item)?;
            self.store.models(&step.models)?;
            if let Some(d) = self.reconstruction_box(&mask) {
                self.decide(d);
            }
            let n = self.n_echoes();
            let mut outputs = Vec::with_capacity(2 * n);
            for e in 1..=n {
                let field = self.store.real(&Item::EchoField(e))?;
                let magnitude = if stages::background::takes_echo_magnitudes(self.config) { self.one_echo_magnitude(e)? } else { None };
                self.log(Level::Info, &format!("Background removal ({algorithm}), echo {e} of {n}"));
                let local = stages::background::remove_background(&field, &mask, magnitude.as_deref(), reliability.as_ref(), &self.echo_meta(e),
                                                                  self.config, self.observer)?;
                self.put(Item::EchoLocalField(e), Data::Real(&local.local_field_ppm))?;
                self.put(Item::EchoLocalFieldMask(e), Data::Mask(&local.mask))?;
                outputs.extend([Item::EchoLocalField(e), Item::EchoLocalFieldMask(e)]);
            }
            return self.complete(Done {
                stage, algorithm: Some(&algorithm), params: &params, outcome: None, key: &key, outputs: &outputs,
            }, &format!("Background removal ({algorithm}), each echo"));
        }
        let local = if iqfm {
            let (phases, magnitudes) = self.phase_echoes()?;
            let mask = self.store.mask(&mask_item)?;
            let phase_refs: Vec<&[f64]> = phases.iter().map(|p| p.as_slice()).collect();
            let mag_refs: Vec<&[f64]> = magnitudes.iter().map(|m| m.as_slice()).collect();
            self.store.models(&step.models)?;
            stages::background::iqfm(&phase_refs, &mag_refs, &mask, &self.meta, self.config)?
        } else {
            let field = self.store.real(&Item::FieldPpm)?;
            let mask = self.store.mask(&mask_item)?;
            self.store.models(&step.models)?;
            if let Some(d) = self.reconstruction_box(&mask) {
                self.decide(d);
            }
            // PDF's data weight is a noise map made from every echo's magnitude.
            let magnitudes = if stages::background::takes_echo_magnitudes(self.config) {
                self.echo_magnitude_stack()?
            } else {
                None
            };
            self.log(Level::Info, &format!("Background removal ({algorithm}){label}"));
            let local = stages::background::remove_background(&field, &mask, magnitudes.as_deref(), reliability.as_ref(), &self.meta, self.config, self.observer)?;
            if pass == Pass::Reliable && self.config.masking.two_pass_mode == crate::enums::TwoPassMode::Local {
                // No eroded rim around the holes: LBV fills it (qsm-core `fill_eroded_rim`).
                let support = self.store.mask(&Item::LocalFieldMask(Pass::Main))?;
                let before = local.mask.iter().filter(|&&m| m != 0).count();
                let (local_field_ppm, filled) = stages::background::fill_eroded_rim(
                    &field, &mask, &local.local_field_ppm, &local.mask, &support, &self.meta)?;
                let after = filled.iter().filter(|&&m| m != 0).count();
                self.log(Level::Info, &format!(
                    "Two-pass (local): {} voxels of the rim background removal left around the holes filled by LBV",
                    after.saturating_sub(before)));
                stages::background::LocalField { local_field_ppm, mask: filled }
            } else {
                local
            }
        };
        self.put(Item::LocalField(pass), Data::Real(&local.local_field_ppm))?;
        self.put(Item::LocalFieldMask(pass), Data::Mask(&local.mask))?;
        let label = if iqfm { format!("Background removal (iQFM){label}") } else { format!("Background removal ({algorithm}){label}") };
        self.complete(Done {
            stage, algorithm: Some(&algorithm), params: &params, outcome: None, key: &key, outputs: &outputs,
        }, &label)
    }
}
