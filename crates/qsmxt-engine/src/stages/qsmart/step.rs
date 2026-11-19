//! The QSMART step of a run ([`crate::run`]), for either pass ([`super`]).

use crate::observe::VolumeData;
use crate::plan::{Pass, Stage};
use crate::run::{Done, Run, pass_label};
use crate::stages;
use crate::store::{Data, Item, StageStore};

impl<S: StageStore + ?Sized> Run<'_, S> {
    /// QSMART, from the total field, keeping its intermediates.
    pub(crate) fn qsmart(&mut self, pass: Pass) -> Result<(), S::Error> {
        let name = Stage::Qsmart(pass).name();
        let params = stages::qsmart::params(self.config);
        let Some(key) = self.begin(&name, Some("qsmart"), &params, "Skipping qsmart (cached)",
                                   "QSMART reconstruction") else { return Ok(()) };
        let field = self.store.real(&Item::FieldPpm)?;
        let mask_item = self.pass_mask(pass);
        let mask = self.store.mask(&mask_item)?;
        // QSMART's echo fit's reliability map, when the field came from it: both stages run
        // inside it.
        let reliability = if self.config.field_mapping.b0_estimation == crate::enums::B0Estimation::QsmartEchofit {
            Some(self.store.mask(&Item::FieldReliability)?)
        } else {
            None
        };
        // The magnitude vessels are detected on (and MEDI's edge weighting): every echo's, for
        // QSMART's own, or the combined one.
        let magnitude = match self.config.inversion.qsmart.vessel_magnitude {
            // The inhomogeneity correction made it, for both passes.
            crate::enums::QsmartVesselMagnitude::MeanN4 if self.plan.homogeneity.vessel => {
                let m = if self.store.has(&Item::VesselMagnitude) { Some(self.store.real(&Item::VesselMagnitude)?) } else { None };
                stages::qsmart::Magnitude::Vessel(m)
            }
            crate::enums::QsmartVesselMagnitude::MeanN4 => {
                // The mean is over every echo: a run with magnitude must have each echo's. A run
                // without any detects vessels on a uniform magnitude.
                let n = self.n_echoes();
                let items: Vec<Item> = (1..=n).map(|e| self.echo_magnitude(e)).collect();
                let missing: Vec<usize> = (1..=n).filter(|&e| !self.store.has(&items[e - 1])).collect();
                if !missing.is_empty() && missing.len() < n {
                    let list = missing.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
                    return Err(crate::EngineError::Config(format!(
                        "qsmart: no magnitude for echo {list} of {n}: QSMART's vessel magnitude is the \
                         mean over every echo. Supply every echo's magnitude, or use \
                         --qsmart-vessel-magnitude combined")).into());
                }
                let mut echoes = Vec::with_capacity(n);
                for item in items.iter().filter(|_| missing.is_empty()) {
                    echoes.push(self.store.real(item)?);
                }
                stages::qsmart::Magnitude::Echoes(echoes)
            }
            crate::enums::QsmartVesselMagnitude::Combined => stages::qsmart::Magnitude::Combined(self.combined_magnitude()?),
        };
        let stages::qsmart::Qsmart { chi, intermediates } = stages::qsmart::qsmart(
            &field, &mask, reliability.as_deref(), magnitude, &self.meta, self.config, self.observer)?;
        drop((field, mask, reliability));
        self.put(Item::RawChi(pass), Data::Real(&chi))?;
        let mut outputs = vec![Item::RawChi(pass)];
        for (intermediate, volume) in &intermediates {
            let item = Item::QsmartIntermediate(pass, intermediate);
            self.put(item, match volume {
                VolumeData::Real(v) => Data::Real(v),
                VolumeData::Mask(m) => Data::Mask(m),
            })?;
            outputs.push(item);
        }
        self.complete(Done {
            step: &name, algorithm: Some("qsmart"), params: &params, outcome: None, key: &key, outputs: &outputs,
        }, &format!("QSMART{}", pass_label(pass)))
    }
}
