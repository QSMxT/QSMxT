use super::*;

// `orientation`: which way round every BET step sees the left-right axis: `fsl` (x reversed for a
// neurologically stored image, as FSL's `bet` reads a file) or `as-stored` (as MEDI's `BET.m`).
param_config!(BetConfig from qsm_core::bet::BetParams {
    fractional_intensity: f64, smoothness: f64, gradient_threshold: f64,
    iterations: usize, subdivisions: usize, voxel_scale: f64, orientation: BetOrientation
});
param_config!(HomogeneityConfig from qsm_core::utils::HomogeneityParams {
    sigma_mm: f64, nbox: usize
});
// N4 runs without a mask here (on the magnitude masking is made from), so qsm-core's
// `rescale_intensities`, which applies only with a mask, has nothing to do and is left out.
param_config!(N4Config from qsm_core::utils::N4Params {
    shrink_factor: usize, max_iterations: Vec<usize>, convergence_threshold: f64, mesh: [usize; 3],
    spline_order: usize, bias_field_fwhm: f64, wiener_noise: f64, histogram_bins: usize
});
impl N4Config {
    /// The qsm-core parameters.
    pub fn core(&self) -> qsm_core::utils::N4Params {
        qsm_core::utils::N4Params {
            shrink_factor: self.shrink_factor,
            max_iterations: self.max_iterations.clone(),
            convergence_threshold: self.convergence_threshold,
            mesh: self.mesh,
            spline_order: self.spline_order,
            bias_field_fwhm: self.bias_field_fwhm,
            wiener_noise: self.wiener_noise,
            histogram_bins: self.histogram_bins,
            ..Default::default()
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MaskingConfig {
    pub inhomogeneity_correction: bool,
    /// How the magnitude is corrected for inhomogeneity: `makehomogeneous` (default) or `n4`.
    #[serde(default)]
    pub inhomogeneity_method: InhomogeneityMethod,
    /// The image geometry N4 is given: `scanner` (the scan's header, default) or `make-nii` (the
    /// axis-aligned header MATLAB's `make_nii` writes, as QSMART runs N4).
    #[serde(default)]
    pub n4_geometry: N4Geometry,
    pub sections: Vec<MaskSection>,
    /// How `sections` fold into one mask: `or` (union, the default) or `and` (intersection).
    #[serde(default)]
    pub combine: MaskCombine,
    /// Refinements applied to the combined mask, after `combine` — e.g. filling the holes an
    /// intersection leaves inside the brain. Generator ops are rejected here.
    #[serde(default)]
    pub refinements: Vec<MaskOp>,
    /// Prefer a bring-your-own mask from BIDS derivatives when present, falling back to `sections`.
    /// `Some("*")` = first matching derivatives tool alphabetically; `Some("bet")` = that tool only;
    /// `None` = always compute the mask from `sections`.
    #[serde(default)]
    pub custom_mask_tool: Option<String>,
    /// Reconstruct a second time against a mask whose holes are intact, and keep that
    /// reconstruction wherever it is defined. See [`two_pass_sections`](Self::two_pass_sections).
    #[serde(default)]
    pub two_pass: bool,
    /// The reliable-pass mask, when `two_pass` is set. `None` = [`default_two_pass_sections`]
    /// (an Otsu threshold on the phase-quality map, unfilled).
    ///
    /// An ordinary list of mask sections, folded with [`two_pass_combine`](Self::two_pass_combine)
    /// and then the main mask's `refinements`, so anything expressible for one mask is expressible
    /// for this one.
    #[serde(default)]
    pub two_pass_sections: Option<Vec<MaskSection>>,
    /// How the reliable-pass sections fold into one mask. `None` = the main mask's `combine`.
    #[serde(default)]
    pub two_pass_combine: Option<MaskCombine>,
    /// How the second pass is made and merged: `local` (the default: holes only at strong
    /// sources, second pass used within 6-12 mm of them) or `original` (the published algorithm).
    /// qsm-core `pipeline::two_pass` documents both.
    #[serde(default)]
    pub two_pass_mode: TwoPassMode,
}
impl Default for MaskingConfig {
    fn default() -> Self {
        Self {
            inhomogeneity_correction: true,
            inhomogeneity_method: InhomogeneityMethod::default(),
            n4_geometry: N4Geometry::default(),
            sections: default_mask_sections(),
            combine: MaskCombine::default(),
            refinements: vec![],
            custom_mask_tool: None,
            two_pass: false,
            two_pass_sections: None,
            two_pass_combine: None,
            two_pass_mode: TwoPassMode::default(),
        }
    }
}

impl MaskingConfig {
    /// Why a step of the recipe cannot be run, if one cannot ([`MaskOp::validate`]), naming
    /// where the step is.
    pub fn validate(&self) -> Result<(), String> {
        let sections = |name: &str, list: &[MaskSection]| -> Result<(), String> {
            for (i, s) in list.iter().enumerate() {
                s.generator.validate().map_err(|e| format!("masking.{name}[{i}].generator: {e}"))?;
                for (j, op) in s.refinements.iter().enumerate() {
                    op.validate().map_err(|e| format!("masking.{name}[{i}].refinements[{j}]: {e}"))?;
                }
            }
            Ok(())
        };
        sections("sections", &self.sections)?;
        for (j, op) in self.refinements.iter().enumerate() {
            op.validate().map_err(|e| format!("masking.refinements[{j}]: {e}"))?;
        }
        if let Some(two) = &self.two_pass_sections {
            sections("two_pass_sections", two)?;
        }
        Ok(())
    }

    /// The reliable-pass mask sections: the configured ones, or the default recipe.
    pub fn resolved_two_pass_sections(&self) -> Vec<MaskSection> {
        self.two_pass_sections.clone().unwrap_or_else(default_two_pass_sections)
    }

    /// How the reliable-pass sections are combined: [`two_pass_combine`](Self::two_pass_combine),
    /// or the main mask's `combine` when it is unset.
    pub fn resolved_two_pass_combine(&self) -> MaskCombine {
        self.two_pass_combine.unwrap_or(self.combine)
    }
}
