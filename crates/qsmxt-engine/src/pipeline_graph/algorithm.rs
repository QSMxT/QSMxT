//! The algorithm each step runs and its settings ([`Ctx::algorithm`]), and the setting that asked
//! for it ([`Ctx::enabled_by`]).

use super::Ctx;
use crate::enums::*;
use crate::methods::words;
use crate::plan::{Pass, Stage};
use crate::requirements::{self, Output};

/// A step's algorithm, as the observer reports it and by its name in the methods text, and its
/// settings in the methods text's words.
type Described = ((Option<String>, Option<String>), Vec<String>);

/// An algorithm's id and its name in the methods text.
fn some(id: &str, label: &str) -> (Option<String>, Option<String>) {
    (Some(id.to_string()), Some(label.to_string()))
}

impl Ctx<'_> {
    /// The algorithm a step runs, as the observer reports it, and its name and settings in the
    /// methods text's words.
    pub(super) fn algorithm(&self, stage: Stage) -> (Option<String>, Option<String>, Vec<String>) {
        let c = self.config;
        let ((id, label), settings) = match stage {
            Stage::ScalePhase if self.facts.n_coils > 1 => (some("mcpc3ds", "MCPC-3D-S"), vec![]),
            Stage::InhomogeneityCorrection => self.homogeneity_algorithm(),
            Stage::Magnitude => {
                let s = if self.plan.homogeneity.echoes.is_empty() { vec![] } else { vec!["of the corrected echoes".to_string()] };
                (some("rss", "root sum of squares"), s)
            }
            Stage::Mask(pass) => self.mask_algorithm(pass),
            Stage::Motion => ((None, None), words::registration_settings(c)),
            Stage::FieldMap => self.field_map_algorithm(),
            Stage::BackgroundRemoval(pass) => {
                let alg = c.bg_removal.algorithm;
                let mut s = words::bg_removal_settings(c);
                if pass == Pass::Reliable && c.masking.two_pass_mode == crate::enums::TwoPassMode::Local && alg != BfAlgorithm::Iqfm {
                    s.push("LBV in the eroded rim around the holes".to_string());
                }
                ((Some(alg.to_string()), Some(words::bg_removal(alg))), s)
            }
            Stage::Inversion(_) => match crate::stages::inversion::iqsm_algorithm(c) {
                Some(id) => ((Some(id.to_string()), Some(words::inversion(c.inversion.algorithm))), vec![]),
                None => {
                    let alg = c.inversion.algorithm;
                    let mut s = words::inversion_settings(c, alg);
                    // HEIDI after the inversion; a two-pass run's fills the combined map.
                    if c.inversion.heidi.enabled && !self.plan.two_pass {
                        s.push("then HEIDI".into());
                    }
                    ((Some(alg.to_string()), Some(words::inversion(alg))), s)
                }
            },
            Stage::TwoPassCombination => {
                let s = if c.masking.two_pass_mode == crate::enums::TwoPassMode::Local {
                    vec!["second pass within 6-12 mm of its holes".to_string()]
                } else {
                    vec![]
                };
                if c.inversion.heidi.enabled { (some("heidi", "then HEIDI"), s) } else { ((None, None), s) }
            }
            Stage::Tgv(_) => (some("tgv", "TGV"), words::inversion_settings(c, QsmAlgorithm::Tgv)),
            Stage::Qsmart(_) => {
                let mut s = vec![format!("{} inversion", words::inversion(c.inversion.qsmart.inversion))];
                s.extend(words::qsmart_settings(c));
                (some("qsmart", "QSMART"), s)
            }
            Stage::Reference | Stage::SinglePassReference => {
                let label = match c.qsm.reference {
                    QsmReference::Mean => "mean".to_string(),
                    QsmReference::None => "none".to_string(),
                    QsmReference::Region => format!("region: {}", c.qsm.reference_region.as_deref().unwrap_or_default()),
                };
                ((Some(c.qsm.reference.to_string()), Some(label)), vec![])
            }
            Stage::Swi => (some("clear-swi", "CLEAR-SWI"), vec![]),
            Stage::Smwi => ((Some("smwi".into()), None), vec![]),
            Stage::Relaxometry => (some("arlo", "ARLO"), vec![]),
            Stage::R2 => {
                let label = if self.plan.steps.iter().any(|s| s.stage == Stage::R2 && !s.models.is_empty()) {
                    "EPG fit; R2PRIMEnet"
                } else {
                    "EPG fit"
                };
                (some("epg", label), vec![])
            }
            Stage::ChiSeparation => {
                let alg = c.separation.algorithm;
                ((Some(alg.to_string()), Some(words::separation(alg))), words::separation_settings(c))
            }
            Stage::Segmentation => {
                let label = format!("SynthSeg {}", c.segmentation.version);
                let s = if self.supplied_dseg() {
                    vec![format!("supplied ({})", c.segmentation.custom_dseg_tool.as_deref().unwrap_or_default())]
                } else {
                    vec![]
                };
                ((Some("synthseg".into()), Some(label)), s)
            }
            Stage::TukeyWindow => ((None, None), vec![format!("taper ratio {}", c.pipeline.tukey_window)]),
            Stage::Load | Stage::ScalePhase | Stage::ResampleAxial | Stage::Analysis
            | Stage::OutputSpace => ((None, None), vec![]),
        };
        (id, label, settings)
    }

    /// [`Self::algorithm`] of the inhomogeneity correction.
    fn homogeneity_algorithm(&self) -> Described {
        let c = self.config;
        let t = &self.plan.homogeneity;
        let mut s = Vec::new();
        let method = if t.echoes.is_empty() {
            some("n4", "N4")
        } else {
            s.push(words::echoes(&t.echoes, self.n()));
            if c.masking.inhomogeneity_method == InhomogeneityMethod::N4 && c.masking.n4_geometry == N4Geometry::MakeNii {
                s.push("make_nii geometry".to_string());
            }
            match c.masking.inhomogeneity_method {
                InhomogeneityMethod::Makehomogeneous => some("makehomogeneous", "makehomogeneous"),
                InhomogeneityMethod::N4 => some("n4", "N4"),
            }
        };
        if t.vessel {
            s.push("echo mean, N4 as QSMART does (vessel detection)".to_string());
        }
        (method, s)
    }

    /// [`Self::algorithm`] of a pass's mask.
    fn mask_algorithm(&self, pass: Pass) -> Described {
        let c = self.config;
        let sections = match pass {
            Pass::Main => c.masking.sections.clone(),
            Pass::Reliable => c.masking.resolved_two_pass_sections(),
        };
        let s = if pass == Pass::Main && self.supplied_mask() {
            vec![format!("supplied ({})", c.masking.custom_mask_tool.as_deref().unwrap_or_default())]
        } else {
            let mut lines = words::mask_sections(&sections);
            if sections.len() > 1 {
                let combine = match pass {
                    Pass::Main => c.masking.combine,
                    Pass::Reliable => c.masking.resolved_two_pass_combine(),
                };
                lines.push(format!("sections combined by {combine}"));
            }
            if pass == Pass::Reliable && c.masking.two_pass_mode == crate::enums::TwoPassMode::Local {
                lines.push("holes kept only at strong sources".to_string());
            }
            lines
        };
        ((None, None), s)
    }

    /// [`Self::algorithm`] of the field map.
    fn field_map_algorithm(&self) -> Described {
        let c = self.config;
        let fm = &c.field_mapping;
        // MEDI's complex fit fits its own phase offset (crate::stages::field).
        let per_echo = self.per_echo();
        let removes_offsets = self.n() > 1 && !per_echo && fm.phase_offset_removal
            && fm.b0_estimation != B0Estimation::ComplexFit;
        let id = if removes_offsets { "phase_offset_removal".to_string() } else { fm.unwrapping_algorithm.to_string() };
        let label = match fm.unwrapping_algorithm {
            UnwrappingAlgorithm::Romeo => "ROMEO",
            UnwrappingAlgorithm::Laplacian => "Laplacian",
            UnwrappingAlgorithm::Prelude => "PRELUDE",
            UnwrappingAlgorithm::MediRegionGrowing => "MEDI region growing",
        };
        let mut s = Vec::new();
        if removes_offsets {
            s.push("phase offsets removed".to_string());
        }
        s.extend(words::unwrapping_settings(c));
        if per_echo {
            s.push("each echo's field on its own".to_string());
        } else if self.n() > 1 {
            s.push(format!("B0 by {}", match fm.b0_estimation {
                B0Estimation::WeightedAvg => "weighted average",
                B0Estimation::LinearFit => "linear fit",
                B0Estimation::RobustFit => "robust fit",
                B0Estimation::QsmartEchofit => "QSMART's echo fit",
                B0Estimation::ComplexFit => "complex fit",
            }));
        }
        ((Some(id), Some(label.to_string())), s)
    }

    /// The setting that asked for `stage`, for a step that runs only because one did.
    pub(super) fn enabled_by(&self, stage: Stage) -> Option<String> {
        let wanted = |o: Output| requirements::wanted(self.facts, self.config, o);
        let first = |outputs: &[Output]| outputs.iter().copied().find(|&o| wanted(o)).map(|o| o.setting().to_string());
        match stage {
            Stage::Swi => first(&[Output::Swi]),
            Stage::Smwi => first(&[Output::Smwi]),
            Stage::Segmentation => first(&[Output::Segmentation]),
            Stage::Relaxometry => first(&[Output::T2star, Output::R2star]),
            // Planned when either is asked for; the data decides which it can make.
            Stage::R2 => first(&[Output::R2, Output::R2prime])
                .or_else(|| Some(if self.config.pipeline.do_r2map { Output::R2 } else { Output::R2prime }.setting().to_string())),
            Stage::ChiSeparation => Some(Output::ChiSeparation.setting().to_string()),
            Stage::Analysis => Some(Output::Analysis.setting().to_string()),
            Stage::Motion => Some("motion_correction.enabled".into()),
            Stage::TukeyWindow => Some("pipeline.tukey_window".into()),
            // Asked for by the setting, or needed by an algorithm that only understands axial data.
            Stage::ResampleAxial if self.config.pipeline.axial_resampling => Some("pipeline.axial_resampling".into()),
            Stage::Mask(Pass::Reliable) | Stage::BackgroundRemoval(Pass::Reliable) | Stage::Inversion(Pass::Reliable)
            | Stage::Tgv(Pass::Reliable) | Stage::Qsmart(Pass::Reliable) | Stage::TwoPassCombination
            | Stage::SinglePassReference => Some("masking.two_pass".into()),
            _ => None,
        }
    }
}
