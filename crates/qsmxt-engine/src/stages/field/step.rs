//! The field-mapping step of a run ([`crate::run`]) ([`super`]).

use log::Level;

use crate::plan::Stage;
use crate::record::{Choice, Decision, Topic};
use crate::run::{Done, Run, centre_frequency_decision};
use crate::stages;
use crate::store::{Data, Item, StageStore};

impl<S: StageStore + ?Sized> Run<'_, S> {
    /// The total field, computed once on the run's mask and shared by both passes of a two-pass
    /// run: the reliable mask is a subset of it.
    pub(crate) fn field_map(&mut self) -> Result<(), S::Error> {
        let algorithm = stages::field::algorithm(self.config, &self.meta);
        let params = stages::field::params(self.config, &self.meta);
        let Some(key) = self.begin(Stage::FieldMap, Some(&algorithm), &params, "Skipping unwrap (cached)",
                                   "Phase unwrapping / echo combination") else { return Ok(()) };
        match centre_frequency_decision(self.facts) {
            Some(d) => {
                self.log(Level::Warn, &format!("{} ({})", d.reason, d.detail.as_deref().unwrap_or_default()));
                self.decide(d);
            }
            None => self.log(Level::Info, &format!(
                "Phase converted to ppm with the scanner's centre frequency, {:.4} MHz ({:.4} T)",
                self.facts.imaging_frequency.unwrap_or_default(), self.facts.field_strength)),
        }
        let n = self.n_echoes();
        if self.per_echo() {
            // Each echo's field on its own, one echo at a time.
            let mask_item = self.mask_item();
            let mask = self.store.mask(&mask_item)?;
            let mut outputs = Vec::with_capacity(n);
            for e in 1..=n {
                self.observer.progress(e - 1, n);
                let phase_item = self.echo_phase(e);
                let phase = self.store.real(&phase_item)?;
                let magnitude = self.one_echo_magnitude(e)?;
                let field = stages::field::map_echo_field(&phase, magnitude.as_deref(), &mask, &self.echo_meta(e), self.config)?;
                self.put(Item::EchoField(e), Data::Real(&field))?;
                outputs.push(Item::EchoField(e));
            }
            self.observer.progress(n, n);
            return self.complete(Done {
                stage: Stage::FieldMap, algorithm: Some(&algorithm), params: &params, outcome: None, key: &key, outputs: &outputs,
            }, "Phase unwrapping");
        }
        let phase_items: Vec<Item> = (1..=n).map(|e| self.echo_phase(e)).collect();
        let phases = phase_items.iter().map(|i| self.store.real(i)).collect::<Result<Vec<_>, _>>()?;
        let mask_item = self.mask_item();
        let mask = self.store.mask(&mask_item)?;
        let mut magnitudes = Vec::with_capacity(n);
        for e in 1..=n {
            let item = self.echo_magnitude(e);
            if self.store.has(&item) {
                magnitudes.push(self.store.real(&item)?);
            }
        }
        let phase_refs: Vec<&[f64]> = phases.iter().map(|p| p.as_slice()).collect();
        let mag_refs: Vec<&[f64]> = magnitudes.iter().map(|m| m.as_slice()).collect();
        let field = stages::field::map_field(&phase_refs, &mag_refs, &mask, &self.meta, self.config)?;
        drop(phases);
        if field.excluded > 0 {
            let in_mask = mask.iter().filter(|&&m| m != 0).count();
            self.decide(Decision::new(Topic::FieldMask, Choice::Partial,
                "Voxels where the phase offset could not be estimated were excluded from the field map and every step after it.")
                .with_detail(format!("{} voxels ({:.2} % of the mask)", field.excluded,
                                     100.0 * field.excluded as f64 / in_mask.max(1) as f64)));
        }

        self.put(Item::FieldPpm, Data::Real(&field.b0_field_ppm))?;
        self.put(Item::TotalFieldHz, Data::Real(&field.total_field_hz))?;
        self.put(Item::FieldMask, Data::Mask(&field.field_mask))?;
        let first_echo_time = self.meta.echo_times[0];
        let fields = |item: &Item| crate::bids::outputs::step_fields(item, first_echo_time).unwrap_or_default();
        self.store.annotate(&Item::TotalFieldHz, fields(&Item::TotalFieldHz))?;
        let mut outputs = vec![Item::FieldPpm, Item::TotalFieldHz, Item::FieldMask];
        // A published field map is paired with a magnitude for anatomical reference: the first
        // echo's, the one with the most signal.
        if let Some(first) = magnitudes.first() {
            self.put(Item::TotalFieldMagnitude, Data::Real(first))?;
            self.store.annotate(&Item::TotalFieldMagnitude, fields(&Item::TotalFieldMagnitude))?;
            outputs.push(Item::TotalFieldMagnitude);
        }
        if let Some(report) = &field.robust_fit_report {
            self.put(Item::RobustFitReport, Data::Json(report))?;
            outputs.push(Item::RobustFitReport);
        }
        // QSMART's echo fit's reliability map, which QSMART reconstructs inside.
        if let Some(reliability) = &field.reliability {
            self.put(Item::FieldReliability, Data::Mask(reliability))?;
            outputs.push(Item::FieldReliability);
        }
        // The threshold QSMART's echo fit resolved, recorded beside the parameters.
        let outcome = field.qsmart_fit_threshold.map(|t| ("qsmart_fit_threshold", serde_json::json!(t)));
        self.complete(Done {
            stage: Stage::FieldMap, algorithm: Some(&algorithm), params: &params, outcome, key: &key, outputs: &outputs,
        }, "Phase unwrapping")
    }
}
