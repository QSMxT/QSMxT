//! Retrospective motion correction (qsm-core `pipeline::run_motion_correction`): rigid
//! co-registration of every echo onto a reference echo before field mapping.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Which echo the others are aligned onto. Written `"first"`, `"last"` or a 1-based echo number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MotionReference {
    #[default]
    First,
    Last,
    /// 1-based echo number.
    Echo(usize),
}

impl fmt::Display for MotionReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::First => write!(f, "first"),
            Self::Last => write!(f, "last"),
            Self::Echo(n) => write!(f, "{n}"),
        }
    }
}

impl std::str::FromStr for MotionReference {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "first" => Ok(Self::First),
            "last" => Ok(Self::Last),
            n => match n.parse::<usize>() {
                Ok(n) if n >= 1 => Ok(Self::Echo(n)),
                _ => Err(format!("motion reference must be first, last or an echo number from 1, got {s:?}")),
            },
        }
    }
}

impl Serialize for MotionReference {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for MotionReference {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw { S(String), N(usize) }
        match Raw::deserialize(d)? {
            Raw::S(s) => s.parse().map_err(serde::de::Error::custom),
            Raw::N(n) => format!("{n}").parse().map_err(serde::de::Error::custom),
        }
    }
}

impl MotionReference {
    pub fn to_core(self) -> qsm_core::motion::MotionReference {
        match self {
            Self::First => qsm_core::motion::MotionReference::First,
            Self::Last => qsm_core::motion::MotionReference::Last,
            Self::Echo(n) => qsm_core::motion::MotionReference::Index(n.saturating_sub(1)),
        }
    }
}

/// Rigid registration (`qsm_core::registration::RigidParams`), top-level in the config because
/// it is shared: motion correction uses it, and so does co-registering multi-orientation groups. `initial` is not exposed: chaining overwrites it, and it
/// is per volume.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RegistrationConfig {
    pub levels: usize,
    pub max_samples: usize,
    pub max_iterations: usize,
    pub max_evaluations: usize,
    pub tolerance: f64,
    pub rotation_step_deg: f64,
    pub translation_step_mm: f64,
    pub min_overlap: f64,
}

impl Default for RegistrationConfig {
    fn default() -> Self {
        let p = qsm_core::registration::RigidParams::default();
        Self {
            levels: p.levels, max_samples: p.max_samples, max_iterations: p.max_iterations,
            max_evaluations: p.max_evaluations, tolerance: p.tolerance,
            rotation_step_deg: p.rotation_step_deg, translation_step_mm: p.translation_step_mm,
            min_overlap: p.min_overlap,
        }
    }
}

impl RegistrationConfig {
    pub fn to_core(&self) -> qsm_core::registration::RigidParams {
        qsm_core::registration::RigidParams {
            levels: self.levels, max_samples: self.max_samples, max_iterations: self.max_iterations,
            max_evaluations: self.max_evaluations, tolerance: self.tolerance,
            rotation_step_deg: self.rotation_step_deg, translation_step_mm: self.translation_step_mm,
            min_overlap: self.min_overlap, initial: None,
        }
    }
}

/// Motion correction. Off by default. `declared_b0` is not exposed: the stage takes the run's
/// B0 direction from its scan metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MotionCorrectionConfig {
    pub enabled: bool,
    pub reference: MotionReference,
    /// Seed each echo's registration from its neighbour's solution.
    pub chain: bool,
    /// Pyramid levels for a chained (seeded) registration.
    pub chain_levels: usize,
    /// NCC below which a registration is reported as suspect.
    pub min_ncc: f64,
}

impl Default for MotionCorrectionConfig {
    fn default() -> Self {
        let p = qsm_core::motion::MotionParams::default();
        Self {
            enabled: false,
            reference: MotionReference::First,
            chain: p.chain,
            chain_levels: p.chain_levels,
            min_ncc: p.min_ncc,
        }
    }
}

impl MotionCorrectionConfig {
    /// qsm-core's stage config, with the shared rigid-registration settings.
    pub fn to_core(&self, registration: &RegistrationConfig) -> qsm_core::pipeline::MotionCorrectionConfig {
        qsm_core::pipeline::MotionCorrectionConfig {
            enabled: self.enabled,
            motion: qsm_core::motion::MotionParams {
                reference: self.reference.to_core(),
                rigid: registration.to_core(),
                chain: self.chain,
                chain_levels: self.chain_levels,
                declared_b0: None,
                min_ncc: self.min_ncc,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_round_trips_and_is_one_based() {
        for (s, r) in [("first", MotionReference::First), ("last", MotionReference::Last), ("3", MotionReference::Echo(3))] {
            assert_eq!(s.parse::<MotionReference>().unwrap(), r);
            assert_eq!(r.to_string(), s);
        }
        assert!("0".parse::<MotionReference>().is_err());
        assert_eq!(MotionReference::Echo(3).to_core(), qsm_core::motion::MotionReference::Index(2));
        let c: MotionCorrectionConfig = toml::from_str("reference = 2").unwrap();
        assert_eq!(c.reference, MotionReference::Echo(2));
        let c: MotionCorrectionConfig = toml::from_str("reference = \"last\"").unwrap();
        assert_eq!(c.reference, MotionReference::Last);
    }

    #[test]
    fn defaults_are_off_and_otherwise_qsm_cores() {
        let c = MotionCorrectionConfig::default();
        assert!(!c.enabled);
        let core = c.to_core(&RegistrationConfig::default());
        let d = qsm_core::motion::MotionParams::default();
        assert_eq!((core.motion.chain, core.motion.chain_levels, core.motion.min_ncc), (d.chain, d.chain_levels, d.min_ncc));
        assert_eq!(core.motion.rigid.levels, d.rigid.levels);
        assert_eq!(core.motion.reference, d.reference);
    }
}
