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
    /// `patch` is the sliding-window size `[x, y, z]` in voxels at 1 mm.
    HdBet {
        #[serde(default = "hd_bet_patch")] patch: [usize; 3],
        #[serde(default)] tta: bool,
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
    pub fn hd_bet_default() -> Self { Self::HdBet { patch: hd_bet_patch(), tta: false } }
    /// HD-BET's native (training-size) sliding-window patch `[x, y, z]`.
    pub fn hd_bet_default_patch() -> [usize; 3] { hd_bet_patch() }
    /// Whether this op creates a mask (as opposed to refining one).
    pub fn is_generator(&self) -> bool {
        matches!(self, Self::Threshold { .. } | Self::Bet { .. } | Self::HdBet { .. })
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
            Self::HdBet { patch, tta } => {
                write!(f, "hd-bet:{}x{}x{}", patch[0], patch[1], patch[2])?;
                if *tta { write!(f, ":tta")?; }
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
        "fill-holes" => Ok(MaskOp::FillHoles { max_size: parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1000) }),
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
            for part in parts.iter().skip(1).filter(|p| !p.is_empty()) {
                match *part {
                    "tta" => tta = true,
                    "native" => patch = hd_bet_patch(),
                    "low-memory" => patch = hd_bet_low_memory_patch(),
                    dims => patch = parse_hd_bet_patch(dims)?,
                }
            }
            Ok(MaskOp::HdBet { patch, tta })
        }
        _ => Err(ConfigError::Parse(format!("Unknown mask-op: '{}'", parts[0]))),
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

    #[test]
    fn test_parse_hd_bet() {
        assert_eq!(parse_mask_op("hd-bet").unwrap(), MaskOp::HdBet { patch: [192, 192, 96], tta: false });
        assert_eq!(parse_mask_op("hd-bet:low-memory").unwrap(), MaskOp::HdBet { patch: [128, 128, 64], tta: false });
        assert_eq!(parse_mask_op("hd-bet:160x160x128:tta").unwrap(), MaskOp::HdBet { patch: [160, 160, 128], tta: true });
        assert!(parse_mask_op("hd-bet:100x100x100").is_err(), "not a multiple of 32x32x16");
        assert!(parse_mask_op("hd-bet:big").is_err());
        let op = MaskOp::HdBet { patch: [128, 128, 64], tta: true };
        assert_eq!(format!("{op}"), "hd-bet:128x128x64:tta");
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
        assert_eq!(format!("{sec}"), "magnitude,hd-bet:192x192x96,signal-erode:0.80:5:1:12.0:1000");
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

    #[test]
    fn test_parse_masking_input() {
        assert_eq!(parse_masking_input("phase-quality"), Some(MaskingInput::PhaseQuality));
        assert_eq!(parse_masking_input("magnitude"), Some(MaskingInput::Magnitude));
        assert_eq!(parse_masking_input("magnitude-first"), Some(MaskingInput::MagnitudeFirst));
        assert_eq!(parse_masking_input("magnitude-last"), Some(MaskingInput::MagnitudeLast));
        assert_eq!(parse_masking_input("invalid"), None);
    }
}
