//! The R2*/T2* step of a run ([`crate::run`]) ([`super`]).

use log::Level;

use crate::run::{Done, Run};
use crate::stages;
use crate::store::{Data, Item, StageStore};

impl<S: StageStore + ?Sized> Run<'_, S> {
    /// R2* by ARLO from every echo's magnitude, and T2* from it.
    pub(crate) fn relaxometry(&mut self) -> Result<(), S::Error> {
        let params = stages::relaxometry::t2star_r2star_params(&self.meta);
        let Some(key) = self.begin("t2star_r2star", Some("arlo"), &params, "Skipping t2star_r2star (cached)",
                                   "Computing R2*/T2* maps") else { return Ok(()) };
        let n = self.n_echoes();
        self.log(Level::Info, &format!("Computing R2*/T2* maps (ARLO, {n} echoes)"));
        let mask_item = self.mask_item();
        let mask = self.store.mask(&mask_item)?;
        let mut magnitudes = Vec::with_capacity(mask.len() * n);
        for e in 1..=n {
            let item = self.working_magnitude(e);
            let echo = self.store.real(&item)?;
            if echo.len() != mask.len() {
                return Err(crate::error::EngineError::Config(format!(
                    "mag echo {e} has {} voxels, but this run is processed on a grid of {}", echo.len(), mask.len())).into());
            }
            magnitudes.extend(echo);
        }
        let (r2star, t2star) = stages::relaxometry::t2star_r2star(&magnitudes, &mask, &self.meta)?;
        drop(magnitudes);
        self.put(Item::R2star, Data::Real(&r2star))?;
        self.put(Item::T2star, Data::Real(&t2star))?;
        self.complete(Done {
            step: "t2star_r2star", algorithm: Some("arlo"), params: &params, outcome: None, key: &key,
            outputs: &[Item::R2star, Item::T2star],
        }, "T2*/R2* mapping")
    }
}
