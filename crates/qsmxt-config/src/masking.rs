use serde::{Deserialize, Serialize};
use std::fmt;
use crate::error::ConfigError;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum MaskingInput {
    MagnitudeFirst, Magnitude, MagnitudeLast, PhaseQuality,
}
impl fmt::Display for MaskingInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            Self::MagnitudeFirst => "magnitude-first", Self::Magnitude => "magnitude",
            Self::MagnitudeLast => "magnitude-last", Self::PhaseQuality => "phase-quality",
        })
    }
}

pub fn parse_masking_input(s: &str) -> Option<MaskingInput> {
    match s.trim() {
        "magnitude" => Some(MaskingInput::Magnitude),
        "magnitude-first" => Some(MaskingInput::MagnitudeFirst),
        "magnitude-last" => Some(MaskingInput::MagnitudeLast),
        "phase-quality" => Some(MaskingInput::PhaseQuality),
        _ => None,
    }
}

/// How multiple mask sections fold into the final mask.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MaskCombine {
    /// Union — a voxel is kept if any section keeps it.
    #[default]
    Or,
    /// Intersection — a voxel is kept only if every section keeps it.
    And,
}

impl MaskCombine {
    /// Fold one section's mask into the accumulator, voxel by voxel.
    pub fn accumulate(&self, acc: &mut [u8], section: &[u8]) {
        match self {
            Self::Or => for (a, &s) in acc.iter_mut().zip(section) { *a |= s; },
            Self::And => for (a, &s) in acc.iter_mut().zip(section) { *a &= s; },
        }
    }
}

impl fmt::Display for MaskCombine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self { Self::Or => "or", Self::And => "and" })
    }
}

