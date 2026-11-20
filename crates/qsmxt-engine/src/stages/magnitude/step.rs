//! The magnitude steps of a run ([`crate::run`]): the inhomogeneity correction and the combined
//! magnitude ([`super`]).

use serde_json::json;

use crate::plan::Stage;
use crate::observe::StageEvent;
use crate::run::{Done, Run};
use crate::stages;
use crate::store::{Data, Item, StageStore};

impl<S: StageStore + ?Sized> Run<'_, S> {
    /// Echo `e`'s magnitude on the working grid, as the steps before the correction left it.
    pub(crate) fn current_magnitude(&mut self, e: usize) -> Result<Option<Vec<f64>>, S::Error> {
        for item in [Item::Magnitude(e), Item::AcquiredMagnitude(e)] {
            if self.store.has(&item) {
                return self.store.real(&item).map(Some);
            }
        }
        Ok(None)
    }

    /// Echo `e`'s magnitude as the steps after the inhomogeneity correction read it: corrected
    /// where the correction corrects it.
    pub(crate) fn corrected_magnitude(&mut self, e: usize) -> Result<Option<Vec<f64>>, S::Error> {
        if self.plan.homogeneity.echoes.contains(&e) {
            return self.store.real(&Item::HomogeneousMagnitude(e)).map(Some);
        }
        self.current_magnitude(e)
    }

    /// The inhomogeneity correction of what its consumers ask for ([`Plan::homogeneity`](crate::plan::Plan::homogeneity)): each
    /// echo's magnitude with the configured method, and QSMART's vessel magnitude.
    pub(crate) fn homogeneity(&mut self) -> Result<(), S::Error> {
        let targets = self.plan.homogeneity.clone();
        let params = stages::magnitude::step_params(self.config, &targets);
        let Some(key) = self.begin(Stage::InhomogeneityCorrection, None, &params, "Skipping inhomogeneity correction (cached)",
                                   "Correcting the magnitude for inhomogeneity") else { return Ok(()) };
        let mut outputs = Vec::new();
        // QSMART's vessel magnitude is the mean over every echo: a run with magnitude must have
        // each echo's.
        if targets.vessel {
            let n = self.n_echoes();
            let mut echoes = Vec::with_capacity(n);
            let mut missing = Vec::new();
            for e in 1..=n {
                match self.current_magnitude(e)? {
                    Some(m) => echoes.push(m),
                    None => missing.push(e.to_string()),
                }
            }
            if !missing.is_empty() {
                return Err(crate::EngineError::Config(format!(
                    "qsmart: no magnitude for echo {} of {n}: QSMART's vessel magnitude is the mean over every \
                     echo. Supply every echo's magnitude, or use --qsmart-vessel-magnitude combined",
                    missing.join(", "))).into());
            }
            let refs: Vec<&[f64]> = echoes.iter().map(|v| v.as_slice()).collect();
            let vessel = stages::magnitude::vessel_magnitude(&refs, &self.meta)?;
            drop(echoes);
            self.put(Item::VesselMagnitude, Data::Real(&vessel))?;
            outputs.push(Item::VesselMagnitude);
        }
        for &e in &targets.echoes {
            let Some(m) = self.current_magnitude(e)? else { continue };
            let corrected = stages::magnitude::homogeneity_correct(&m, &self.meta, self.config, &format!("echo {e}'s magnitude"))?;
            self.put(Item::HomogeneousMagnitude(e), Data::Real(&corrected))?;
            outputs.push(Item::HomogeneousMagnitude(e));
        }
        self.complete(Done {
            stage: Stage::InhomogeneityCorrection, algorithm: None, params: &params, outcome: None, key: &key, outputs: &outputs,
        }, "Inhomogeneity correction")
    }

    pub(crate) fn magnitude(&mut self) -> Result<(), S::Error> {
        let corrected = !self.plan.homogeneity.echoes.is_empty();
        // The combined magnitude is the RSS of the corrected echoes; where it is kept follows
        // `pipeline.do_magnitude`.
        let params = json!({ "echoes": if corrected { "corrected" } else { "as acquired" },
                             "published": self.config.pipeline.do_magnitude });
        let doing = if corrected { "Computing the RSS-combined magnitude of the corrected echoes" } else { "Computing RSS-combined magnitude" };
        let Some(key) = self.begin(Stage::Magnitude, Some("rss"), &params, "Skipping magnitude (cached)", doing) else { return Ok(()) };
        let n = self.n_echoes();
        let mut echoes = Vec::with_capacity(n);
        for e in 1..=n {
            echoes.extend(self.corrected_magnitude(e)?);
        }
        if !echoes.is_empty() {
            let refs: Vec<&[f64]> = echoes.iter().map(|v| v.as_slice()).collect();
            let combined = stages::magnitude::combined_magnitude(&refs)?;
            drop(echoes);
            self.put(Item::CombinedMagnitude, Data::Real(&combined))?;
            self.record(Done {
                stage: Stage::Magnitude, algorithm: Some("rss"), params: &params, outcome: None, key: &key,
                outputs: &[Item::CombinedMagnitude],
            })?;
        } else {
            self.skip("There was no magnitude to combine.");
        }
        self.observer.stage("magnitude", Some("rss"), StageEvent::Finished("RSS magnitude"));
        Ok(())
    }
}
