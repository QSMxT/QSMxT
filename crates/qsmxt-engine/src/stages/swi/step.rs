//! The SWI and SMWI steps of a run ([`crate::run`]) ([`super`]).

use log::Level;

use crate::plan::Stage;
use crate::run::{Done, Run};
use crate::stages;
use crate::store::{Data, Item, StageStore};

impl<S: StageStore + ?Sized> Run<'_, S> {
    /// CLEAR-SWI over every echo, and its minIP.
    pub(crate) fn swi(&mut self) -> Result<(), S::Error> {
        let params = stages::swi::swi_params(self.config, &self.meta)?;
        let Some(key) = self.begin(Stage::Swi, Some("clear-swi"), &params, "Skipping swi (cached)", "Computing SWI") else { return Ok(()) };
        let n = self.n_echoes();
        self.log(Level::Info, &format!("Computing CLEAR-SWI ({} echo{})", n, if n == 1 { "" } else { "es" }));
        let phase_items: Vec<Item> = (1..=n).map(|e| self.echo_phase(e)).collect();
        let mag_items: Vec<Item> = (1..=n).map(|e| self.echo_magnitude(e)).collect();
        let phases = phase_items.iter().map(|i| self.store.real(i)).collect::<Result<Vec<_>, _>>()?;
        let magnitudes = mag_items.iter().map(|i| self.store.real(i)).collect::<Result<Vec<_>, _>>()?;
        let stages::swi::Swi { projected: swi, filtered_phase } =
            stages::swi::swi(&phases, &magnitudes, &self.meta, self.config)?;
        drop((phases, magnitudes));
        self.put(Item::Swi, Data::Real(&swi.image))?;
        self.put(Item::SwiMip, Data::OnGrid(&swi.mip, &swi.mip_geometry))?;
        let mut outputs = vec![Item::Swi, Item::SwiMip];
        if let Some(phase) = filtered_phase {
            self.put(Item::SwiPhase, Data::Real(&phase))?;
            self.store.annotate(&Item::SwiPhase, crate::bids::outputs::swi_phase_fields(n))?;
            outputs.push(Item::SwiPhase);
        }
        self.complete(Done {
            stage: Stage::Swi, algorithm: Some("clear-swi"), params: &params, outcome: None, key: &key,
            outputs: &outputs,
        }, "SWI")
    }

    /// SMWI, weighted by the final, referenced susceptibility map over the voxels it is defined on
    /// (outside them χ is 0 by construction, not by measurement).
    pub(crate) fn smwi(&mut self) -> Result<(), S::Error> {
        let params = stages::swi::smwi_params(self.config, &self.meta)?;
        if !self.store.has(&Item::Chi) {
            self.log(Level::Warn, "Skipping SMWI: no susceptibility map");
            self.skip("There was no susceptibility map to weight by.");
            return Ok(());
        }
        let Some(key) = self.begin(Stage::Smwi, Some("smwi"), &params, "Skipping smwi (cached)", "Computing SMWI") else { return Ok(()) };
        let s = &self.config.smwi;
        self.log(Level::Info, &format!("Computing SMWI (threshold {} ppm, power {}, mIP over {} slices)",
                                       s.threshold_ppm, s.power, s.mip_window));
        let mask_item = if self.store.has(&Item::ChiSupport) { Item::ChiSupport } else { self.mask_item() };
        let chi = self.store.real(&Item::Chi)?;
        let magnitude = self.store.real(&Item::CombinedMagnitude)?;
        let mask = self.store.mask(&mask_item)?;
        let weighted = stages::swi::smwi(&magnitude, &chi, &mask, &self.meta, self.config)?;
        drop((chi, magnitude, mask));
        let mut outputs = Vec::new();
        for (contrast, image) in &weighted {
            self.put(Item::Smwi(*contrast), Data::Real(&image.image))?;
            self.put(Item::SmwiMip(*contrast), Data::OnGrid(&image.mip, &image.mip_geometry))?;
            outputs.extend([Item::Smwi(*contrast), Item::SmwiMip(*contrast)]);
        }
        self.complete(Done {
            stage: Stage::Smwi, algorithm: Some("smwi"), params: &params, outcome: None, key: &key, outputs: &outputs,
        }, "SMWI")
    }
}