pub fn parse_mask_combine(s: &str) -> Option<MaskCombine> {
    match s.trim() {
        "or" => Some(MaskCombine::Or),
        "and" => Some(MaskCombine::And),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum MaskThresholdMethod { Otsu, Fixed, Percentile }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum MaskOp {
    Threshold { method: MaskThresholdMethod, #[serde(default)] value: Option<f64> },
    Bet { fractional_intensity: f64 },
    Erode { iterations: usize },
    Dilate { iterations: usize },
    Close { radius: usize },
    FillHoles { max_size: usize },
    GaussianSmooth { sigma_mm: f64 },
    /// Signal-gated erosion (QSM-CI): peel only low-signal boundary voxels (sinus / skull-base
    /// dropout), after dividing out the receive-coil bias, down to a depth cap. Needs magnitude.
    SignalErode {
        #[serde(default = "se_threshold")] threshold: f64,
        #[serde(default = "se_depth_cap")] depth_cap: usize,
        #[serde(default = "se_global_erosions")] global_erosions: usize,
        #[serde(default = "se_bias_sigma")] bias_sigma: f64,
        #[serde(default = "se_min_component")] min_component: usize,
    },
    /// HD-BET deep-learning brain extraction from the magnitude (needs a `dl` build).
    /// `patch` is the sliding-window size `[x, y, z]` in voxels at 1 mm; `tile_step` is the
    /// window stride as a fraction of the patch, in `(0, 1]`.
    HdBet {
        #[serde(default = "hd_bet_patch")] patch: [usize; 3],
        #[serde(default)] tta: bool,
        #[serde(default = "hd_bet_tile_step")] tile_step: f64,
    },
}

// Defaults come from qsm-core so the two never drift apart.
fn se_default() -> qsm_core::utils::SignalErosionParams { qsm_core::utils::SignalErosionParams::default() }
fn se_threshold() -> f64 { se_default().threshold }
fn se_depth_cap() -> usize { se_default().depth_cap }
fn se_global_erosions() -> usize { se_default().global_erosions }
fn se_bias_sigma() -> f64 { se_default().bias_sigma }
fn se_min_component() -> usize { se_default().min_component }
fn hd_bet_patch() -> [usize; 3] { let p = qsm_core::bet::HdBetParams::default().patch; [p.0, p.1, p.2] }
fn hd_bet_tile_step() -> f64 { hd_bet_default_tile_step() }
/// qsm-core's default HD-BET sliding-window step (nnU-Net's `tile_step_size`).
pub fn hd_bet_default_tile_step() -> f64 { qsm_core::bet::HdBetParams::default().tile_step }
/// `hd-bet:low-memory` patch — qsm-core's `HdBetParams::low_memory()` (~1.9 GB peak vs ~4.5 GB).
pub fn hd_bet_low_memory_patch() -> [usize; 3] {
    let p = qsm_core::bet::HdBetParams::low_memory().patch;
    [p.0, p.1, p.2]
}

impl MaskOp {
    /// Signal-gated erosion with qsm-core's defaults (the QSM-CI harmonization setting).
    pub fn signal_erode_default() -> Self {
        Self::SignalErode {
            threshold: se_threshold(), depth_cap: se_depth_cap(), global_erosions: se_global_erosions(),
            bias_sigma: se_bias_sigma(), min_component: se_min_component(),
        }
    }
    /// HD-BET with the native (training-size) patch and no test-time augmentation.
    pub fn hd_bet_default() -> Self {
        Self::HdBet { patch: hd_bet_patch(), tta: false, tile_step: hd_bet_tile_step() }
    }
    /// HD-BET's native (training-size) sliding-window patch `[x, y, z]`.
    pub fn hd_bet_default_patch() -> [usize; 3] { hd_bet_patch() }
    /// Whether this op creates a mask (as opposed to refining one).
    pub fn is_generator(&self) -> bool {
        matches!(self, Self::Threshold { .. } | Self::Bet { .. } | Self::HdBet { .. })
    }
}

impl MaskOp {
    /// The shortest spec that [`parse_mask_op`] turns back into this op.
    ///
    /// [`Display`](fmt::Display) deliberately states every parameter, because that string is the
    /// mask stage's cache key: a change in a qsm-core default has to invalidate the cache rather
    /// than quietly reuse a mask built with the old value. A command line has the opposite need,
    /// so this drops parameters that are already the default. Only the multi-parameter ops
    /// (`hd-bet`, `signal-erode`) differ from `Display`; the rest are short and explicit already,
    /// and `erode:1` reads better than a bare `erode`.
    pub fn compact_spec(&self) -> String {
        match self {
            Self::HdBet { patch, tta, tile_step } => {
                let mut spec = String::from("hd-bet");
                if *patch == hd_bet_low_memory_patch() {
                    spec += ":low-memory";
                } else if *patch != hd_bet_patch() {
                    spec += &format!(":{}x{}x{}", patch[0], patch[1], patch[2]);
                }
                if *tta { spec += ":tta"; }
                if (*tile_step - hd_bet_tile_step()).abs() > f64::EPSILON {
                    spec += &format!(":step={tile_step}");
                }
                spec
            }
            Self::SignalErode { threshold, depth_cap, global_erosions, bias_sigma, min_component } => {
                let d = se_default();
                // Keep every field up to the last one that differs from the default: parsing
                // fills in the ones left off, and a half-empty `signal-erode:::0` reads as noise.
                let fields = [
                    (format!("{threshold:.2}"), (*threshold - d.threshold).abs() > f64::EPSILON),
                    (format!("{depth_cap}"), *depth_cap != d.depth_cap),
                    (format!("{global_erosions}"), *global_erosions != d.global_erosions),
                    (format!("{bias_sigma:.1}"), (*bias_sigma - d.bias_sigma).abs() > f64::EPSILON),
                    (format!("{min_component}"), *min_component != d.min_component),
                ];
                let keep = fields.iter().rposition(|(_, differs)| *differs).map_or(0, |i| i + 1);
                std::iter::once("signal-erode".to_string())
                    .chain(fields[..keep].iter().map(|(v, _)| v.clone()))
                    .collect::<Vec<_>>()
                    .join(":")
            }
            other => format!("{other}"),
        }
    }
}

impl fmt::Display for MaskOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Threshold { method: MaskThresholdMethod::Otsu, .. } => write!(f, "threshold:otsu"),
            Self::Threshold { method: MaskThresholdMethod::Fixed, value } => write!(f, "threshold:fixed:{:.4}", value.unwrap_or(0.5)),
            Self::Threshold { method: MaskThresholdMethod::Percentile, value } => write!(f, "threshold:percentile:{:.1}", value.unwrap_or(75.0)),
            Self::Bet { fractional_intensity } => write!(f, "bet:{:.2}", fractional_intensity),
            Self::Erode { iterations } => write!(f, "erode:{}", iterations),
            Self::Dilate { iterations } => write!(f, "dilate:{}", iterations),
            Self::Close { radius } => write!(f, "close:{}", radius),
            Self::FillHoles { max_size } => write!(f, "fill-holes:{}", max_size),
            Self::GaussianSmooth { sigma_mm } => write!(f, "gaussian:{:.1}", sigma_mm),
            Self::SignalErode { threshold, depth_cap, global_erosions, bias_sigma, min_component } =>
                write!(f, "signal-erode:{:.2}:{}:{}:{:.1}:{}", threshold, depth_cap, global_erosions, bias_sigma, min_component),
            Self::HdBet { patch, tta, tile_step } => {
                write!(f, "hd-bet:{}x{}x{}", patch[0], patch[1], patch[2])?;
                if *tta { write!(f, ":tta")?; }
                write!(f, ":step={}", tile_step)?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MaskSection {
    pub input: MaskingInput,
    pub generator: MaskOp,
    #[serde(default)]
    pub refinements: Vec<MaskOp>,
}

impl MaskSection {
    pub fn has_generator(&self) -> bool {
        self.generator.is_generator()
    }
    pub fn all_ops(&self) -> Vec<MaskOp> {
        let mut ops = vec![self.generator.clone()];
        ops.extend(self.refinements.iter().cloned());
        ops
    }
}

impl fmt::Display for MaskSection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = std::iter::once(format!("{}", self.input))
            .chain(self.all_ops().iter().map(|op| format!("{}", op)))
            .collect();
        write!(f, "{}", parts.join(","))
    }
}

impl MaskSection {
    /// The section as a `--mask` argument, with default parameters left off.
    /// See [`MaskOp::compact_spec`] for why this is not `Display`.
    pub fn compact_spec(&self) -> String {
        std::iter::once(format!("{}", self.input))
            .chain(self.all_ops().iter().map(|op| op.compact_spec()))
            .collect::<Vec<_>>()
            .join(",")
    }
}

pub fn default_mask_sections() -> Vec<MaskSection> {
    vec![MaskSection {
        input: MaskingInput::PhaseQuality,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        refinements: vec![
            MaskOp::Dilate { iterations: 1 },
            MaskOp::FillHoles { max_size: 0 },
            MaskOp::Erode { iterations: 1 },
        ],
    }]
}

/// The reliable-pass mask for two-pass artefact reduction, when none is configured.
///
/// An Otsu threshold on the phase-quality map and nothing else. The dilate/fill-holes/erode
/// refinements `robust-threshold` applies are deliberately absent: the holes strong susceptibility
/// sources punch in a phase-quality mask are what this pass is built around, and filling them
/// would collapse it onto the main pass. Mirrors qsm-core's `default_reliable_sections`, which
/// `two_pass_default_matches_qsm_core` holds it to.
pub fn default_two_pass_sections() -> Vec<MaskSection> {
    vec![MaskSection {
        input: MaskingInput::PhaseQuality,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        refinements: vec![],
    }]
}

/// QSMART has no internal mask erosion (unlike V-SHARP), so it needs a tight
/// brain mask — a loose threshold mask leaks non-brain phase into the global
/// dipole inversion and produces streaking. Default QSMART to BET-on-magnitude.
pub fn qsmart_default_mask_sections() -> Vec<MaskSection> {
    vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Bet { fractional_intensity: 0.5 },
        refinements: vec![MaskOp::Erode { iterations: 2 }],
    }]
}

