//! The steps of a recipe: their labels, the values offered for them, and editing those values.

use super::*;

impl MaskRecipe {
    pub fn mask_op_label_value(op: &MaskOp) -> (&'static str, String) {
        match op {
            MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. } => ("threshold", "otsu".to_string()),
            MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value } =>
                ("threshold", format!("fixed:{}", value.unwrap_or(0.5))),
            MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value } =>
                ("threshold", format!("percentile:{}", value.unwrap_or(75.0))),
            MaskOp::Threshold { method: MaskThresholdMethod::StiGui, .. } =>
                ("threshold", "sti-gui".to_string()),
            MaskOp::Bet { fractional_intensity, voxel_scale } => ("bet", format!("{}{}", fractional_intensity,
                if (*voxel_scale - qsmxt_engine::bet_default_voxel_scale()).abs() > f64::EPSILON {
                    format!(" scale={voxel_scale}")
                } else {
                    String::new()
                })),
            MaskOp::Erode { iterations } => ("erode", format!("{}", iterations)),
            MaskOp::Dilate { iterations } => ("dilate", format!("{}", iterations)),
            MaskOp::Close { radius } => ("close", format!("{}", radius)),
            MaskOp::MinIntensity { value } => ("min-intensity", format!("{}", value)),
            MaskOp::Majority => ("majority", "3×3×3".to_string()),
            MaskOp::LargestComponent => ("largest-component", "26-connected".to_string()),
            MaskOp::Imfill => ("imfill", "all holes".to_string()),
            MaskOp::FillHoles { max_size } => ("fill-holes", if *max_size == 0 { "auto".to_string() } else { format!("{}", max_size) }),
            MaskOp::GaussianSmooth { sigma_mm } => ("gaussian", format!("{}", sigma_mm)),
            MaskOp::HdBet { patch, tta, tile_step } => {
                let step = if patch.is_some()
                    && (*tile_step - qsmxt_engine::hd_bet_default_tile_step()).abs() > f64::EPSILON {
                    format!(" step={tile_step}")
                } else {
                    String::new()
                };
                let inference = match patch {
                    None => "full".to_string(),
                    Some(p) => format!("{}x{}x{}", p[0], p[1], p[2]),
                };
                ("hd-bet", format!("{inference}{step}{}", if *tta { " tta" } else { "" }))
            }
            MaskOp::Rs2Net { tta, tile_step } => {
                let step = if (*tile_step - qsmxt_engine::rs2_net_default_tile_step()).abs() > f64::EPSILON {
                    format!(" step={tile_step}")
                } else {
                    String::new()
                };
                ("rs2-net", format!("rodent{}{}", step, if *tta { " tta" } else { "" }))
            }
            MaskOp::MindGrab { tta } => ("mindgrab", if *tta { "flip tta" } else { "default" }.to_string()),
            // Only the threshold: every other parameter has a row of its own below.
            MaskOp::SignalErode { threshold, .. } => ("signal-erode", format!("{:.2}", threshold)),
        }
    }

    /// Help for a mask step's row: what the value it shows does (the mask schema's wording),
    /// and how to change it.
    pub fn mask_op_help(op: &MaskOp) -> String {
        let step = mask_step_of(op);
        match op {
            MaskOp::Threshold { .. } => format!("{} — Enter: choose method", step.help),
            MaskOp::HdBet { .. } | MaskOp::Rs2Net { .. } | MaskOp::MindGrab { .. } =>
                format!("{} (needs a deep-learning build)", step.help),
            // Nothing to adjust.
            MaskOp::Majority | MaskOp::LargestComponent | MaskOp::Imfill => step.help.to_string(),
            // The row shows the step's first parameter (signal-erode's threshold); the rest have
            // rows of their own.
            _ => format!("{} (Enter to edit, +/- to nudge)", step.params.first().map_or(step.help, |p| p.help)),
        }
    }

    /// Create a default mask op for the given type name: the mask schema's default for the
    /// first step type of that name (a threshold is Otsu's).
    pub fn default_mask_op(type_name: &str) -> Option<MaskOp> {
        let step = mask_schema().steps.iter().find(|s| s.label == type_name)?;
        serde_json::from_value(step.default.clone()).ok()
    }

    /// Index of a section's generator in [`MASK_GENERATOR_TYPES`].
    pub fn mask_generator_index(&self, section: usize) -> usize {
        self.mask_section(section)
            .and_then(|s| MASK_GENERATOR_TYPES.iter().position(|&t| t == Self::mask_op_label_value(&s.generator).0))
            .unwrap_or(0)
    }

    /// HD-BET's sliding-window steps worth offering. qsm-core accepts any stride in `(0, 1]`:
    /// 0.5 is HD-BET's own default, and larger steps trade patch-seam quality for a shorter run.
    pub const HD_BET_STEPS: [f64; 4] = [0.5, 0.625, 0.75, 1.0];

    /// BET's voxel-size scalings worth offering. Any positive factor is valid, but only two
    /// matter in practice: 1.0 for clinical data at its own geometry, and 10.0 for rodent brains,
    /// which BET's millimetre-scale surface model otherwise cannot find. The steps between are
    /// there for the in-between field strengths.
    pub const BET_VOXEL_SCALES: [f64; 4] = [1.0, 2.0, 5.0, 10.0];

    /// The values a list of offered `choices` shows for `current`: the offered ones, with
    /// `current` added in order when it is not one of them (a value from a loaded config), and
    /// the index of `current`.
    pub(super) fn value_choices(choices: &[f64], current: f64) -> (Vec<f64>, usize) {
        let mut values = choices.to_vec();
        if !values.iter().any(|v| (v - current).abs() < 1e-9) {
            values.push(current);
            values.sort_by(f64::total_cmp);
        }
        let at = values.iter().position(|v| (v - current).abs() < 1e-9).unwrap_or(0);
        (values, at)
    }

    /// The HD-BET patch steps offered for a section's generator, and the current one's index.
    pub fn hd_bet_step_choices(&self, section: usize) -> (Vec<f64>, usize) {
        let current = match self.mask_section(section).map(|s| &s.generator) {
            Some(MaskOp::HdBet { tile_step, .. }) => *tile_step,
            _ => qsmxt_engine::hd_bet_default_tile_step(),
        };
        Self::value_choices(&Self::HD_BET_STEPS, current)
    }

    /// Set a section's HD-BET patch step (no change unless its generator is HD-BET).
    pub(super) fn set_hd_bet_tile_step(&mut self, section: usize, step: f64) -> bool {
        let Some(MaskOp::HdBet { tile_step, .. }) = self.mask_section_mut(section).map(|s| &mut s.generator) else { return false };
        *tile_step = step;
        true
    }

    /// The BET voxel scalings offered for a section's generator, and the current one's index.
    pub fn bet_voxel_scale_choices(&self, section: usize) -> (Vec<f64>, usize) {
        let current = match self.mask_section(section).map(|s| &s.generator) {
            Some(MaskOp::Bet { voxel_scale, .. }) => *voxel_scale,
            _ => qsmxt_engine::bet_default_voxel_scale(),
        };
        Self::value_choices(&Self::BET_VOXEL_SCALES, current)
    }

    /// Set a section's BET voxel scaling (no change unless its generator is BET).
    pub(super) fn set_bet_voxel_scale(&mut self, section: usize, scale: f64) -> bool {
        let Some(MaskOp::Bet { voxel_scale, .. }) = self.mask_section_mut(section).map(|s| &mut s.generator) else { return false };
        *voxel_scale = scale;
        true
    }

    /// HD-BET's inference choices (whole volume, native sliding window, low-memory window), in
    /// the order of the mask schema's `hd_bet_inference`.
    pub fn hd_bet_inference_patches() -> [Option<[usize; 3]>; 3] {
        [None, Some(MaskOp::hd_bet_default_patch()), Some(qsmxt_engine::hd_bet_low_memory_patch())]
    }

    /// The index of a section's HD-BET inference in [`Self::hd_bet_inference_patches`].
    pub fn hd_bet_inference_index(&self, section: usize) -> usize {
        match self.mask_section(section).map(|s| &s.generator) {
            Some(MaskOp::HdBet { patch, .. }) => Self::hd_bet_inference_patches().iter().position(|p| p == patch).unwrap_or(0),
            _ => 0,
        }
    }

    /// Set a section's HD-BET inference by index into [`Self::hd_bet_inference_patches`].
    pub(super) fn set_hd_bet_inference(&mut self, section: usize, idx: usize) -> bool {
        let Some(&new) = Self::hd_bet_inference_patches().get(idx) else { return false };
        let Some(MaskOp::HdBet { patch, .. }) = self.mask_section_mut(section).map(|s| &mut s.generator) else { return false };
        *patch = new;
        true
    }

    /// Turn a section's MindGrab test-time augmentation on or off (no change for another
    /// generator).
    pub(super) fn toggle_mindgrab_tta(&mut self, section: usize) -> bool {
        let Some(MaskOp::MindGrab { tta }) = self.mask_section_mut(section).map(|s| &mut s.generator) else { return false };
        *tta = !*tta;
        true
    }

    /// Ordered masking input sources, matching the modal/cycle option order.
    pub const MASK_INPUT_SOURCES: [MaskingInput; 4] = {
        use MaskingInput::*;
        [MagnitudeFirst, Magnitude, MagnitudeLast, PhaseQuality]
    };

    /// Current masking-input source index for a section (for populating the modal).
    pub fn mask_input_index(&self, section: usize) -> usize {
        self.mask_section(section)
            .and_then(|s| Self::MASK_INPUT_SOURCES.iter().position(|x| *x == s.input))
            .unwrap_or(0)
    }

    /// Set a section's masking input source by index (from the modal).
    pub(super) fn set_mask_input(&mut self, section: usize, idx: usize) -> bool {
        let Some(&src) = Self::MASK_INPUT_SOURCES.get(idx) else { return false };
        let Some(section_ref) = self.mask_section_mut(section) else { return false };
        section_ref.input = src;
        true
    }

    /// Set a section's generator algorithm by index into [`MASK_GENERATOR_TYPES`]. Only rewrites
    /// the generator when the algorithm type actually changes, preserving existing parameters.
    pub(super) fn set_mask_generator(&mut self, section: usize, idx: usize) -> bool {
        if self.mask_section(section).is_none() || idx == self.mask_generator_index(section) { return false; }
        let Some(op) = MASK_GENERATOR_TYPES.get(idx).and_then(|t| Self::default_mask_op(t)) else { return false };
        if let Some(section_ref) = self.mask_section_mut(section) {
            section_ref.generator = op;
        }
        true
    }

    /// Ordered threshold methods, matching the modal/cycle option order.
    pub const MASK_THRESHOLD_METHODS: [MaskThresholdMethod; 4] = {
        use MaskThresholdMethod::*;
        [Otsu, Fixed, Percentile, StiGui]
    };

    /// Current threshold-method index for a section (0 if the generator isn't a threshold).
    pub fn mask_threshold_method_index(&self, section: usize) -> usize {
        match self.mask_section(section).map(|s| &s.generator) {
            Some(MaskOp::Threshold { method, .. }) => {
                Self::MASK_THRESHOLD_METHODS.iter().position(|m| m == method).unwrap_or(0)
            }
            _ => 0,
        }
    }

    /// Set a section's threshold method by index (from the modal). No change unless the
    /// generator is a threshold; the existing threshold value is preserved.
    pub(super) fn set_mask_threshold_method(&mut self, section: usize, idx: usize) -> bool {
        let Some(&new_method) = Self::MASK_THRESHOLD_METHODS.get(idx) else { return false };
        let Some(MaskOp::Threshold { method, .. }) = self.mask_section_mut(section).map(|s| &mut s.generator) else { return false };
        *method = new_method;
        true
    }

    /// Set the method of a refinement step that is a threshold (from a loaded config), by index
    /// into [`Self::MASK_THRESHOLD_METHODS`].
    pub(super) fn set_refinement_threshold_method(&mut self, section: usize, index: usize, idx: usize) -> bool {
        let Some(&new_method) = Self::MASK_THRESHOLD_METHODS.get(idx) else { return false };
        let Some(MaskOp::Threshold { method, .. }) = self.mask_refinements_mut(section).and_then(|r| r.get_mut(index)) else { return false };
        *method = new_method;
        true
    }

    /// The number a mask row edits, as text to start editing from, when the row edits a number:
    /// a threshold's value, BET's fractional intensity, a refinement step's value, or one of
    /// signal-erode's settings. `None` for every other row (selectors and switches).
    pub fn mask_value_text(&self, row: &PipelineRow) -> Option<String> {
        match *row {
            PipelineRow::MaskOpThresholdValue { section } => match self.mask_section(section).map(|s| &s.generator) {
                Some(MaskOp::Threshold { value, .. }) => Some(value.map(|v| format!("{v}")).unwrap_or_default()),
                _ => None,
            },
            PipelineRow::MaskOpGeneratorParam { section } => match self.mask_section(section).map(|s| &s.generator) {
                Some(MaskOp::Bet { fractional_intensity, .. }) => Some(format!("{fractional_intensity}")),
                _ => None,
            },
            PipelineRow::MaskOpEntry { section, index } => match self.mask_refinements(section)?.get(index)? {
                MaskOp::Bet { fractional_intensity, .. } => Some(format!("{fractional_intensity}")),
                MaskOp::Erode { iterations } | MaskOp::Dilate { iterations } => Some(format!("{iterations}")),
                MaskOp::Close { radius } => Some(format!("{radius}")),
                MaskOp::MinIntensity { value } => Some(format!("{value}")),
                MaskOp::FillHoles { max_size } => Some(format!("{max_size}")),
                MaskOp::GaussianSmooth { sigma_mm } => Some(format!("{sigma_mm}")),
                MaskOp::SignalErode { threshold, .. } => Some(format!("{threshold}")),
                _ => None,
            },
            PipelineRow::MaskOpSignalErodeParam { section, index, param } => match self.mask_refinements(section)?.get(index)? {
                MaskOp::SignalErode { depth_cap, global_erosions, bias_sigma, min_component, .. } => Some(match param {
                    0 => format!("{depth_cap}"),
                    1 => format!("{global_erosions}"),
                    2 => format!("{bias_sigma}"),
                    _ => format!("{min_component}"),
                }),
                _ => None,
            },
            _ => None,
        }
    }

    /// Nudge the number a mask row edits by one step up (`delta` 1, `+`) or down (-1, `-`), within
    /// its range: BET's fractional intensity by 0.05 (0.1 for a refinement step), an iteration
    /// count or radius by 1, a minimum intensity by a tenth of its order of magnitude, the
    /// largest hole by 100, a smoothing σ by 0.5 mm, signal-erode's threshold by 0.05 and its
    /// settings by 1 (the smallest component by 100). Returns whether the row has such a number.
    pub(super) fn nudge_mask_value(&mut self, row: &PipelineRow, delta: isize) -> bool {
        let d = delta as f64;
        let step_usize = |v: &mut usize, by: isize, min: usize| *v = (*v as isize + delta * by).max(min as isize) as usize;
        match *row {
            PipelineRow::MaskOpGeneratorParam { section } => {
                let Some(MaskOp::Bet { fractional_intensity, .. }) = self.mask_section_mut(section).map(|s| &mut s.generator) else { return false };
                *fractional_intensity = ((*fractional_intensity + d * 0.05).clamp(0.05, 1.0) * 100.0).round() / 100.0;
            }
            PipelineRow::MaskOpEntry { section, index } => {
                let Some(op) = self.mask_refinements_mut(section).and_then(|r| r.get_mut(index)) else { return false };
                match op {
                    MaskOp::Bet { fractional_intensity, .. } => *fractional_intensity = ((*fractional_intensity + d * 0.1).clamp(0.1, 1.0) * 10.0).round() / 10.0,
                    MaskOp::Erode { iterations } | MaskOp::Dilate { iterations } => step_usize(iterations, 1, 1),
                    MaskOp::Close { radius } => step_usize(radius, 1, 1),
                    MaskOp::MinIntensity { value } => *value = step_min_intensity(*value, delta),
                    MaskOp::FillHoles { max_size } => step_usize(max_size, 100, 0),
                    MaskOp::GaussianSmooth { sigma_mm } => *sigma_mm = (*sigma_mm + d * 0.5).max(0.5),
                    MaskOp::SignalErode { threshold, .. } => *threshold = ((*threshold + d * 0.05).clamp(0.05, 0.95) * 100.0).round() / 100.0,
                    _ => return false,
                }
            }
            PipelineRow::MaskOpSignalErodeParam { section, index, param } => {
                let Some(MaskOp::SignalErode { depth_cap, global_erosions, bias_sigma, min_component, .. }) =
                    self.mask_refinements_mut(section).and_then(|r| r.get_mut(index)) else { return false };
                match param {
                    0 => step_usize(depth_cap, 1, 0),
                    1 => step_usize(global_erosions, 1, 0),
                    // Voxels, like qsm-core's `bias_sigma`; 0 would divide out nothing.
                    2 => *bias_sigma = (*bias_sigma + d).max(1.0),
                    _ => step_usize(min_component, 100, 0),
                }
            }
            _ => return false,
        }
        true
    }

    /// Set the number a mask row edits ([`Self::mask_value_text`]) from typed text, and whether
    /// it did. A threshold value left empty goes back to its default; any other text that is not
    /// a number in the setting's range is refused with why, and nothing changes.
    pub(super) fn set_mask_value(&mut self, row: &PipelineRow, text: &str) -> Result<bool, String> {
        let text = text.trim();
        let real = |what: &str, ok: &dyn Fn(f64) -> bool, range: &str| -> Result<f64, String> {
            text.parse::<f64>().ok().filter(|v| v.is_finite() && ok(*v))
                .ok_or_else(|| format!("{what} is a number {range}, not \"{text}\""))
        };
        let count = |what: &str, min: usize| -> Result<usize, String> {
            text.parse::<usize>().ok().filter(|v| *v >= min)
                .ok_or_else(|| format!("{what} is a whole number of at least {min}, not \"{text}\""))
        };
        let fraction = |what: &str| real(what, &|v| v > 0.0 && v <= 1.0, "above 0 and at most 1");
        match *row {
            PipelineRow::MaskOpThresholdValue { section } => {
                let value = if text.is_empty() { None } else { Some(real("The threshold", &|_| true, "")?) };
                match self.mask_section_mut(section).map(|s| &mut s.generator) {
                    Some(MaskOp::Threshold { value: v, .. }) => *v = value,
                    _ => return Ok(false),
                }
            }
            PipelineRow::MaskOpGeneratorParam { section } => {
                let v = fraction("BET's fractional intensity")?;
                match self.mask_section_mut(section).map(|s| &mut s.generator) {
                    Some(MaskOp::Bet { fractional_intensity, .. }) => *fractional_intensity = v,
                    _ => return Ok(false),
                }
            }
            PipelineRow::MaskOpEntry { section, index } => {
                let Some(op) = self.mask_refinements(section).and_then(|r| r.get(index)).cloned() else { return Ok(false) };
                let new = match op {
                    MaskOp::Bet { voxel_scale, .. } => MaskOp::Bet { fractional_intensity: fraction("BET's fractional intensity")?, voxel_scale },
                    MaskOp::Erode { .. } => MaskOp::Erode { iterations: count("Erode's iterations", 1)? },
                    MaskOp::Dilate { .. } => MaskOp::Dilate { iterations: count("Dilate's iterations", 1)? },
                    MaskOp::Close { .. } => MaskOp::Close { radius: count("Close's radius", 1)? },
                    MaskOp::MinIntensity { .. } => MaskOp::MinIntensity { value: real("The minimum intensity", &|v| v > 0.0, "above 0")? },
                    MaskOp::FillHoles { .. } => MaskOp::FillHoles { max_size: count("The largest hole (0: any size)", 0)? },
                    MaskOp::GaussianSmooth { .. } => MaskOp::GaussianSmooth { sigma_mm: real("The smoothing σ", &|v| v > 0.0, "above 0")? },
                    MaskOp::SignalErode { depth_cap, global_erosions, bias_sigma, min_component, .. } => MaskOp::SignalErode {
                        threshold: real("Signal-erode's threshold", &|v| v > 0.0 && v < 1.0, "between 0 and 1")?,
                        depth_cap, global_erosions, bias_sigma, min_component,
                    },
                    _ => return Ok(false),
                };
                if let Some(op) = self.mask_refinements_mut(section).and_then(|r| r.get_mut(index)) { *op = new; }
            }
            PipelineRow::MaskOpSignalErodeParam { section, index, param } => {
                let label = signal_erode_params().get(param).map_or("The setting", |p| p.0);
                let Some(MaskOp::SignalErode { depth_cap, global_erosions, bias_sigma, min_component, .. }) =
                    self.mask_refinements(section).and_then(|r| r.get(index)).cloned() else { return Ok(false) };
                let (mut d, mut g, mut b, mut m) = (depth_cap, global_erosions, bias_sigma, min_component);
                match param {
                    0 => d = count(label, 0)?,
                    1 => g = count(label, 0)?,
                    // Voxels, like qsm-core's `bias_sigma`; 0 would divide out nothing.
                    2 => b = real(label, &|v| v > 0.0, "above 0")?,
                    _ => m = count(label, 0)?,
                }
                if let Some(MaskOp::SignalErode { depth_cap, global_erosions, bias_sigma, min_component, .. }) =
                    self.mask_refinements_mut(section).and_then(|r| r.get_mut(index))
                {
                    (*depth_cap, *global_erosions, *bias_sigma, *min_component) = (d, g, b, m);
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Label and current value of one extra signal-erode parameter, for its own row.
    pub fn signal_erode_param(&self, section: usize, index: usize, param: usize) -> Option<(&'static str, String)> {
        let MaskOp::SignalErode { depth_cap, global_erosions, bias_sigma, min_component, .. } =
            self.mask_refinements(section)?.get(index)? else { return None };
        let (label, _) = *signal_erode_params().get(param)?;
        let value = match param {
            0 => if *depth_cap == 0 { "off (no cap)".to_string() } else { format!("{depth_cap}") },
            1 => format!("{global_erosions}"),
            2 => format!("{bias_sigma:.1}"),
            _ => format!("{min_component}"),
        };
        Some((label, value))
    }

    /// The step types offered for adding refinement steps (morphological only): a section's
    /// generator is chosen on its own row.
    pub fn available_op_types() -> Vec<&'static str> {
        MASK_OP_TYPES.iter()
            .filter(|t| !MASK_GENERATOR_TYPES.contains(t))
            .copied()
            .collect()
    }
}

/// A minimum intensity one `+`/`-` nudge on: by a tenth of its order of magnitude (100 → 110 or
/// 90, 0.5 → 0.55 or 0.45), so a magnitude in any units can be adjusted. Stays above 0.
pub(super) fn step_min_intensity(value: f64, delta: isize) -> f64 {
    // Going down from a power of ten steps by the smaller order (100 → 99, not 90 → 0 eventually).
    let base = if delta < 0 { value * (1.0 - 1e-9) } else { value };
    let step = 10f64.powf(base.max(f64::MIN_POSITIVE).log10().floor() - 1.0);
    let next = value + delta as f64 * step;
    // Round to the step, so repeated steps do not accumulate floating-point noise.
    let next = (next / step).round() * step;
    if next > 0.0 { next } else { value }
}

/// A part of a recipe as the store holds it.
pub(super) fn json<T: serde::Serialize>(v: &T) -> serde_json::Value {
    serde_json::to_value(v).unwrap_or_default()
}
