//! The R2 and R2' step of a run ([`crate::run`]), and the MESE it reads ([`super`]).

use log::Level;
use serde_json::json;

use crate::plan::Step;
use crate::record::{Choice, Decision, Topic};
use crate::run::{Done, Run};
use crate::stages;
use crate::store::{Data, Item, StageStore};

/// An R2 map, and the mask it was measured over when it was fitted from a MESE.
type R2Map = (Option<Vec<f64>>, Option<Vec<u8>>);

impl<S: StageStore + ?Sized> Run<'_, S> {
    /// The MESE's echoes, voxel-major on the working grid, and where its FOV reaches
    /// ([`stages::r2::mese_on_grid`]); why not, without a usable MESE.
    pub(crate) fn mese_on_grid(&mut self, mese: &crate::store::Mese) -> Result<Result<stages::r2::MeseOnGrid, String>, S::Error> {
        let store = &mut *self.store;
        Ok(stages::r2::mese_on_grid(mese, |n| store.supplied(&Item::MeseMagnitude(n)), self.meta.dims, &self.meta.affine)?)
    }

    /// R2 from the MESE (or a supplied map) and R2′, measured as R2* − R2 where R2 was measured
    /// and otherwise, as the strategy allows, predicted from R2* by R2PRIMEnet (or supplied). An
    /// input that is not there skips its map with a warning rather than failing the run.
    pub(crate) fn r2(&mut self, step: &Step) -> Result<(), S::Error> {
        let (want_r2, want_r2p) = (self.config.pipeline.do_r2map, self.config.pipeline.do_r2primemap);
        if !want_r2 && !want_r2p {
            return Ok(());
        }
        let mese = self.store.mese();
        let sep = &self.config.separation;
        let supplied: Vec<Item> = [(sep.custom_r2_tool.is_some(), Item::SuppliedR2),
                                   (sep.custom_r2prime_tool.is_some(), Item::SuppliedR2prime)]
            .into_iter().filter_map(|(read, item)| read.then_some(item)).collect();
        let params = self.with_supplied(stages::r2::params(self.config, &self.meta, mese.as_ref()), &supplied);
        let Some(key) = self.begin("r2_r2prime", Some("epg"), &params, "Skipping r2_r2prime (cached)",
                                   "Computing R2/R2' maps") else { return Ok(()) };
        let mask_item = self.mask_item();
        let mask = self.store.mask(&mask_item)?;
        let mut outputs = Vec::new();

        // ── R2 map (Hz) ──
        let (r2, r2_mask) = self.r2_map(mese.as_ref(), &mask)?;
        if let Some(r2) = &r2 {
            self.put(Item::R2, Data::Real(r2))?;
            outputs.push(Item::R2);
        }

        // ── R2' map (Hz) = R2* − R2 ──
        if want_r2p {
            // Where this R2' came from, recorded with it (#236): a predicted R2' looks exactly like
            // a measured one, and only that record can tell a downstream reader which it is.
            let mut source = serde_json::Map::new();
            let r2p = self.r2prime_map(step, r2.as_deref(), r2_mask.as_deref(), &mask, &mut outputs, &mut source)?;
            if let Some(r2p) = &r2p {
                self.put(Item::R2prime, Data::Real(r2p))?;
                self.store.annotate(&Item::R2prime, source)?;
                outputs.push(Item::R2prime);
            }
        }

        if outputs.is_empty() {
            return Ok(());
        }
        self.complete(Done {
            step: "r2_r2prime", algorithm: Some("epg"), params: &params, outcome: None, key: &key, outputs: &outputs,
        }, "R2/R2' mapping")
    }

    /// The R2 map (Hz): supplied, or fitted from the MESE where its FOV reaches; with the mask it
    /// was measured over.
    fn r2_map(&mut self, mese: Option<&crate::store::Mese>, mask: &[u8]) -> Result<R2Map, S::Error> {
        let config = self.config;
        let sep = &config.separation;
        // The mask R2 was actually measured over. Narrower than the brain mask when the MESE's FOV
        // does not reach all of it; `None` means R2 came from somewhere other than a MESE.
        let mut r2_mask: Option<Vec<u8>> = None;

        let r2: Option<Vec<f64>> = if let Some(tool) = &sep.custom_r2_tool {
            if self.store.has(&Item::SuppliedR2) {
                let from = self.store.identity(&Item::SuppliedR2);
                self.log(Level::Info, &format!("Using custom R2 map from {}", from.as_str().unwrap_or_default()));
                self.decide(Decision::new(Topic::R2Source, Choice::Supplied, "The R2 map was the one supplied with the scan.")
                    .with_detail(from.as_str().unwrap_or_default()));
                Some(self.map(&Item::SuppliedR2)?)
            } else {
                self.log(Level::Warn, &format!("no custom R2 map under derivatives (tool: {tool}) — skipping R2"));
                self.decide(Decision::new(Topic::R2Source, Choice::Unavailable,
                    "No supplied R2 map was found, so there is no R2 map.").with_detail(tool.clone()));
                None
            }
        } else if let Some(mese) = mese {
            if let Some(why) = crate::requirements::mese_problem(self.facts) {
                // Refused up front when R2 was asked for; here only when something else implied it.
                self.log(Level::Warn, &format!("Not measuring R2 from the MESE ({why})"));
                self.decide(Decision::new(Topic::R2Source, Choice::Unavailable,
                    "R2 was not measured: the multi-echo spin-echo acquisition cannot be used.").with_detail(why));
                None
            } else {
                self.log(Level::Info, "Computing R2 (EPG) from MESE");
                match self.mese_on_grid(mese)? {
                    Ok(stages::r2::MeseOnGrid { echoes: se, coverage }) => {
                        // R2 is only measured where the MESE's FOV reaches. Fitting outside it
                        // would put a number on interpolated zeros, and R2' = R2* - R2 would then
                        // quietly take that number as a measurement.
                        let fit_mask = stages::r2::mese_measured_mask(mask, &coverage);
                        let (in_mask, in_both) = (stages::r2::count_set(mask), stages::r2::count_set(&fit_mask));
                        if in_mask > 0 && in_both < in_mask {
                            self.log(Level::Warn, &format!(
                                "The MESE covers {:.0}% of the brain mask; R2 (and so a measured R2') \
                                 is left at zero over the remaining {} voxels.",
                                100.0 * in_both as f64 / in_mask as f64, in_mask - in_both));
                            self.decide(Decision::new(Topic::R2Coverage, Choice::Partial,
                                "R2 was measured only where the multi-echo spin-echo acquisition covers the brain mask, \
                                 and left at zero elsewhere.")
                                .with_detail(format!("{:.0}% of the brain mask", 100.0 * in_both as f64 / in_mask as f64)));
                        }
                        self.decide(Decision::new(Topic::R2Source, Choice::Measured,
                            "R2 was fitted from the multi-echo spin-echo acquisition.").with_detail(mese.name.clone()));
                        let r2 = stages::r2::r2_epg(&se, &fit_mask, mese, &self.meta)?;
                        r2_mask = Some(fit_mask);
                        Some(r2)
                    }
                    Err(why) => {
                        self.decide(Decision::new(Topic::R2Source, Choice::Unavailable,
                            "R2 was not measured: the multi-echo spin-echo acquisition cannot be used.").with_detail(why));
                        None
                    }
                }
            }
        } else {
            self.log(Level::Info, "No MESE acquisition and no custom R2 map — R2/R2' unavailable for this run");
            self.decide(Decision::new(Topic::R2Source, Choice::Unavailable,
                "There is no R2 map: the scan has no multi-echo spin-echo acquisition and none was supplied."));
            None
        };
        Ok((r2, r2_mask))
    }

    /// The R2′ map (Hz): supplied, or computed ([`Self::computed_r2prime`]); where it came from goes
    /// in `source`.
    fn r2prime_map(
        &mut self, step: &Step, r2: Option<&[f64]>, r2_mask: Option<&[u8]>, mask: &[u8], outputs: &mut Vec<Item>,
        source: &mut serde_json::Map<String, serde_json::Value>,
    ) -> Result<Option<Vec<f64>>, S::Error> {
        let config = self.config;
        let sep = &config.separation;
        let r2p: Option<Vec<f64>> = if let Some(tool) = &sep.custom_r2prime_tool {
            if self.store.has(&Item::SuppliedR2prime) {
                let from = self.store.identity(&Item::SuppliedR2prime);
                let from = from.as_str().unwrap_or_default();
                self.log(Level::Info, &format!("Using custom R2' map from {from}"));
                self.decide(Decision::new(Topic::R2primeSource, Choice::Supplied,
                    "The R2′ map was the one supplied with the scan.").with_detail(from));
                source.insert("R2primeSource".into(), json!("supplied"));
                source.insert("R2primeSuppliedFrom".into(), json!(from));
                Some(self.map(&Item::SuppliedR2prime)?)
            } else {
                self.log(Level::Warn, &format!("no custom R2' map under derivatives (tool: {tool}) — skipping R2'"));
                self.decide(Decision::new(Topic::R2primeSource, Choice::Unavailable,
                    "No supplied R2′ map was found, so there is no R2′ map.").with_detail(tool.clone()));
                None
            }
        } else {
            self.computed_r2prime(step, r2, r2_mask, mask, outputs, source)?
        };
        Ok(r2p)
    }

    /// R2′ measured as R2* − R2 where R2 was measured and the strategy allows, and otherwise
    /// predicted by R2PRIMEnet ([`Self::predicted_r2prime`]) where it can be.
    fn computed_r2prime(
        &mut self, step: &Step, r2: Option<&[f64]>, r2_mask: Option<&[u8]>, mask: &[u8], outputs: &mut Vec<Item>,
        source: &mut serde_json::Map<String, serde_json::Value>,
    ) -> Result<Option<Vec<f64>>, S::Error> {
        use crate::enums::R2PrimeStrategy;
        let config = self.config;
        let sep = &config.separation;
        let strategy = sep.r2prime_strategy;
        let has_r2star = self.store.has(&Item::R2star);
        // Measuring wins wherever it is possible and allowed: R2' = R2* - R2 is a
        // measurement, and R2PRIMEnet's is an estimate standing in for one.
        let r2p = if let (Some(r2), true) = (r2, strategy != R2PrimeStrategy::R2primenet) {
            if has_r2star {
                self.log(Level::Info, "R2' measured as R2* - R2");
                let r2star = self.store.real(&Item::R2star)?;
                // Subtract only where R2 was measured: outside the MESE's FOV, R2 is zero
                // because nothing was acquired there, and R2' = R2* - 0 would report the
                // whole of R2* as reversible.
                let (r2p, coverage) = stages::r2::measured_r2prime(&r2star, r2, mask, r2_mask)?;
                // Kept, so a 0 in the R2' map can be told apart from R2* = R2.
                self.put(Item::R2primeCoverage, Data::Mask(&coverage))?;
                outputs.push(Item::R2primeCoverage);
                let (in_mask, measured) = (stages::r2::count_set(mask), stages::r2::count_set(&coverage));
                if in_mask > 0 && measured < in_mask {
                    self.log(Level::Warn, &format!(
                        "R2' is a measurement over {:.0}% of the brain mask; the remaining {} \
                         voxels are left at zero because no R2 was measured there. Maps \
                         derived from this R2' inherit that gap.",
                        100.0 * measured as f64 / in_mask as f64, in_mask - measured));
                    self.decide(Decision::new(Topic::R2primeCoverage, Choice::Partial,
                        "R2′ was measured only where R2 was, and left at zero elsewhere.")
                        .with_detail(format!("{:.0}% of the brain mask", 100.0 * measured as f64 / in_mask as f64)));
                }
                self.decide(Decision::new(Topic::R2primeSource, Choice::Measured, "R2′ was computed as R2* − R2."));
                source.insert("R2primeSource".into(), json!("measured"));
                source.insert("R2primeMethod".into(), json!(match &sep.custom_r2_tool {
                    Some(tool) => format!("R2* - R2 (R2 supplied by {tool})"),
                    None => "R2* - R2 (R2 by EPG fit of the MESE)".to_string(),
                }));
                Some(r2p)
            } else {
                self.log(Level::Warn, "R2' needs both R2* and R2 - one is missing; skipping R2'");
                self.decide(Decision::new(Topic::R2primeSource, Choice::Unavailable,
                    "There is no R2′ map: R2* was not computed to subtract R2 from."));
                None
            }
        } else if strategy == R2PrimeStrategy::Mese {
            self.log(Level::Warn,
                "Skipping R2': no R2 to subtract (no MESE acquisition and no custom R2 map), \
                 and --r2prime-strategy mese does not estimate one. Use `auto` or \
                 `r2primenet` to predict R2' from R2* instead.");
            self.decide(Decision::new(Topic::R2primeSource, Choice::Unavailable,
                "There is no R2′ map: there was no R2 to subtract, and R2′ was only to be measured."));
            None
        } else if !has_r2star {
            self.log(Level::Warn, "Skipping R2': R2PRIMEnet predicts it from R2*, which was not computed");
            self.decide(Decision::new(Topic::R2primeSource, Choice::Unavailable,
                "There is no R2′ map: R2* was not computed to predict it from."));
            None
        } else {
            // Auto with no R2 to subtract, or r2primenet outright.
            self.predicted_r2prime(step, strategy, mask, source)?
        };
        Ok(r2p)
    }

    /// R2′ predicted from R2* by R2PRIMEnet, as the field strength allows: `None` where it is not
    /// run, an error where it is refused.
    fn predicted_r2prime(
        &mut self, step: &Step, strategy: crate::enums::R2PrimeStrategy, mask: &[u8],
        source: &mut serde_json::Map<String, serde_json::Value>,
    ) -> Result<Option<Vec<f64>>, S::Error> {
        use crate::enums::R2PrimeStrategy;
        use crate::requirements::{r2primenet_field_message, r2primenet_field_policy, R2primenetFieldPolicy};
        let config = self.config;
        let sep = &config.separation;
        let field = self.meta.field_strength;
        Ok(match r2primenet_field_policy(field, strategy, sep.r2primenet.ignore_field_mismatch) {
            R2primenetFieldPolicy::Skip => {
                self.log(Level::Warn, &format!(
                    "Skipping R2': there is no R2 to subtract, and {}. Pass \
                     --r2primenet-ignore-field-mismatch to predict it anyway (uncalibrated).",
                    r2primenet_field_message(field)));
                self.decide(Decision::new(Topic::R2primeSource, Choice::Unavailable,
                    "There is no R2′ map: there was no R2 to subtract, and R2PRIMEnet is not run \
                     off the field strength it was trained at.").with_detail(format!("{field} T")));
                None
            }
            R2primenetFieldPolicy::Refuse => {
                return Err(crate::EngineError::Config(format!(
                    "--r2prime-strategy r2primenet: {}; or pass \
                     --r2primenet-ignore-field-mismatch to run it anyway and treat the R2' \
                     (and any chi-separation built on it) as uncalibrated.",
                    r2primenet_field_message(field))).into());
            }
            policy => {
                if policy == R2primenetFieldPolicy::RunUncalibrated {
                    self.log(Level::Warn, &format!(
                        "{}. Running anyway (--r2primenet-ignore-field-mismatch): treat this \
                         R2', and any chi-separation built on it, as uncalibrated.",
                        r2primenet_field_message(field)));
                    self.decide(Decision::new(Topic::R2primenetCalibration, Choice::Uncalibrated,
                        "R2PRIMEnet was run off the field strength it was trained at, so its R2′ is uncalibrated.")
                        .with_detail(format!("{field} T")));
                }
                if strategy == R2PrimeStrategy::Auto {
                    self.log(Level::Info, "No R2 to subtract - estimating R2' from R2* with R2PRIMEnet");
                }
                if let Some(tilt) = stages::r2::r2primenet_grid_tilt(self.meta.source_geometry.is_some(), &self.meta.affine) {
                    self.log(Level::Warn, &format!(
                        "R2PRIMEnet assumes B0 lies along +z, but this run is reconstructing on its \
                         acquired grid with B0 {tilt:.1}° from the slice normal, so its R2' is \
                         off-distribution. The run was not resampled because a measured R2 was \
                         expected and did not materialise. Pass --axial-resampling to resample, \
                         or --r2prime-strategy mese to not estimate R2' at all."));
                    self.decide(Decision::new(Topic::R2primenetGrid, Choice::OffDistribution,
                        "R2PRIMEnet ran on a grid whose B0 is not along the slice normal, so its R2′ is \
                         off its training distribution.").with_detail(format!("{tilt:.1}°")));
                }
                self.store.models(&step.models)?;
                let r2star = self.store.real(&Item::R2star)?;
                let predicted = stages::r2::r2primenet(&r2star, mask, &self.meta, &sep.r2primenet, self.observer)?;
                self.log(Level::Info, "R2' estimated by R2PRIMEnet (an estimate, not a measurement)");
                self.decide(Decision::new(Topic::R2primeSource, Choice::Predicted,
                    "R2′ was estimated from R2* by R2PRIMEnet."));
                source.insert("R2primeSource".into(), json!("predicted"));
                source.insert("R2primeMethod".into(), json!("R2PRIMEnet (from R2*)"));
                source.insert("R2primenetTrainedFieldStrength".into(),
                              json!(qsm_core::relaxometry::TRAINED_B0_T));
                source.insert("R2primenetCalibrated".into(), json!(policy == R2primenetFieldPolicy::Run));
                // χ-sepnet's toolbox route makes its own, in its training frame.
                if crate::config::chisepnet_route_can_estimate_r2prime(config) {
                    source.insert("R2primeUsedByChiSeparation".into(), json!(false));
                }
                Some(predicted)
            }
        })
    }
}
