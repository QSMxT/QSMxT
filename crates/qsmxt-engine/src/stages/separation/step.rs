//! The χ-separation step of a run ([`crate::run`]) ([`super`]).

use log::Level;
use serde_json::json;

use crate::plan::{Pass, Step};
use crate::record::{Choice, Decision, Topic};
use crate::run::{Done, Run, chi_bound_decision};
use crate::stages;
use crate::store::{Contrast, Data, Item, StageStore};

impl<S: StageStore + ?Sized> Run<'_, S> {
    /// χ-separation of the susceptibility map (the run's, or one supplied) into its paramagnetic
    /// and diamagnetic sources, from what the earlier steps made (local field, R2*, R2′, the
    /// echoes' magnitude, the MESE). Fitted over, and 0 outside, χ's support
    /// ([`stages::separation::separation_support`]). Only ever asked for, never implied, so an
    /// input it lacks fails the run: `qsmxt run` has already refused the cases the scan decides,
    /// and what is left (the local field the reconstruction route did not write, an R2′ that did
    /// not materialise) must not be a successful run with the maps missing.
    pub(crate) fn chi_separation(&mut self, step: &Step) -> Result<(), S::Error> {
        use stages::separation as sep;
        let alg = self.config.separation.algorithm;
        let alg_name = format!("{alg}");
        // Every method needs a conventional QSM (χ_total): the run's, a supplied one, or (χ-sep
        // iLSQR) one this step reconstructs from the local field.
        let own_qsm = sep::reconstructs_qsm(self.config);
        let qsm_item = match &self.config.separation.custom_qsm_tool {
            None if own_qsm => {
                self.decide(Decision::new(Topic::QsmSource, Choice::Computed,
                    "The susceptibility map separated was reconstructed from the local field by iLSQR, \
                     as the χ-separation toolbox does."));
                None
            }
            Some(tool) => {
                if !self.store.has(&Item::SuppliedChi) {
                    return Err(crate::EngineError::Config(format!(
                        "--do-chisep ({alg_name}) needs a QSM, and --use-custom-qsm {tool} names no Chimap \
                         for this run under derivatives/{tool}")).into());
                }
                let from = self.store.identity(&Item::SuppliedChi);
                self.log(Level::Info, &format!("Using custom QSM from {}", from.as_str().unwrap_or_default()));
                self.decide(Decision::new(Topic::QsmSource, Choice::Supplied,
                    "The susceptibility map separated was the one supplied with the scan.")
                    .with_detail(from.as_str().unwrap_or_default()));
                Some(Item::SuppliedChi)
            }
            None => {
                if !self.store.has(&Item::Chi) {
                    return Err(crate::EngineError::Config(format!(
                        "--do-chisep ({alg_name}) needs a QSM, and none was made")).into());
                }
                Some(Item::Chi)
            }
        };
        let mese = self.store.mese();
        let s = &self.config.separation;
        let supplied: Vec<Item> = [(s.custom_qsm_tool.is_some(), Item::SuppliedChi),
                                   (s.custom_r2prime_tool.is_some(), Item::SuppliedR2prime)]
            .into_iter().filter_map(|(read, item)| read.then_some(item)).collect();
        if let Some(d) = chi_bound_decision(self.config, self.meta.field_strength, &self.meta.echo_times) {
            self.decide(d);
        }
        let route_estimates_r2prime = self.chisepnet_route_estimates_r2prime();
        self.decide_chisepnet_r2prime(route_estimates_r2prime);
        let params = self.with_supplied(sep::params(self.config, &self.meta, mese.is_some()), &supplied);
        let Some(key) = self.begin("chi_separation", Some(&alg_name), &params, "Skipping chi_separation (cached)",
                                   "Chi-separation") else { return Ok(()) };
        self.log(Level::Info, &format!("Chi-separation ({alg_name})"));
        let (nx, ny, nz) = self.meta.dims;
        let n_voxels = nx * ny * nz;
        let mask_item = self.mask_item();
        let brain = self.store.mask(&mask_item)?;

        // Multi-echo magnitude (voxel-major) and its RSS over echoes, when there is magnitude;
        // χ-sep MEDI also takes the echoes one volume after another, for its noise map and CSF mask.
        let chi_sep_medi = alg == crate::SeparationAlgorithm::ChiSepMedi;
        let mut echo_major = Vec::new();
        let (magnitude_multi, magnitude_rss) = if self.meta.has_magnitude {
            let n = self.n_echoes();
            let mut multi = stages::relaxometry::VoxelMajor::new(n_voxels, n);
            let mut rss = vec![0.0f64; n_voxels];
            for e in 1..=n {
                let item = self.working_magnitude(e);
                let echo = self.store.real(&item)?;
                multi.set(e - 1, &echo, &format!("mag echo {e}"))?;
                if chi_sep_medi {
                    echo_major.extend_from_slice(&echo);
                }
                for (r, m) in rss.iter_mut().zip(&echo) {
                    *r += m * m;
                }
            }
            for v in rss.iter_mut() { *v = v.sqrt(); }
            (Some(multi.data), Some(rss))
        } else {
            (None, None)
        };
        // R2* and R2′, from the relaxometry and R2 steps, or a supplied R2′.
        let mut r2star = if self.store.has(&Item::R2star) { Some(self.store.real(&Item::R2star)?) } else { None };
        let r2prime_item = if self.config.separation.custom_r2prime_tool.is_some() { Item::SuppliedR2prime } else { Item::R2prime };
        let mut r2prime = if self.store.has(&r2prime_item) { Some(self.map(&r2prime_item)?) } else { None };
        // χ-sepnet's toolbox route predicts R2' from R2* itself, with R2PRIMEnet in its training
        // frame, as the authors do; it takes an R2' only when that is measured or supplied.
        if route_estimates_r2prime {
            if r2star.is_some() {
                self.log(Level::Info, "Chi-separation (chi-sepnet): R2' predicted from R2* inside the network's route");
            }
            r2prime = None;
        }
        // The MESE's echoes on this run's grid, for HC-ChiSep. Voxels outside its FOV are zero,
        // the same as voxels outside the brain.
        let se_multi = match &mese {
            Some(m) => self.mese_on_grid(m)?.ok().map(|m| m.echoes),
            None => None,
        };
        // The local field (ppm), for the field-based methods.
        let field_item = Item::LocalField(Pass::Main);
        let mut local_field = if self.store.has(&field_item) { self.store.real(&field_item)? } else { Vec::new() };
        let mut qsm = match &qsm_item {
            Some(item) => self.map(item)?,
            None if local_field.is_empty() => Vec::new(),
            None => {
                let field_mask = self.store.mask(&Item::LocalFieldMask(Pass::Main))?;
                self.log(Level::Info, "Chi-separation: reconstructing its QSM from the local field (iLSQR)");
                sep::initial_qsm(&local_field, &field_mask, &self.meta, self.config, self.observer)?
            }
        };

        // χ-sep MEDI's noise map and CSF mask are made below, on the support, from the magnitude:
        // here they count as present when the magnitude is.
        let makes_medi_inputs = chi_sep_medi && !echo_major.is_empty();
        let (no_map, no_mask): (&[f64], &[u8]) = (&[], &[]);
        let missing = sep::missing_inputs(alg, &qsm_core::pipeline::SeparationInputs {
            local_field_ppm: &local_field, qsm: &qsm, qsm_per_echo: None, mask: &brain, r2prime: r2prime.as_deref(),
            r2star: r2star.as_deref(), magnitude_rss: magnitude_rss.as_deref(),
            magnitude_multi: magnitude_multi.as_deref(), se_magnitude_multi: se_multi.as_deref(),
            n_std: makes_medi_inputs.then_some(no_map), mask_csf: makes_medi_inputs.then_some(no_mask),
        });
        if !missing.is_empty() {
            return Err(crate::EngineError::Config(format!(
                "--do-chisep ({alg_name}) cannot run: missing {}", missing.join(", "))).into());
        }

        // Fit, and write, only where χ is defined. The run's χ is 0 on the rim background-field
        // removal eroded; fitting the brain mask would treat that rim as measured χ = 0. Its
        // support is the one referencing kept; a supplied χ has none, so it is the brain mask less
        // the voxels that χ leaves undefined. The field-based methods also need the local field
        // there, so they are further limited to where it exists.
        let published = if qsm_item == Some(Item::Chi) && self.store.has(&Item::ChiSupport) {
            Some(self.store.mask(&Item::ChiSupport)?)
        } else {
            None
        };
        let uses_field = sep::uses_field(alg);
        for (what, len) in [("brain mask", brain.len()), ("QSM", qsm.len())]
            .into_iter()
            .chain(published.as_ref().map(|p| ("QSM support", p.len())))
            .chain(uses_field.then_some(("local field", local_field.len())))
            .chain(r2prime.as_ref().map(|m| ("a relaxation map", m.len())))
            .chain(r2star.as_ref().map(|m| ("a relaxation map", m.len())))
        {
            if len != n_voxels {
                return Err(crate::EngineError::DimensionMismatch(format!(
                    "chi-separation: {what} has {len} voxels but this run is processed on {n_voxels}")).into());
            }
        }
        let support = sep::separation_support(&brain, published.as_deref(), &qsm, uses_field.then_some(local_field.as_slice()));
        self.log(Level::Info, &format!("Chi-separation over the χ support: {} of {} brain-mask voxels",
            stages::r2::count_set(&support), stages::r2::count_set(&brain)));
        // The maps combined with χ voxel by voxel take the same support, so no method — the
        // spatial and deep-learning ones included — sees values on the rim that χ does not have.
        // The χ-separation toolbox's methods take R2′ and R2* as its script passes them, unmasked
        // ([`sep::takes_relaxation_unmasked`]); what they write is still 0 outside the support.
        sep::zero_outside(&support, &mut qsm);
        if uses_field {
            sep::zero_outside(&support, &mut local_field);
        }
        for map in [r2prime.as_mut(), r2star.as_mut()].into_iter().flatten() {
            sep::restrict_relaxation(self.config, &support, map);
        }
        // DECOMPOSE takes a QSM per echo (its authors run STAR-QSM on each echo): a per-echo QSM's
        // echoes' maps, or else the one QSM given at every echo (said in the methods text).
        let qsm_per_echo = if alg != crate::SeparationAlgorithm::Decompose {
            None
        } else if qsm_item == Some(Item::Chi) && self.per_echo() {
            let mut maps = Vec::with_capacity(self.n_echoes());
            for e in 1..=self.n_echoes() {
                let mut map = self.store.real(&Item::EchoChi(e))?;
                sep::zero_outside(&support, &mut map);
                maps.push(map);
            }
            Some(qsm_core::pipeline::per_echo::voxel_major(&maps))
        } else {
            Some(sep::repeat_per_echo(&qsm, self.n_echoes()))
        };
        let (n_std, mask_csf) = if chi_sep_medi {
            let (n_std, csf) = sep::chi_sep_medi_inputs(
                &echo_major, &self.meta.echo_times, &support, self.meta.dims, self.meta.voxel_size,
                self.config.separation.chi_sep_medi.csf_r2star)?;
            self.log(Level::Info, &format!("Chi-separation (MEDI): CSF mask of {} voxels from {} R2*",
                stages::r2::count_set(&csf), self.config.separation.chi_sep_medi.csf_r2star.label()));
            (Some(n_std), Some(csf))
        } else {
            (None, None)
        };
        let inputs = qsm_core::pipeline::SeparationInputs {
            local_field_ppm: &local_field, qsm: &qsm, qsm_per_echo: qsm_per_echo.as_deref(),
            mask: &support, r2prime: r2prime.as_deref(),
            r2star: r2star.as_deref(), magnitude_rss: magnitude_rss.as_deref(),
            magnitude_multi: magnitude_multi.as_deref(), se_magnitude_multi: se_multi.as_deref(),
            n_std: n_std.as_deref(), mask_csf: mask_csf.as_deref(),
        };
        self.store.models(&step.models)?;
        let separated = sep::separate(inputs, mese.as_ref().map(|m| m.echo_times.as_slice()), &self.meta,
                                      self.config, self.observer)?;
        self.put(Item::SeparatedChi(Contrast::Paramagnetic), Data::Real(&separated.paramagnetic))?;
        self.put(Item::SeparatedChi(Contrast::Diamagnetic), Data::Real(&separated.diamagnetic))?;
        self.put(Item::SeparatedTotalChi, Data::Real(&separated.total))?;
        // What χ-sepnet's toolbox route used, recorded with what it made: R2′ is no output of it.
        if crate::config::chisepnet_route_can_estimate_r2prime(self.config) {
            let mut used = serde_json::Map::new();
            used.insert("R2primeSource".into(), json!(if route_estimates_r2prime { "predicted" } else { "measured" }));
            used.insert("R2primeMethod".into(), json!(if route_estimates_r2prime {
                "R2PRIMEnet (from R2*, within chi-separation)"
            } else {
                "R2* - R2"
            }));
            for item in [Item::SeparatedChi(Contrast::Paramagnetic), Item::SeparatedChi(Contrast::Diamagnetic),
                         Item::SeparatedTotalChi] {
                self.store.annotate(&item, used.clone())?;
            }
        }
        self.complete(Done {
            step: "chi_separation", algorithm: Some(&alg_name), params: &params, outcome: None, key: &key,
            outputs: &[Item::SeparatedChi(Contrast::Paramagnetic), Item::SeparatedChi(Contrast::Diamagnetic),
                       Item::SeparatedTotalChi],
        }, &format!("Chi-separation ({alg_name})"))
    }

    /// Whether χ-sepnet's toolbox route makes its own R2′ on this run: it has none measured to take.
    pub(crate) fn chisepnet_route_estimates_r2prime(&mut self) -> bool {
        crate::config::chisepnet_route_can_estimate_r2prime(self.config)
            && !(self.config.separation.r2prime_strategy != crate::R2PrimeStrategy::R2primenet
                && self.store.has(&Item::R2) && self.store.has(&Item::R2prime))
    }

    /// Where χ-sepnet's toolbox route got its R2′ on this run.
    pub(crate) fn decide_chisepnet_r2prime(&mut self, estimates: bool) {
        use crate::requirements::{r2primenet_field_message, r2primenet_field_policy, R2primenetFieldPolicy};
        if !crate::config::chisepnet_route_can_estimate_r2prime(self.config) {
            return;
        }
        if !estimates {
            self.decide(Decision::new(Topic::ChiSeparationR2prime, Choice::Measured,
                "χ-separation used the R2′ measured as R2* − R2."));
            return;
        }
        self.decide(Decision::new(Topic::ChiSeparationR2prime, Choice::Predicted,
            "χ-separation used its own R2′, estimated from R2* by R2PRIMEnet in the network's training frame."));
        let sep = &self.config.separation;
        let field = self.meta.field_strength;
        if r2primenet_field_policy(field, sep.r2prime_strategy, sep.r2primenet.ignore_field_mismatch)
            == R2primenetFieldPolicy::RunUncalibrated
        {
            self.log(Level::Warn, &format!(
                "{}. Running anyway (--r2primenet-ignore-field-mismatch): treat this chi-separation, built on \
                 its R2', as uncalibrated.", r2primenet_field_message(field)));
            self.decide(Decision::new(Topic::R2primenetCalibration, Choice::Uncalibrated,
                "R2PRIMEnet was run off the field strength it was trained at, so its R2′ is uncalibrated.")
                .with_detail(format!("{field} T")));
        }
    }
}
