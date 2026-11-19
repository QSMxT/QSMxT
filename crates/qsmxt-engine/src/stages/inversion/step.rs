//! The inversion steps of a run ([`crate::run`]): the dipole inversion of either pass (classical,
//! iQSM or TGV), the two passes combined, and the raw map referenced into the final QSM
//! ([`super`], [`crate::referencing`]).

use log::Level;
use serde_json::{json, Value};

use crate::config::PipelineConfig;
use crate::plan::{Pass, Stage, Step};
use crate::run::{Done, Run, pass_label, medi_csf_decision};
use crate::stages;
use crate::store::{Data, Item, StageStore};

impl<S: StageStore + ?Sized> Run<'_, S> {
    /// The magnitude the configured inversion takes
    /// ([`stages::inversion::magnitude_input`]).
    pub(crate) fn inversion_magnitude(&mut self) -> Result<Option<Vec<f64>>, S::Error> {
        match stages::inversion::magnitude_input(self.config) {
            Some(stages::inversion::MagnitudeInput::Combined) => self.combined_magnitude(),
            Some(stages::inversion::MagnitudeInput::Rss) => {
                let (nx, ny, nz) = self.meta.dims;
                let n = nx * ny * nz;
                Ok(self.echo_magnitude_stack()?.map(|stack| {
                    let echoes: Vec<&[f64]> = stack.chunks(n).collect();
                    qsm_core::pipeline::rss_combine(&echoes).unwrap_or_default()
                }))
            }
            Some(stages::inversion::MagnitudeInput::Echoes) => self.echo_magnitude_stack(),
            None => Ok(None),
        }
    }

    /// Dipole inversion for one pass, of the local field background removal left — or, for the
    /// total-field methods (TFI, AutoQSM, NeXtQSM), which remove or model the
    /// background themselves, of the total field inside the pass's mask.
    pub(crate) fn inversion(&mut self, step: &Step, pass: Pass) -> Result<(), S::Error> {
        let name = step.stage.name();
        let label = pass_label(pass);
        // A two-pass run inverts each pass without HEIDI, and fills the combined map once
        // ([`Self::two_pass_combination`]).
        let config = stages::inversion::pass_config(self.config, self.plan.two_pass);
        let config: &PipelineConfig = &config;
        let skip_bgremove = crate::bridge::skips_bgremove(self.config);
        if skip_bgremove {
            self.log(Level::Info, "Skipping background removal (single-step inversion handles it internally)");
        }
        let algorithm = format!("{}", self.config.inversion.algorithm);
        let per_echo = self.per_echo();
        // A per-echo QSM inverts each echo as a single-echo scan's.
        if let Some(d) = medi_csf_decision(self.config, if per_echo { 1 } else { self.n_echoes() }) {
            self.decide(d);
        }
        let mut params = stages::inversion::params(config);
        // The echo time the field is converted to radians at, as the scan's echo times resolve it
        // (each echo's own, for a per-echo QSM).
        if per_echo {
            let tes: Vec<Option<f64>> = (1..=self.n_echoes())
                .map(|e| stages::inversion::resolved_te(self.config, &self.echo_meta(e))).collect();
            params["per_echo"] = json!(true);
            if tes.iter().any(Option::is_some) {
                params["te_s"] = json!(tes);
            }
        } else if let Some(te) = stages::inversion::resolved_te(self.config, &self.meta) {
            params["te_s"] = serde_json::json!(te);
        }
        if !per_echo {
            if let Some(te) = stages::inversion::resolved_heidi_te(config, &self.meta) {
                params["heidi_te_s"] = serde_json::json!(te);
            }
        }
        let Some(key) = self.begin(&name, Some(&algorithm), &params, "Skipping invert (cached)",
                                   &format!("Dipole inversion{label}")) else { return Ok(()) };
        if per_echo {
            // Read only by the total-field methods: the others each take their echo's own mask.
            let mask = if skip_bgremove { Some(self.store.mask(&self.pass_mask(pass))?) } else { None };
            self.store.models(&step.models)?;
            let n = self.n_echoes();
            let mut outputs = Vec::with_capacity(n);
            for e in 1..=n {
                // The total-field methods take the echo's field inside the mask, the others its
                // local field inside the mask background removal left.
                let (field, mask) = if let Some(mask) = &mask {
                    (self.store.real(&Item::EchoField(e))?, mask.clone())
                } else {
                    (self.store.real(&Item::EchoLocalField(e))?, self.store.mask(&Item::EchoLocalFieldMask(e))?)
                };
                let magnitude = match stages::inversion::magnitude_input(self.config) {
                    Some(_) => self.one_echo_magnitude(e)?,
                    None => None,
                };
                if e == 1 {
                    if let Some(d) = self.reconstruction_box(&mask) {
                        self.decide(d);
                    }
                }
                self.log(Level::Info, &format!("Dipole inversion ({algorithm}), echo {e} of {n}"));
                let chi = stages::inversion::invert(&field, &mask, magnitude.as_deref(), &self.echo_meta(e), config, self.observer)?;
                self.put(Item::EchoRawChi(e), Data::Real(&chi))?;
                outputs.push(Item::EchoRawChi(e));
            }
            return self.complete(Done {
                step: &name, algorithm: Some(&algorithm), params: &params, outcome: None, key: &key, outputs: &outputs,
            }, &format!("Dipole inversion ({algorithm}), each echo"));
        }
        let (field_item, mask_item) = if skip_bgremove {
            (Item::FieldPpm, self.pass_mask(pass))
        } else {
            (Item::LocalField(pass), Item::LocalFieldMask(pass))
        };
        let field = self.store.real(&field_item)?;
        let mask = self.store.mask(&mask_item)?;
        // The combined magnitude, or every echo's for MEDI and TFI.
        let magnitude = self.inversion_magnitude()?;
        self.store.models(&step.models)?;
        if let Some(d) = self.reconstruction_box(&mask) {
            self.decide(d);
        }
        self.log(Level::Info, &format!("Dipole inversion ({algorithm}){label}"));
        let chi = stages::inversion::invert(&field, &mask, magnitude.as_deref(), &self.meta, config, self.observer)?;
        self.put(Item::RawChi(pass), Data::Real(&chi))?;
        self.complete(Done {
            step: &name, algorithm: Some(&algorithm), params: &params, outcome: None, key: &key,
            outputs: &[Item::RawChi(pass)],
        }, &format!("Dipole inversion ({algorithm}){label}"))
    }

    /// iQSM / iQSM+: the raw map straight from every echo's wrapped phase.
    pub(crate) fn iqsm(&mut self, step: &Step, pass: Pass) -> Result<(), S::Error> {
        let name = step.stage.name();
        let label = pass_label(pass);
        let algorithm = stages::inversion::iqsm_algorithm(self.config).unwrap_or_default();
        let params = stages::inversion::iqsm_params(&self.meta);
        let Some(key) = self.begin(&name, Some(algorithm), &params, &format!("Skipping {algorithm} (cached)"),
                                   &format!("iQSM reconstruction{label}")) else { return Ok(()) };
        let (phases, magnitudes) = self.phase_echoes()?;
        let mask_item = self.pass_mask(pass);
        let mask = self.store.mask(&mask_item)?;
        let phase_refs: Vec<&[f64]> = phases.iter().map(|p| p.as_slice()).collect();
        let mag_refs: Vec<&[f64]> = magnitudes.iter().map(|m| m.as_slice()).collect();
        self.store.models(&step.models)?;
        let chi = stages::inversion::iqsm(&phase_refs, &mag_refs, &mask, &self.meta, self.config)?;
        self.put(Item::RawChi(pass), Data::Real(&chi))?;
        let net = if algorithm == "iqsm-plus" { "iQSM+" } else { "iQSM" };
        self.complete(Done {
            step: &name, algorithm: Some(algorithm), params: &params, outcome: None, key: &key,
            outputs: &[Item::RawChi(pass)],
        }, &format!("{net}{label}"))
    }

    /// TGV-QSM, from the total field (several echoes) or echo 1's wrapped phase (one).
    pub(crate) fn tgv(&mut self, pass: Pass) -> Result<(), S::Error> {
        use stages::inversion::TgvSource;
        let name = Stage::Tgv(pass).name();
        let label = pass_label(pass);
        let params = stages::inversion::tgv_params(self.config, &self.meta);
        let Some(key) = self.begin(&name, Some("tgv"), &params, "Skipping tgv (cached)",
                                   &format!("TGV-QSM reconstruction{label}")) else { return Ok(()) };
        let mask_item = self.pass_mask(pass);
        let mask = self.store.mask(&mask_item)?;
        let data = match stages::inversion::tgv_source(&self.meta) {
            TgvSource::FieldPpm => self.store.real(&Item::FieldPpm)?,
            TgvSource::WrappedPhase => { let i = self.echo_phase(1); self.store.real(&i)? }
        };
        let chi = stages::inversion::tgv(&data, &mask, &self.meta, self.config)?;
        self.put(Item::RawChi(pass), Data::Real(&chi))?;
        self.complete(Done {
            step: &name, algorithm: Some("tgv"), params: &params, outcome: None, key: &key,
            outputs: &[Item::RawChi(pass)],
        }, &format!("TGV-QSM{label}"))
    }

    /// The two passes' raw maps combined into one ([`stages::inversion::combine_two_pass`]), and
    /// HEIDI's cone fill on it when HEIDI is on ([`stages::inversion::heidi_cone_fill`], with the
    /// main pass's local field and mask).
    pub(crate) fn two_pass_combination(&mut self) -> Result<(), S::Error> {
        let support = self.support(Pass::Reliable);
        let mut params = json!({ "support": self.store.identity(&support) });
        if self.config.masking.two_pass_mode == crate::enums::TwoPassMode::Local {
            params["merge"] = json!("second pass within 6 mm of its holes, feathered to 12 mm");
        }
        let heidi = stages::inversion::heidi_params(self.config);
        if let Some(h) = &heidi {
            params["heidi"] = h.clone();
            if let Some(te) = stages::inversion::resolved_heidi_te(self.config, &self.meta) {
                params["heidi"]["te_s"] = json!(te);
            }
        }
        let doing = if heidi.is_some() { "Combining two-pass reconstructions, then HEIDI" } else { "Combining two-pass reconstructions" };
        let Some(key) = self.begin("twopass", None, &params, "Skipping twopass (cached)", doing) else { return Ok(()) };
        let chi_reliable = self.store.real(&Item::RawChi(Pass::Reliable))?;
        let chi_main = self.store.real(&Item::RawChi(Pass::Main))?;
        let declared = self.store.mask(&support)?;
        let brain_item = self.pass_mask(Pass::Main);
        let brain = self.store.mask(&brain_item)?;
        let mut combined = if self.config.masking.two_pass_mode == crate::enums::TwoPassMode::Local {
            // The second pass only near its holes, feathered into the first.
            let reliable_mask = self.store.mask(&Item::ReliableMask)?;
            let main_support_item = self.support(Pass::Main);
            let main_support = self.store.mask(&main_support_item)?;
            stages::inversion::combine_two_pass_local(&chi_reliable, &chi_main, &declared, &reliable_mask, &brain,
                                                      &main_support, &self.meta)?
        } else {
            stages::inversion::combine_two_pass(&chi_reliable, &chi_main, &declared, &brain)?
        };
        drop((chi_reliable, chi_main, declared, brain));
        if heidi.is_some() {
            let (field, mask) = (self.store.real(&Item::LocalField(Pass::Main))?, self.store.mask(&Item::LocalFieldMask(Pass::Main))?);
            self.log(Level::Info, "HEIDI on the combined two-pass map");
            combined = stages::inversion::heidi_cone_fill(&combined, &field, &mask, &self.meta, self.config, self.observer)?;
        }
        self.put(Item::CombinedRawChi, Data::Real(&combined))?;
        self.complete(Done {
            step: "twopass", algorithm: heidi.is_some().then_some("heidi"), params: &params, outcome: None, key: &key,
            outputs: &[Item::CombinedRawChi],
        }, if heidi.is_some() { "Two-pass combination and HEIDI" } else { "Two-pass combination" })
    }

    /// Reference a raw map over the voxels it is defined on ([`crate::referencing`]): the final
    /// map, or (`single_pass`) a two-pass run's single-pass map. The single-pass map's support is
    /// the main pass's; the combined map takes each voxel from one pass or the other, so its
    /// support is the union of both.
    pub(crate) fn reference(&mut self, single_pass: bool) -> Result<(), S::Error> {
        let stage = if single_pass { Stage::SinglePassReference } else { Stage::Reference };
        let name = stage.name();
        let method = format!("{}", self.config.qsm.reference);
        let segmentation = self.store.recorded_key("segmentation");
        let params = crate::referencing::params(self.config, segmentation.as_deref())?;
        let region = crate::referencing::reference_region(self.config)?;
        let Some(key) = self.begin(&name, Some(&method), &params, &format!("Skipping {name} (cached)"),
                                   "Referencing QSM") else { return Ok(()) };
        self.log(Level::Info, &format!("QSM referencing ({})", self.config.qsm.reference));
        let dseg = match region {
            Some((spec, _)) => {
                let Some(segmentation) = self.segmentation_item() else {
                    return Err(crate::EngineError::Config(format!(
                        "--qsm-reference {spec}: no segmentation to measure the region on. Enable \
                         --do-segmentation, or supply one with --use-custom-dseg")).into());
                };
                Some(self.map(&segmentation)?)
            }
            None => None,
        };
        if !single_pass && self.per_echo() {
            return self.reference_per_echo(&name, &method, &params, &key, dseg.as_deref());
        }
        let (raw, supports, out) = if single_pass {
            (Item::RawChi(Pass::Main), vec![self.support(Pass::Main)], Item::SinglePassChi)
        } else if self.plan.two_pass {
            (Item::CombinedRawChi, vec![self.support(Pass::Main), self.support(Pass::Reliable)], Item::Chi)
        } else {
            (Item::RawChi(Pass::Main), vec![self.support(Pass::Main)], Item::Chi)
        };
        let chi = self.store.real(&raw)?;
        let declared = supports.iter().map(|s| self.store.mask(s)).collect::<Result<Vec<_>, _>>()?;
        let declared_refs: Vec<&[u8]> = declared.iter().map(|m| m.as_slice()).collect();
        let crate::referencing::Referenced { chi, support, offset } =
            crate::referencing::reference_chi(self.config, &chi, &declared_refs, dseg.as_deref())?;
        self.put(out, Data::Real(&chi))?;
        let mut outputs = vec![out];
        if !single_pass {
            self.put(Item::ChiSupport, Data::Mask(&support))?;
            outputs.push(Item::ChiSupport);
        }
        // What the map was referenced to, recorded with the map itself: each map is referenced
        // over its own support, so each has its own offset.
        self.store.annotate(&out, crate::referencing::sidecar_fields(self.config, offset.as_ref()))?;
        self.complete(Done {
            step: &name, algorithm: Some(&method), params: &params, outcome: None, key: &key, outputs: &outputs,
        }, "QSM referencing")
    }

    /// A per-echo QSM's referencing: each echo's map over the voxels it is defined on (published
    /// with its echo time), and the QSM, the mean of the echoes' maps (qsm-core
    /// `per_echo::mean_across_echoes`, equal weights) where every echo's is defined, referenced
    /// as a QSM is. Referencing commutes with the mean over one support, so the QSM is also the
    /// mean of the referenced echoes' maps there.
    pub(crate) fn reference_per_echo(
        &mut self, name: &str, method: &str, params: &Value, key: &str, dseg: Option<&[f64]>,
    ) -> Result<(), S::Error> {
        let n = self.n_echoes();
        let skip_bgremove = crate::bridge::skips_bgremove(self.config);
        let mut raws = Vec::with_capacity(n);
        let mut common: Option<Vec<u8>> = None;
        let mut outputs = Vec::with_capacity(n + 2);
        for e in 1..=n {
            let raw = self.store.real(&Item::EchoRawChi(e))?;
            let declared_item = if skip_bgremove { self.pass_mask(Pass::Main) } else { Item::EchoLocalFieldMask(e) };
            let declared = self.store.mask(&declared_item)?;
            let crate::referencing::Referenced { chi, support, offset } =
                crate::referencing::reference_chi(self.config, &raw, &[&declared], dseg)?;
            self.put(Item::EchoChi(e), Data::Real(&chi))?;
            let mut fields = crate::referencing::sidecar_fields(self.config, offset.as_ref());
            fields.insert("EchoTime".into(), json!(self.meta.echo_times[e - 1]));
            self.store.annotate(&Item::EchoChi(e), fields)?;
            outputs.push(Item::EchoChi(e));
            common = Some(match common {
                None => support,
                Some(c) => c.iter().zip(&support).map(|(&a, &b)| u8::from(a != 0 && b != 0)).collect(),
            });
            raws.push(raw);
        }
        let common = common.unwrap_or_default();
        let mut mean = qsm_core::pipeline::per_echo::mean_across_echoes(&raws);
        drop(raws);
        for (m, &c) in mean.iter_mut().zip(&common) {
            if c == 0 {
                *m = 0.0;
            }
        }
        self.log(Level::Info, &format!("QSM: the mean of {n} echoes' maps"));
        let crate::referencing::Referenced { chi, support, offset } =
            crate::referencing::reference_chi(self.config, &mean, &[&common], dseg)?;
        self.put(Item::Chi, Data::Real(&chi))?;
        self.put(Item::ChiSupport, Data::Mask(&support))?;
        self.store.annotate(&Item::Chi, crate::referencing::sidecar_fields(self.config, offset.as_ref()))?;
        outputs.extend([Item::Chi, Item::ChiSupport]);
        self.complete(Done {
            step: name, algorithm: Some(method), params, outcome: None, key, outputs: &outputs,
        }, "QSM referencing")
    }
}
