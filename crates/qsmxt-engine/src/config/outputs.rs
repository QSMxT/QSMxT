use super::*;

/// The patch of a SynthSeg sliding window, or `None` for the whole volume.
pub(crate) fn synthseg_patch(inference: qsm_core::segment::SynthSegInference) -> Option<usize> {
    match inference {
        qsm_core::segment::SynthSegInference::FullVolume => None,
        qsm_core::segment::SynthSegInference::SlidingWindow { patch, .. } => Some(patch),
    }
}

/// SynthSeg's low-memory patch — qsm-core's `SynthSegInference::LOW_MEMORY`.
pub fn synthseg_low_memory_patch() -> usize {
    synthseg_patch(qsm_core::segment::SynthSegInference::LOW_MEMORY).expect("a sliding window")
}

/// The sliding window `patch` asks for, its neighbours overlapping by at least half a patch (as
/// qsm-core's low-memory window does).
pub fn synthseg_inference(patch: Option<usize>) -> qsm_core::segment::SynthSegInference {
    use qsm_core::segment::SynthSegInference as I;
    patch.map_or(I::FullVolume, |patch| I::SlidingWindow { patch, overlap: patch / 2 })
}

/// Whole-brain segmentation via SynthSeg, and the per-structure statistics drawn from it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SegmentationConfig {
    /// Weights generation: `v1` (32 labels, ships with SynthSeg) or `v2` (33 labels, adds CSF).
    /// Selects the label table, so it must match the weights being run.
    pub version: SynthSegVersion,
    /// Cubic centre-crop in voxels at 1 mm before inference, rounded up to a multiple of 32.
    /// `None` processes the whole volume, as the SynthSeg CLI does.
    pub crop: Option<usize>,
    /// Average the posteriors with a left-right flipped second pass. SynthSeg's default; doubles
    /// inference time.
    pub flip_averaging: bool,
    /// Reset each topological class to its largest connected component.
    pub topology_cleanup: bool,
    /// Gaussian blur on the posteriors, in voxels. Zero disables it.
    pub sigma_smoothing: f64,
    /// Run the network as a sliding window of cubes this many voxels on a side at 1 mm (at least
    /// 64, rounded up to a multiple of 16), overlapping by at least half a patch and
    /// Gaussian-blended, so its memory
    /// follows the patch instead of the volume. `None` runs the whole volume at once, as the
    /// SynthSeg CLI does.
    #[serde(default)]
    pub patch: Option<usize>,
    /// Prefer a bring-your-own segmentation from `<bids>/derivatives/<tool>/…` over running
    /// SynthSeg, mirroring `masking.custom_mask_tool`. Always falls back to computing one.
    #[serde(default)]
    pub custom_dseg_tool: Option<String>,
}
impl Default for SegmentationConfig {
    fn default() -> Self {
        let p = qsm_core::segment::SynthSegParams::default();
        Self {
            version: match p.version {
                qsm_core::segment::SynthSegVersion::V1 => SynthSegVersion::V1,
                qsm_core::segment::SynthSegVersion::V2 => SynthSegVersion::V2,
            },
            crop: p.crop,
            flip_averaging: p.flip_averaging,
            topology_cleanup: p.topology_cleanup,
            sigma_smoothing: p.sigma_smoothing,
            patch: synthseg_patch(p.inference),
            custom_dseg_tool: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SmwiConfig {
    /// |χ| (ppm) at which the weighting mask reaches zero.
    pub threshold_ppm: f64,
    /// Power the mask is raised to — contrast strength.
    pub power: f64,
    pub mip_window: usize,
}
impl Default for SmwiConfig {
    fn default() -> Self {
        let p = qsm_core::swi::SmwiParams::default();
        Self { threshold_ppm: p.threshold_ppm, power: p.power, mip_window: p.mip_window }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SwiConfig {
    /// Phase unwrapping before the echoes are combined.
    pub unwrap: SwiUnwrap,
    pub hp_sigma: [f64; 3],
    pub scaling: SwiScaling,
    pub strength: f64,
    pub mip_window: usize,
    /// Also publish CLEAR-SWI's high-pass filtered phase (`desc-swi_phase`): the phase the SWI is
    /// weighted by, in Hz for several echoes and radians for one. Comes from the same CLEAR-SWI
    /// computation as the SWI, so it needs SWI on.
    pub filtered_phase: bool,
}
impl SwiConfig {
    /// The CLEAR-SWI parameters this config runs with: qsm-core's defaults for what the config
    /// does not expose.
    pub fn to_params(&self) -> qsm_core::swi::SwiParams {
        qsm_core::swi::SwiParams {
            unwrap: self.unwrap.into(),
            hp_sigma: self.hp_sigma,
            scaling: self.scaling.into(),
            strength: self.strength,
            mip_window: self.mip_window,
            ..Default::default()
        }
    }
}
impl Default for SwiConfig {
    fn default() -> Self {
        let p = qsm_core::swi::SwiParams::default();
        Self {
            unwrap: p.unwrap.into(),
            hp_sigma: p.hp_sigma,
            scaling: p.scaling.into(),
            strength: p.strength,
            mip_window: p.mip_window,
            filtered_phase: false,
        }
    }
}