/// HD-BET on the magnitude followed by signal-gated erosion — the QSM-CI harmonization masking
/// (the `hd-bet` mask preset).
pub fn hd_bet_mask_sections() -> Vec<MaskSection> {
    vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::hd_bet_default(),
        refinements: vec![MaskOp::signal_erode_default()],
    }]
}

/// A complete masking recipe: the sections, how they fold together, and the refinements that
/// run on the combined mask. Presets that need more than one section return one of these.
#[derive(Debug, Clone, PartialEq)]
pub struct MaskRecipe {
    pub sections: Vec<MaskSection>,
    pub combine: MaskCombine,
    pub refinements: Vec<MaskOp>,
}

impl MaskRecipe {
    /// A recipe that is just sections, OR'd, with nothing after the combine.
    pub fn from_sections(sections: Vec<MaskSection>) -> Self {
        Self { sections, combine: MaskCombine::Or, refinements: vec![] }
    }
}

/// BET on the magnitude intersected with a thresholded phase-quality map, then hole-filled and
/// eroded — the two-mask recipe recommended by the ISMRM electro-magnetic tissue properties study
/// group consensus (Bilgic et al., MRM 2024; doi:10.1002/mrm.30006). BET bounds the head while the
/// phase-quality threshold drops voxels whose phase cannot be unwrapped reliably; the holes that
/// intersection leaves inside the brain are filled afterwards. The phase-quality section carries
/// the same dilate/fill-holes/erode refinements as the `robust-threshold` preset, so each side of
/// the intersection is the mask its own preset would produce.
pub fn bet_and_phase_mask_recipe() -> MaskRecipe {
    MaskRecipe {
        sections: vec![
            MaskSection {
                input: MaskingInput::MagnitudeFirst,
                generator: MaskOp::Bet { fractional_intensity: 0.5 },
                refinements: vec![],
            },
            MaskSection {
                input: MaskingInput::PhaseQuality,
                generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
                // The same refinements the `robust-threshold` preset applies to a
                // thresholded quality map, so the phase side of the intersection is the
                // mask that preset would have produced.
                refinements: vec![
                    MaskOp::Dilate { iterations: 1 },
                    MaskOp::FillHoles { max_size: 0 },
                    MaskOp::Erode { iterations: 1 },
                ],
            },
        ],
        combine: MaskCombine::And,
        refinements: vec![
            MaskOp::FillHoles { max_size: 0 },
            MaskOp::Erode { iterations: 1 },
        ],
    }
}

