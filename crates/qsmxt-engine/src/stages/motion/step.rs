//! The motion-correction step of a run ([`crate::run`]) ([`super`]).

use log::Level;

use crate::plan::Stage;
use crate::run::{Done, Run, motion_decision};
use crate::stages;
use crate::store::{Data, Item, StageStore};

impl<S: StageStore + ?Sized> Run<'_, S> {
    /// Retrospective motion correction: every echo co-registered onto the reference echo and the
    /// mask cut to where every echo still has data. Off (or impossible: one echo, or no
    /// magnitude), what an earlier run of it left is stale: its coverage mask must not be
    /// published as this run's, and every step that read the corrected echoes or that mask has to
    /// run again on the uncorrected ones.
    pub(crate) fn motion(&mut self) -> Result<(), S::Error> {
        let m = &self.config.motion_correction;
        if !m.enabled {
            return self.store.discard("motion", &[Item::CoverageMask]);
        }
        // Decided per run by the plan, which said so before the run started.
        self.decide(motion_decision(self.plan.motion_corrected));
        if !self.plan.motion_corrected {
            return self.store.discard("motion", &[Item::CoverageMask]);
        }
        let n = self.n_echoes();
        let params = stages::motion::params(self.config, &self.meta);
        let Some(key) = self.begin(Stage::Motion, None, &params, "Skipping motion correction (cached)",
                                   "Motion correction (rigid co-registration of the echoes)") else { return Ok(()) };
        if m.reference != crate::MotionReference::First
            && self.config.masking.sections.iter().any(|s| s.input == crate::MaskingInput::MagnitudeFirst)
        {
            self.log(Level::Warn, &format!(
                "Motion correction aligns the echoes onto echo {} but the mask was made on the first \
                 echo's magnitude; if the head moved, the mask is offset from the corrected echoes", m.reference));
        }
        let (mut phases, mut magnitudes) = (Vec::with_capacity(n), Vec::with_capacity(n));
        for e in 1..=n {
            phases.push(self.store.real(&Item::Phase(e))?);
            magnitudes.push(self.store.real(&Item::Magnitude(e))?);
        }
        let mask = self.store.mask(&Item::BrainMask)?;
        let phase_refs: Vec<&[f64]> = phases.iter().map(|v| v.as_slice()).collect();
        let mag_refs: Vec<&[f64]> = magnitudes.iter().map(|v| v.as_slice()).collect();
        let r = stages::motion::correct(&phase_refs, &mag_refs, &mask, &self.meta, self.config)?;
        drop((phases, magnitudes));
        let mut outputs = Vec::with_capacity(2 * n + 2);
        for e in 1..=n {
            self.put(Item::CorrectedPhase(e), Data::Real(&r.phases[e - 1]))?;
            self.put(Item::CorrectedMagnitude(e), Data::Real(&r.magnitudes[e - 1]))?;
            outputs.extend([Item::CorrectedPhase(e), Item::CorrectedMagnitude(e)]);
        }
        self.put(Item::CoverageMask, Data::Mask(&r.mask))?;
        self.put(Item::MotionReport, Data::Json(&stages::motion::report(&r)))?;
        outputs.extend([Item::CoverageMask, Item::MotionReport]);
        for (level, line) in stages::motion::summary(&r, &mask) {
            self.log(level, &line);
        }
        self.complete(Done {
            stage: Stage::Motion, algorithm: None, params: &params, outcome: None, key: &key, outputs: &outputs,
        }, "Motion correction")
    }
}
