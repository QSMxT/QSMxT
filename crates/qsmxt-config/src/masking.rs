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
        matches!(self.generator, MaskOp::Threshold { .. } | MaskOp::Bet { .. })
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

pub fn parse_mask_op(s: &str) -> crate::Result<MaskOp> {
    let parts: Vec<&str> = s.splitn(3, ':').collect();
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
        _ => Err(ConfigError::Parse(format!("Unknown mask-op: '{}'", parts[0]))),
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