/// Every `--mask-preset`, in the order the TUI lists them. One list, so the CLI, the TUI and the
/// generated command cannot disagree about what a preset means.
pub fn mask_presets() -> Vec<(&'static str, MaskRecipe)> {
    vec![
        ("robust-threshold", MaskRecipe::from_sections(default_mask_sections())),
        ("bet", MaskRecipe::from_sections(vec![MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Bet { fractional_intensity: 0.5 },
            refinements: vec![MaskOp::Erode { iterations: 2 }],
        }])),
        ("hd-bet", MaskRecipe::from_sections(hd_bet_mask_sections())),
        ("bet-and-phase", bet_and_phase_mask_recipe()),
    ]
}

/// The preset a masking config reproduces, for writing `--mask-preset <name>` instead of spelling
/// every section out. Returns the preset name and, when the only difference is that every section
/// reads a different image, the `--masking-input` that goes with it.
///
/// Exact matches win: `--mask-preset bet` and `--mask-preset bet --masking-input magnitude` mean
/// the same thing, and the shorter one is the one to print.
pub fn masking_preset_command(masking: &crate::config::MaskingConfig) -> Option<(&'static str, Option<MaskingInput>)> {
    let presets = mask_presets();
    let tail_matches = |r: &MaskRecipe| r.combine == masking.combine && r.refinements == masking.refinements;

    if let Some((name, _)) = presets.iter().find(|(_, r)| r.sections == masking.sections && tail_matches(r)) {
        return Some((name, None));
    }

    // Every section reading the same image is what `--masking-input` produces.
    let input = masking.sections.first()?.input;
    if !masking.sections.iter().all(|s| s.input == input) {
        return None;
    }
    presets.iter().find_map(|(name, r)| {
        let overridden: Vec<MaskSection> =
            r.sections.iter().map(|s| MaskSection { input, ..s.clone() }).collect();
        (overridden == masking.sections && tail_matches(r)).then_some((*name, Some(input)))
    })
}

pub fn parse_mask_op(s: &str) -> crate::Result<MaskOp> {
    let parts: Vec<&str> = s.split(':').collect();
    match parts[0] {
        "threshold" => match parts.get(1).copied() {
            Some("otsu") | None => Ok(MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None }),
            Some("fixed") => Ok(MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.5)) }),
            Some("percentile") => Ok(MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value: Some(parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(75.0)) }),
            Some(other) => Err(ConfigError::Parse(format!("Invalid threshold method: '{}'", other))),
        },
        "bet" => Ok(MaskOp::Bet { fractional_intensity: parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.5) }),
        "erode" => Ok(MaskOp::Erode { iterations: parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1) }),
        "dilate" => Ok(MaskOp::Dilate { iterations: parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1) }),
        "close" => Ok(MaskOp::Close { radius: parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1) }),
        // Bare `fill-holes` is the automatic cap (5% of the volume) — what every preset spells as
        // `fill-holes:0` and what the TUI adds — rather than a 1000-voxel cap nothing else used.
        "fill-holes" => Ok(MaskOp::FillHoles { max_size: parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0) }),
        "gaussian" => Ok(MaskOp::GaussianSmooth { sigma_mm: parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(4.0) }),
        "signal-erode" => {
            let num = |i: usize, name: &str| -> crate::Result<Option<f64>> {
                parts.get(i).filter(|s| !s.is_empty()).map(|s| s.parse::<f64>().map_err(|_| {
                    ConfigError::Parse(format!("signal-erode: invalid {name} '{s}'"))
                })).transpose()
            };
            let d = se_default();
            Ok(MaskOp::SignalErode {
                threshold: num(1, "threshold")?.unwrap_or(d.threshold),
                depth_cap: num(2, "depth cap")?.map(|v| v as usize).unwrap_or(d.depth_cap),
                global_erosions: num(3, "global erosions")?.map(|v| v as usize).unwrap_or(d.global_erosions),
                bias_sigma: num(4, "bias sigma")?.unwrap_or(d.bias_sigma),
                min_component: num(5, "min component")?.map(|v| v as usize).unwrap_or(d.min_component),
            })
        }
        "hd-bet" => {
            let mut patch = hd_bet_patch();
            let mut tta = false;
            let mut tile_step = hd_bet_tile_step();
            for part in parts.iter().skip(1).filter(|p| !p.is_empty()) {
                match *part {
                    "tta" => tta = true,
                    "native" => patch = hd_bet_patch(),
                    "low-memory" => patch = hd_bet_low_memory_patch(),
                    s if s.starts_with("step=") => tile_step = parse_hd_bet_step(&s[5..])?,
                    dims => patch = parse_hd_bet_patch(dims)?,
                }
            }
            Ok(MaskOp::HdBet { patch, tta, tile_step })
        }
        _ => Err(ConfigError::Parse(format!("Unknown mask-op: '{}'", parts[0]))),
    }
}

