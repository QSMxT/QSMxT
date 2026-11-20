//! The phase steps of a run ([`crate::run`]): each echo read from the store, scaled, combined,
//! windowed or resampled ([`super`]), and kept.

use log::Level;

use crate::plan::Stage;
use crate::record::{Choice, Decision, Topic};
use crate::run::{Done, Run};
use crate::stages;
use crate::store::{Data, Item, StageStore};

impl<S: StageStore + ?Sized> Run<'_, S> {
    /// Where the phase steps read and write each echo in this run ([`stages::phase::Chain`]).
    pub(crate) fn chain(&self) -> stages::phase::Chain {
        stages::phase::Chain {
            makes_magnitude: self.facts.n_coils > 1 || self.facts.real_imaginary,
            has_magnitude: self.meta.has_magnitude || self.facts.n_coils > 1 || self.facts.real_imaginary,
            windowed: self.plan.has(Stage::TukeyWindow),
            resampled: self.meta.source_geometry.is_some(),
            coils: self.facts.n_coils > 1,
        }
    }

    /// Keep a volume on the acquired grid: on the run's grid when it is not resampled, on a grid
    /// of its own when it is.
    pub(crate) fn put_acquired(&mut self, item: Item, data: &[f64]) -> Result<(), S::Error> {
        match self.meta.source_geometry {
            Some((dims, affine)) => self.put(item, Data::OnGrid(data, &stages::output_space::acquired(dims, &affine))),
            None => self.put(item, Data::Real(data)),
        }
    }

    /// A phase step's echo `e`, as it writes it: on the acquired grid unless it is the working
    /// echo (`working`), with a combined echo of uncombined coils also published as it is.
    pub(crate) fn put_echo(&mut self, e: usize, (phase_item, phase): (Item, &[f64]), magnitude: Option<(Item, &[f64])>,
                outputs: &mut Vec<Item>) -> Result<(), S::Error> {
        let working = phase_item == Item::Phase(e);
        for (item, data) in std::iter::once((phase_item, phase)).chain(magnitude) {
            if working { self.put(item, Data::Real(data))? } else { self.put_acquired(item, data)? }
            outputs.push(item);
        }
        if working && self.chain().coils {
            self.put(Item::CombinedEchoPhase(e), Data::Real(phase))?;
            self.put(Item::CombinedEchoMagnitude(e), Data::Real(magnitude.map(|(_, m)| m).unwrap_or_default()))?;
            outputs.extend([Item::CombinedEchoPhase(e), Item::CombinedEchoMagnitude(e)]);
        }
        Ok(())
    }

    /// Refuse a run resampled without a step to resample it: what [`crate::scan::can_resample`]
    /// keeps from happening, for a plan made without it.
    pub(crate) fn check_resampling_planned(&self) -> Result<(), S::Error> {
        if self.meta.source_geometry.is_some() && !self.plan.has(Stage::ResampleAxial) {
            return Err(crate::EngineError::Config(
                "the run is resampled to an axial grid, but its plan has no step to resample it".into()).into());
        }
        Ok(())
    }

    /// Every echo's phase in radians, on the acquired grid. Each echo is read, rescaled and kept
    /// before the next is read. The magnitude given is kept as it is, unread, when nothing comes
    /// after.
    pub(crate) fn scale_phase(&mut self) -> Result<(), S::Error> {
        self.check_resampling_planned()?;
        let chain = self.chain();
        let params = stages::phase::params(&self.meta, None, &chain);
        let Some(key) = self.begin(Stage::ScalePhase, None, &params, "Skipping scale_phase (cached)",
                                   "Rescaling phase to radians") else { return Ok(()) };
        let n = self.n_echoes();
        let mut outputs = Vec::new();
        // One conversion for every echo (qsm_core::pipeline::scale_phases_to_pi): echoes scaled
        // separately would not keep the phase differences between them.
        let mut phases = (1..=n).map(|e| self.store.real(&Item::AcquiredPhase(e))).collect::<Result<Vec<_>, _>>()?;
        qsm_core::pipeline::scale_phases_to_pi(&mut phases);
        for (e, phase) in (1..=n).zip(&phases) {
            self.put_echo(e, (chain.scaled_phase(e), phase), None, &mut outputs)?;
        }
        drop(phases);
        // Raw (uncorrected) per-echo magnitudes as they are, for MCPC-3D-S, linear fit and ROMEO.
        if chain.copies_magnitude() {
            for e in 1..=n {
                if self.store.has(&Item::AcquiredMagnitude(e)) {
                    self.put(Item::Magnitude(e), Data::Same(Item::AcquiredMagnitude(e)))?;
                    outputs.push(Item::Magnitude(e));
                }
            }
        }
        self.complete(Done {
            stage: Stage::ScalePhase, algorithm: None, params: &params, outcome: None, key: &key, outputs: &outputs,
        }, "Rescale phase")
    }

    /// `scale_phase` for a scan given as real and imaginary parts: they become magnitude and
    /// phase ([`stages::phase::real_imaginary`]) before anything else sees them, kept on the
    /// acquired grid as any other scan's phase is. The sign convention follows from the scanner's
    /// manufacturer, so it is a parameter (and keys the cache).
    pub(crate) fn scale_real_imaginary(&mut self) -> Result<(), S::Error> {
        self.check_resampling_planned()?;
        let chain = self.chain();
        let manufacturer = self.store.sidecar()?.and_then(|s| s.manufacturer);
        let negate_phase = stages::phase::negates_real_imaginary_phase(manufacturer.as_deref());
        let params = stages::phase::params(&self.meta, Some(negate_phase), &chain);
        let Some(key) = self.begin(Stage::ScalePhase, None, &params, "Skipping scale_phase (cached)",
                                   "Converting real/imaginary to magnitude/phase") else { return Ok(()) };
        let n = self.n_echoes();
        let acquired = stages::phase::acquired_grid(&self.meta).n_total();
        let (mut real, mut imaginary) = (Vec::with_capacity(n), Vec::with_capacity(n));
        for e in 1..=n {
            for (item, out) in [(Item::AcquiredReal(e), &mut real), (Item::AcquiredImaginary(e), &mut imaginary)] {
                if !self.store.has(&item) {
                    let part = if matches!(item, Item::AcquiredReal(_)) { "real" } else { "imaginary" };
                    return Err(crate::EngineError::Config(format!(
                        "echo {e}: a real/imaginary run without the echo's {part} part")).into());
                }
                let v = self.store.real(&item)?;
                if v.len() != acquired {
                    let what = self.store.identity(&item);
                    return Err(crate::EngineError::DimensionMismatch(format!(
                        "{} has {} voxels, expected {acquired}", what.as_str().unwrap_or_default(), v.len())).into());
                }
                out.push(v);
            }
        }
        let (converted, lines) = stages::phase::real_imaginary(&real, &imaginary, &self.meta, negate_phase);
        drop((real, imaginary));
        for (level, line) in lines {
            self.log(level, &line);
        }
        if negate_phase {
            self.decide(Decision::new(Topic::PhaseSign, Choice::Negated,
                "The phase from the real and imaginary parts was negated, as the scanner's manufacturer \
                 records it with the opposite sign.").with_detail(manufacturer.clone().unwrap_or_default()));
        }
        let chopped: Vec<&str> = ["x", "y", "z"].into_iter().zip(converted.chopping.corrected)
            .filter(|(_, on)| *on).map(|(a, _)| a).collect();
        if !chopped.is_empty() {
            self.decide(Decision::new(Topic::SliceChopping, Choice::Corrected,
                "Phase alternating by π between neighbouring slices was corrected.")
                .with_detail(format!("along {}", chopped.join(", "))));
        }
        let outcome = Some(("real_imaginary_conversion", stages::phase::slice_chopping_json(&converted.chopping)));
        let mut outputs = Vec::new();
        for (i, (phase, mag)) in converted.phases.iter().zip(&converted.magnitudes).enumerate() {
            let e = i + 1;
            let magnitude = chain.scaled_magnitude(e).map(|item| (item, mag.as_slice()));
            self.put_echo(e, (chain.scaled_phase(e), phase), magnitude, &mut outputs)?;
        }
        self.complete(Done {
            stage: Stage::ScalePhase, algorithm: None, params: &params, outcome, key: &key, outputs: &outputs,
        }, "Rescale phase")
    }

    /// `scale_phase` for uncombined coils: every coil's echoes combined by MCPC-3D-S
    /// ([`stages::phase::combine_coils`]) on the acquired grid into the per-echo phase (radians,
    /// coil offsets removed) and magnitude the later steps read. The combined echoes are also
    /// published, as they are on the working grid, by the last of the phase steps.
    pub(crate) fn combine_coils(&mut self) -> Result<(), S::Error> {
        self.check_resampling_planned()?;
        let chain = self.chain();
        let coils = self.store.coils();
        let n = self.n_echoes();
        // MCPC-3D-S estimates each coil's phase offset from the difference of two echoes.
        // `qsmxt run` refuses such a run up front; this keeps a direct caller from QSM.rs's assertion.
        if n < 2 {
            return Err(crate::EngineError::Config(format!(
                "MCPC-3D-S coil combination needs at least 2 echoes, and this run has {n} echo with {} \
                 uncombined coils. Combine the coils first (on the scanner, or with another tool).",
                coils.len())).into());
        }
        let params = stages::phase::coil_params(self.config, &self.meta, &coils, &chain);
        let Some(key) = self.begin(Stage::ScalePhase, Some("mcpc3ds"), &params, "Skipping coil combination (cached)",
                                   "MCPC-3D-S coil combination") else { return Ok(()) };
        self.log(Level::Info, &format!("Coil combination (MCPC-3D-S): {} coils x {n} echoes", coils.len()));
        let acquired = stages::phase::acquired_grid(&self.meta).n_total();
        let (mut phases, mut magnitudes) = (Vec::new(), Vec::new());
        for (c, number) in coils.iter().enumerate().map(|(i, &number)| (i + 1, number)) {
            let (mut cp, mut cm) = (Vec::with_capacity(n), Vec::with_capacity(n));
            for e in 1..=n {
                let (phase_item, mag_item) = (Item::CoilPhase(c, e), Item::CoilMagnitude(c, e));
                let p = self.store.real(&phase_item)?;
                if p.len() != acquired {
                    let what = self.store.identity(&phase_item);
                    return Err(crate::EngineError::DimensionMismatch(format!(
                        "{} has {} voxels, expected {acquired}", what.as_str().unwrap_or_default(), p.len())).into());
                }
                if !self.store.has(&mag_item) {
                    return Err(crate::EngineError::Config(format!(
                        "coil {number} echo {e}: magnitude required for MCPC-3D-S")).into());
                }
                let m = self.store.real(&mag_item)?;
                if m.len() != acquired {
                    let what = self.store.identity(&mag_item);
                    return Err(crate::EngineError::DimensionMismatch(format!(
                        "{} has {} voxels, expected {acquired}", what.as_str().unwrap_or_default(), m.len())).into());
                }
                cp.push(p);
                cm.push(m);
            }
            phases.push(cp);
            magnitudes.push(cm);
        }
        // One conversion for every coil and echo (qsm_core::pipeline::scale_phases_to_pi).
        {
            let mut all: Vec<&mut Vec<f64>> = phases.iter_mut().flatten().collect();
            qsm_core::pipeline::scale_phases_to_pi(&mut all);
        }
        let combined = stages::phase::combine_coils(&phases, &magnitudes, &self.meta, self.config)?;
        drop((phases, magnitudes));
        let mut outputs = Vec::new();
        for e in 1..=n {
            let magnitude = chain.scaled_magnitude(e).map(|item| (item, combined.magnitudes[e - 1].as_slice()));
            self.put_echo(e, (chain.scaled_phase(e), &combined.phases[e - 1]), magnitude, &mut outputs)?;
        }
        // Where the offsets were estimated: on the acquired grid, which all coils share.
        if self.meta.source_geometry.is_some() {
            let mask: Vec<f64> = combined.mask.iter().map(|&m| m as f64).collect();
            self.put_acquired(Item::CoilCombinationMask, &mask)?;
        } else {
            self.put(Item::CoilCombinationMask, Data::Mask(&combined.mask))?;
        }
        outputs.push(Item::CoilCombinationMask);
        self.complete(Done {
            stage: Stage::ScalePhase, algorithm: Some("mcpc3ds"), params: &params, outcome: None, key: &key,
            outputs: &outputs,
        }, "Coil combination")
    }

    /// The k-space Tukey window on every echo, on the acquired grid: phase scaling's phase and the
    /// magnitude beside it ([`stages::phase::Chain::acquired_magnitude`]), each echo read, windowed
    /// and kept before the next.
    pub(crate) fn tukey_window(&mut self) -> Result<(), S::Error> {
        let chain = self.chain();
        let params = stages::phase::tukey_params(&self.meta, self.config, &chain);
        let Some(key) = self.begin(Stage::TukeyWindow, None, &params, "Skipping tukey_window (cached)",
                                   "Applying the k-space Tukey window") else { return Ok(()) };
        if let Some(line) = stages::phase::tukey_window_line(self.config) {
            self.log(Level::Info, &line);
        }
        let mut outputs = Vec::new();
        for e in 1..=self.n_echoes() {
            // The plan refuses the window without magnitude; this covers a direct caller.
            let Some(mag_item) = chain.acquired_magnitude(e).filter(|i| self.store.has(i)) else {
                return Err(crate::EngineError::Config(format!(
                    "echo {e}: the Tukey window (--tukey-window) needs the echo's magnitude")).into());
            };
            let mut phase = self.store.real(&chain.scaled_phase(e))?;
            let mut mag = self.store.real(&mag_item)?;
            stages::phase::tukey_window(&mut mag, &mut phase, &self.meta, self.config);
            let (phase_item, mag_item) = chain.windowed(e);
            self.put_echo(e, (phase_item, &phase), Some((mag_item, &mag)), &mut outputs)?;
        }
        self.complete(Done {
            stage: Stage::TukeyWindow, algorithm: None, params: &params, outcome: None, key: &key, outputs: &outputs,
        }, "Tukey window")
    }

    /// Every echo resampled to the working (axial) grid, magnitude and phase together
    /// ([`stages::phase::working_echo`]), each echo read, resampled and kept before the next.
    /// Skipped for a run that is not resampled: the step before it wrote the working echoes.
    pub(crate) fn resample_axial(&mut self) -> Result<(), S::Error> {
        if self.meta.source_geometry.is_none() {
            return Ok(());
        }
        let chain = self.chain();
        let params = stages::phase::resample_params(&self.meta, &chain);
        let Some(key) = self.begin(Stage::ResampleAxial, None, &params, "Skipping resample_axial (cached)",
                                   "Resampling to axial") else { return Ok(()) };
        let mut outputs = Vec::new();
        for e in 1..=self.n_echoes() {
            let (phase_item, mag_item) = chain.to_resample(e);
            let phase = self.store.real(&phase_item)?;
            let magnitude = match mag_item.filter(|i| self.store.has(i)) {
                Some(item) => Some(self.store.real(&item)?),
                None => None,
            };
            let had_magnitude = magnitude.is_some();
            let out = stages::phase::working_echo(phase, magnitude, true, &self.meta);
            // Without a magnitude the phase was resampled with a uniform one, which is not kept.
            let magnitude = out.magnitude.as_deref().filter(|_| had_magnitude).map(|m| (Item::Magnitude(e), m));
            self.put_echo(e, (Item::Phase(e), &out.phase), magnitude, &mut outputs)?;
        }
        self.complete(Done {
            stage: Stage::ResampleAxial, algorithm: None, params: &params, outcome: None, key: &key, outputs: &outputs,
        }, "Resample to axial")
    }
}