/// `step=<f>` HD-BET sliding-window stride, as a fraction of the patch. qsm-core requires
/// `(0, 1]`: at 1.0 the windows abut, and anything larger would leave gaps in the volume.
fn parse_hd_bet_step(s: &str) -> crate::Result<f64> {
    let v: f64 = s.trim().parse()
        .map_err(|_| ConfigError::Parse(format!("hd-bet: step must be a number, got '{s}'")))?;
    if v > 0.0 && v <= 1.0 {
        Ok(v)
    } else {
        Err(ConfigError::Parse(format!("hd-bet: step {v} must be greater than 0 and at most 1")))
    }
}

/// `XxYxZ` HD-BET patch; each size must be a multiple of the network's 32×32×16 down-sampling.
fn parse_hd_bet_patch(s: &str) -> crate::Result<[usize; 3]> {
    let dims: Vec<usize> = s.split('x').map(|d| d.trim().parse()).collect::<Result<_, _>>()
        .map_err(|_| ConfigError::Parse(format!("hd-bet: expected a patch like 192x192x96, 'low-memory' or 'tta', got '{s}'")))?;
    match dims[..] {
        [x, y, z] if x > 0 && y > 0 && z > 0 && x % 32 == 0 && y % 32 == 0 && z % 16 == 0 => Ok([x, y, z]),
        _ => Err(ConfigError::Parse(format!("hd-bet: patch '{s}' must be three sizes that are multiples of 32x32x16"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_threshold_otsu() {
        let op = parse_mask_op("threshold:otsu").unwrap();
        assert!(matches!(op, MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. }));
    }

    #[test]
    fn test_parse_threshold_fixed() {
        let op = parse_mask_op("threshold:fixed:0.3").unwrap();
        if let MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value } = op {
            assert_eq!(value, Some(0.3));
        } else { panic!("wrong variant"); }
    }

    #[test]
    fn test_parse_threshold_percentile() {
        let op = parse_mask_op("threshold:percentile:80").unwrap();
        if let MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value } = op {
            assert_eq!(value, Some(80.0));
        } else { panic!("wrong variant"); }
    }

    #[test]
    fn test_parse_bet() {
        let op = parse_mask_op("bet:0.35").unwrap();
        if let MaskOp::Bet { fractional_intensity } = op {
            assert!((fractional_intensity - 0.35).abs() < 1e-10);
        } else { panic!("wrong variant"); }
    }

    #[test]
    fn test_parse_erode() {
        let op = parse_mask_op("erode:3").unwrap();
        assert!(matches!(op, MaskOp::Erode { iterations: 3 }));
    }

    #[test]
    fn test_parse_dilate() {
        let op = parse_mask_op("dilate:2").unwrap();
        assert!(matches!(op, MaskOp::Dilate { iterations: 2 }));
    }

    #[test]
    fn test_parse_fill_holes() {
        let op = parse_mask_op("fill-holes:0").unwrap();
        assert!(matches!(op, MaskOp::FillHoles { max_size: 0 }));
    }

    #[test]
    fn test_parse_close() {
        let op = parse_mask_op("close:5").unwrap();
        assert!(matches!(op, MaskOp::Close { radius: 5 }));
    }

    #[test]
    fn test_parse_gaussian() {
        let op = parse_mask_op("gaussian:2.5").unwrap();
        if let MaskOp::GaussianSmooth { sigma_mm } = op {
            assert!((sigma_mm - 2.5).abs() < 1e-10);
        } else { panic!("wrong variant"); }
    }

    #[test]
    fn test_parse_signal_erode() {
        // Defaults match qsm-core's (the QSM-CI harmonization setting).
        assert_eq!(parse_mask_op("signal-erode").unwrap(), MaskOp::signal_erode_default());
        let op = parse_mask_op("signal-erode:0.7:3").unwrap();
        let d = qsm_core::utils::SignalErosionParams::default();
        assert_eq!(op, MaskOp::SignalErode {
            threshold: 0.7, depth_cap: 3, global_erosions: d.global_erosions,
            bias_sigma: d.bias_sigma, min_component: d.min_component,
        });
        assert!(parse_mask_op("signal-erode:abc").is_err());
        // Display round-trips every parameter (it is the mask-stage cache key).
        let full = parse_mask_op("signal-erode:0.75:4:0:10:500").unwrap();
        assert_eq!(format!("{full}"), "signal-erode:0.75:4:0:10.0:500");
        assert_eq!(parse_mask_op(&format!("{full}")).unwrap(), full);
    }

    /// Every spelling of the op survives print → parse unchanged.
    #[test]
    fn hd_bet_specs_round_trip() {
        for spec in ["hd-bet", "hd-bet:low-memory", "hd-bet:128x128x64:step=0.75",
                     "hd-bet:low-memory:step=1:tta", "hd-bet:step=0.625"] {
            let op = parse_mask_op(spec).expect(spec);
            let printed = format!("{op}");
            assert_eq!(parse_mask_op(&printed).expect(&printed), op, "round-trip of {spec}");
        }
        // Printed unconditionally: an op string always states every parameter it ran with.
        assert_eq!(format!("{}", parse_mask_op("hd-bet:low-memory").unwrap()), "hd-bet:128x128x64:step=0.5");
        assert_eq!(format!("{}", parse_mask_op("hd-bet:tta").unwrap()), "hd-bet:192x192x96:tta:step=0.5");
    }

    #[test]
    fn test_parse_hd_bet() {
        let step = hd_bet_tile_step();
        assert_eq!(parse_mask_op("hd-bet").unwrap(), MaskOp::HdBet { patch: [192, 192, 96], tta: false, tile_step: step });
        assert_eq!(parse_mask_op("hd-bet:low-memory").unwrap(), MaskOp::HdBet { patch: [128, 128, 64], tta: false, tile_step: step });
        assert_eq!(parse_mask_op("hd-bet:160x160x128:tta").unwrap(), MaskOp::HdBet { patch: [160, 160, 128], tta: true, tile_step: step });
        assert!(parse_mask_op("hd-bet:100x100x100").is_err(), "not a multiple of 32x32x16");
        assert!(parse_mask_op("hd-bet:big").is_err());

        // The sliding-window stride, which used to be silently pinned to qsm-core's default.
        assert_eq!(parse_mask_op("hd-bet:step=0.75").unwrap(),
                   MaskOp::HdBet { patch: [192, 192, 96], tta: false, tile_step: 0.75 });
        assert_eq!(parse_mask_op("hd-bet:low-memory:step=1:tta").unwrap(),
                   MaskOp::HdBet { patch: [128, 128, 64], tta: true, tile_step: 1.0 });
        assert!(parse_mask_op("hd-bet:step=0").is_err(), "stride must be > 0");
        assert!(parse_mask_op("hd-bet:step=1.5").is_err(), "stride > 1 would leave gaps");
        assert!(parse_mask_op("hd-bet:step=half").is_err());

        // Always printed, and what we print parses back to the same op.
        let stepped = MaskOp::HdBet { patch: [128, 128, 64], tta: false, tile_step: 0.75 };
        assert_eq!(format!("{stepped}"), "hd-bet:128x128x64:step=0.75");
        assert_eq!(parse_mask_op(&format!("{stepped}")).unwrap(), stepped);
        let op = MaskOp::HdBet { patch: [128, 128, 64], tta: true, tile_step: step };
        assert_eq!(format!("{op}"), "hd-bet:128x128x64:tta:step=0.5");
        assert_eq!(parse_mask_op(&format!("{op}")).unwrap(), op);
        assert!(op.is_generator());
        assert!(!MaskOp::signal_erode_default().is_generator());
    }

    #[test]
    fn test_new_ops_serde_defaults() {
        // Config files may omit parameters; they take qsm-core's defaults.
        let sec: MaskSection = toml::from_str(
            "input = \"magnitude\"\ngenerator = { op = \"hd-bet\" }\nrefinements = [{ op = \"signal-erode\" }]\n",
        ).unwrap();
        assert_eq!(sec.generator, MaskOp::hd_bet_default());
        assert_eq!(sec.refinements, vec![MaskOp::signal_erode_default()]);
        assert_eq!(format!("{sec}"), "magnitude,hd-bet:192x192x96:step=0.5,signal-erode:0.80:5:1:12.0:1000");
    }

    #[test]
    fn test_parse_unknown_op() {
        assert!(parse_mask_op("invalid:1").is_err());
    }

    #[test]
    fn test_parse_threshold_defaults() {
        // No value → default
        let op = parse_mask_op("threshold").unwrap();
        assert!(matches!(op, MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. }));
    }

    #[test]
    fn test_mask_op_display() {
        assert_eq!(format!("{}", MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None }), "threshold:otsu");
        assert_eq!(format!("{}", MaskOp::Erode { iterations: 2 }), "erode:2");
        assert_eq!(format!("{}", MaskOp::FillHoles { max_size: 0 }), "fill-holes:0");
    }

    #[test]
    fn test_mask_section_display() {
        let section = MaskSection {
            input: MaskingInput::PhaseQuality,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            refinements: vec![MaskOp::Dilate { iterations: 1 }, MaskOp::Erode { iterations: 1 }],
        };
        assert_eq!(format!("{}", section), "phase-quality,threshold:otsu,dilate:1,erode:1");
    }

    #[test]
    fn test_default_mask_sections() {
        let sections = default_mask_sections();
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].input, MaskingInput::PhaseQuality);
        assert!(sections[0].has_generator());
        assert_eq!(sections[0].refinements.len(), 3);
    }

    /// The whole contract of the compact form: shorter than `Display`, and parses back to the
    /// same op. If those two hold, the command line can use it and the cache key cannot drift.
    #[test]
    fn compact_specs_round_trip_and_are_no_longer() {
        let ops = [
            MaskOp::hd_bet_default(),
            MaskOp::HdBet { patch: hd_bet_low_memory_patch(), tta: false, tile_step: hd_bet_default_tile_step() },
            MaskOp::HdBet { patch: [160, 160, 128], tta: true, tile_step: 0.75 },
            MaskOp::signal_erode_default(),
            parse_mask_op("signal-erode:0.70").unwrap(),
            parse_mask_op("signal-erode:0.80:5:1:12:500").unwrap(),
            MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
            MaskOp::Erode { iterations: 2 },
            MaskOp::FillHoles { max_size: 0 },
            MaskOp::Bet { fractional_intensity: 0.35 },
        ];
        for op in ops {
            let compact = op.compact_spec();
            assert_eq!(parse_mask_op(&compact).unwrap_or_else(|e| panic!("{compact}: {e}")), op,
                       "compact form of {op} does not parse back");
            assert!(compact.len() <= format!("{op}").len(), "{compact} is longer than {op}");
        }

        // The two that motivated it: all-default parameters collapse to the bare op name.
        assert_eq!(MaskOp::hd_bet_default().compact_spec(), "hd-bet");
        assert_eq!(MaskOp::signal_erode_default().compact_spec(), "signal-erode");
        assert_eq!(MaskOp::HdBet { patch: hd_bet_low_memory_patch(), tta: false,
                                   tile_step: hd_bet_default_tile_step() }.compact_spec(), "hd-bet:low-memory");
        // Only trailing defaults drop: a changed last field keeps the ones before it.
        assert_eq!(parse_mask_op("signal-erode:0.80:5:1:12:500").unwrap().compact_spec(),
                   "signal-erode:0.80:5:1:12.0:500");
        // Display stays fully explicit — it is the cache key.
        assert_eq!(format!("{}", MaskOp::hd_bet_default()), "hd-bet:192x192x96:step=0.5");
    }

    /// The preset list is what `--mask-preset` and the TUI both build from, and the command
    /// generator recognises every entry in it.
    #[test]
    fn every_preset_is_recognised_from_its_config() {
        for (name, recipe) in mask_presets() {
            let masking = crate::config::MaskingConfig {
                sections: recipe.sections.clone(), combine: recipe.combine,
                refinements: recipe.refinements.clone(), ..Default::default()
            };
            assert_eq!(masking_preset_command(&masking), Some((name, None)), "{name}");

            // ...and with every section's input swapped, as `--masking-input` does.
            for input in [MaskingInput::Magnitude, MaskingInput::MagnitudeFirst,
                          MaskingInput::MagnitudeLast, MaskingInput::PhaseQuality] {
                let swapped = crate::config::MaskingConfig {
                    sections: recipe.sections.iter().map(|s| MaskSection { input, ..s.clone() }).collect(),
                    ..masking.clone()
                };
                let (got, got_input) = masking_preset_command(&swapped)
                    .unwrap_or_else(|| panic!("{name} + {input} not recognised"));
                assert_eq!(got, name, "{name} + {input}");
                // An exact match reports no override, whatever the input happens to be.
                assert!(got_input.is_none() || got_input == Some(input), "{name} + {input}");
            }
        }

        // A hand-edited recipe is not a preset.
        let mut masking = crate::config::MaskingConfig::default();
        masking.sections[0].refinements.push(MaskOp::Erode { iterations: 7 });
        assert_eq!(masking_preset_command(&masking), None);
    }

    #[test]
    fn mask_combine_parses_and_round_trips() {
        assert_eq!(parse_mask_combine("or"), Some(MaskCombine::Or));
        assert_eq!(parse_mask_combine("and"), Some(MaskCombine::And));
        assert_eq!(parse_mask_combine(" and "), Some(MaskCombine::And));
        assert_eq!(parse_mask_combine("xor"), None);
        assert_eq!(MaskCombine::default(), MaskCombine::Or, "OR stays the historical default");
        for c in [MaskCombine::Or, MaskCombine::And] {
            assert_eq!(parse_mask_combine(&format!("{c}")), Some(c));
        }
    }

    #[test]
    fn mask_combine_accumulates_union_and_intersection() {
        let a = [1u8, 1, 0, 0];
        let b = [1u8, 0, 1, 0];

        let mut acc = a;
        MaskCombine::Or.accumulate(&mut acc, &b);
        assert_eq!(acc, [1, 1, 1, 0]);

        let mut acc = a;
        MaskCombine::And.accumulate(&mut acc, &b);
        assert_eq!(acc, [1, 0, 0, 0]);
    }

    /// The consensus recipe: BET on the magnitude intersected with thresholded phase quality,
    /// with the intersection's holes filled afterwards.
    #[test]
    fn bet_and_phase_recipe_is_an_intersection_with_post_combine_steps() {
        let r = bet_and_phase_mask_recipe();
        assert_eq!(r.combine, MaskCombine::And);
        assert_eq!(r.sections.len(), 2);
        assert_eq!(r.sections[0].input, MaskingInput::MagnitudeFirst);
        assert!(matches!(r.sections[0].generator, MaskOp::Bet { .. }));
        assert_eq!(r.sections[1].input, MaskingInput::PhaseQuality);
        assert!(matches!(r.sections[1].generator, MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. }));
        // Hole-filling has to happen after the intersection — that is the whole point of the
        // post-combine list, since fill-holes does not commute with an intersection.
        assert_eq!(r.refinements, vec![MaskOp::FillHoles { max_size: 0 }, MaskOp::Erode { iterations: 1 }]);

        assert_eq!(MaskRecipe::from_sections(default_mask_sections()).combine, MaskCombine::Or);
        assert!(MaskRecipe::from_sections(default_mask_sections()).refinements.is_empty());
    }

    /// The reliable-pass default has to be the same recipe on both sides of the bridge — qsm-core
    /// owns it for qsmbly, this crate owns it for the CLI and TUI, and a drift between them would
    /// mean `--two-pass` reconstructed different regions in the browser and on the command line.
    #[test]
    fn two_pass_default_matches_qsm_core() {
        assert_eq!(
            crate::to_mask_sections(&default_two_pass_sections()),
            qsm_core::pipeline::default_reliable_sections(),
        );
    }

    /// Filling or closing here would make the reliable mask a copy of the main one, and the second
    /// reconstruction pure wasted time.
    #[test]
    fn two_pass_default_keeps_its_holes() {
        let sections = default_two_pass_sections();
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].input, MaskingInput::PhaseQuality);
        assert!(sections[0].refinements.is_empty());
    }

    #[test]
    fn test_parse_masking_input() {
        assert_eq!(parse_masking_input("phase-quality"), Some(MaskingInput::PhaseQuality));
        assert_eq!(parse_masking_input("magnitude"), Some(MaskingInput::Magnitude));
        assert_eq!(parse_masking_input("magnitude-first"), Some(MaskingInput::MagnitudeFirst));
        assert_eq!(parse_masking_input("magnitude-last"), Some(MaskingInput::MagnitudeLast));
        assert_eq!(parse_masking_input("invalid"), None);
    }
}
