use serde::{Deserialize, Serialize};
use crate::enums::*;
use crate::masking::*;

// ─── Macro for algorithm parameter structs ───
// Generates a serde struct with Default sourced from qsm-core. Each default goes through `Into`, so
// a field can be QSMxT's serde form of a qsm-core type (`te`: `EchoTime` → `EchoTimeChoice`).

macro_rules! param_config {
    ($name:ident from $core:path { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        #[serde(default)]
        pub struct $name { $(pub $field: $ty),* }

        impl Default for $name {
            fn default() -> Self {
                #[allow(unused)]
                let p = <$core>::default();
                Self { $($field: p.$field.into()),* }
            }
        }
    };
}

mod inversion;
mod separation;
mod dependencies;
mod background;
mod masking;
mod field_mapping;
mod outputs;
mod multi_orientation;

pub use inversion::*;
pub use separation::*;
pub use dependencies::*;
pub use background::*;
pub use masking::*;
pub use field_mapping::*;
pub use outputs::*;
pub use multi_orientation::*;

// ─── Top-level config and section structs ───

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PipelineConfig {
    #[serde(default)]
    pub description: String,
    /// The settings-file layout this config was written with ([`crate::migrate`]): an older file
    /// is brought up to date when it is read.
    pub schema_version: crate::migrate::SchemaVersion,
    pub pipeline: PipelineToggles,
    pub field_mapping: FieldMappingConfig,
    pub masking: MaskingConfig,
    pub bg_removal: BgRemovalConfig,
    pub inversion: InversionConfig,
    pub separation: SeparationConfig,
    pub qsm: QsmConfig,
    pub swi: SwiConfig,
    pub smwi: SmwiConfig,
    pub segmentation: SegmentationConfig,
    pub bet: BetConfig,
    pub homogeneity: HomogeneityConfig,
    /// N4 inhomogeneity correction (`masking.inhomogeneity_method = "n4"`).
    #[serde(default)]
    pub n4: N4Config,
    #[serde(default)]
    pub multi_orientation: MultiOrientationConfig,
    /// Rigid co-registration of the echoes before field mapping. Off by default.
    #[serde(default)]
    pub motion_correction: crate::motion::MotionCorrectionConfig,
    /// Rigid registration settings, shared by motion correction and multi-orientation
    /// co-registration (`--registration-*`).
    #[serde(default)]
    pub registration: crate::motion::RegistrationConfig,
    /// Recommendations the user chose not to follow ([`crate::recommendations`]), each for the
    /// value its setting had then: a change to the setting brings the recommendation back. They
    /// change nothing a run computes. A reference-pipeline rule's is dropped when read: there is
    /// nothing to dismiss ([`crate::recommendations::is_reference_rule`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "recommendation_dismissals")]
    pub dismissed_recommendations: Vec<Dismissal>,
    /// The toolbox pipeline the settings were chosen from ([`crate::pipelines`], `--pipeline`):
    /// a label that changes nothing a run computes. While its choices hold, the config is said to
    /// follow that pipeline, with its departures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_pipeline: Option<String>,
    /// The required changes the run made itself ([`PipelineConfig::normalised`]): outputs turned on
    /// because another setting needs them and the configuration left them off, each said in plain
    /// words ("Segmentation turned on: Reference = ventricles needs it ..."). Provenance only: what
    /// the log said, kept in `pipeline_config.toml` and the methods text's Note. It changes nothing
    /// a run computes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_changes: Vec<String>,
}

/// `dismissed_recommendations` as read, less any of a reference-pipeline rule.
fn recommendation_dismissals<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Dismissal>, D::Error> {
    let mut all = Vec::<Dismissal>::deserialize(d)?;
    all.retain(|x| !crate::recommendations::is_reference_rule(&x.rule));
    Ok(all)
}

/// A recommendation dismissed for one value of its setting ([`crate::recommendations::dismiss`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dismissal {
    /// QSM.rs's rule id (`rts.bg-vsharp`).
    pub rule: String,
    /// The setting's value(s) it was dismissed for, as
    /// [`chosen_text`](crate::recommendations::chosen_text) writes them (`pdf`).
    pub chosen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PipelineToggles {
    pub do_qsm: bool,
    pub do_swi: bool,
    /// Publish the combined magnitude (`part-mag_T2starw`): the root sum of squares of every
    /// echo's magnitude, each corrected for inhomogeneity when that is on. On by default. Off, the
    /// run computes it only for a step that reads it (SMWI, SynthSeg, a magnitude-weighted
    /// inversion), and keeps it among that run's intermediates.
    pub do_magnitude: bool,
    /// Susceptibility map-weighted imaging — weights the magnitude by a mask built from χ rather
    /// than from filtered phase. Needs a susceptibility map, so it implies QSM.
    #[serde(default)]
    pub do_smwi: bool,
    /// Whole-brain parcellation of the GRE magnitude via SynthSeg (`_dseg.nii`).
    #[serde(default)]
    pub do_segmentation: bool,
    /// Per-structure statistics over the segmentation, for every quantitative map the run made.
    /// Needs a segmentation, so it implies one; and needs at least one map, so it implies QSM only
    /// when nothing else would produce one.
    #[serde(default)]
    pub do_analysis: bool,
    pub do_t2starmap: bool,
    pub do_r2starmap: bool,
    /// Compute an R2 map (Hz) from a multi-echo spin-echo (MESE) acquisition via EPG.
    pub do_r2map: bool,
    /// Compute an R2' map (Hz) = R2* − R2 (needs GRE magnitude + a MESE acquisition).
    pub do_r2primemap: bool,
    pub do_chi_separation: bool,
    pub export_dicom: bool,
    /// Resample oblique acquisitions onto an axial grid when their obliquity exceeds
    /// `obliquity_threshold`. Off by default: the classical methods take the B0 direction and are
    /// correct on the acquired grid. Methods that only work on axial data resample regardless.
    pub axial_resampling: bool,
    /// Degrees of obliquity above which `axial_resampling` resamples (non-negative).
    pub obliquity_threshold: f64,
    /// Reconstruct inside a box around the mask rather than the whole field of view. Pure speed
    /// on paper, but it moves the FFT's periodic boundary closer to the object, so it is opt-in.
    #[serde(default)]
    pub crop_to_mask: bool,
    /// Margin in millimetres around the mask when `crop_to_mask` is set. Measured on a UK
    /// Biobank acquisition, 16 mm moved chi by 0.8% of dynamic range at the median and 5.1% at
    /// p99; 32 mm and 64 mm were identical to the uncropped reconstruction. 32 mm is the
    /// smallest margin that costs nothing in accuracy.
    /// Grow the grid outward to an FFT-friendly size before the FFT-bound stages.
    ///
    /// Off by default. It discards nothing, but it is not a no-op: the dipole kernel is sampled
    /// at `k = n / (N dx)`, so changing `N` changes where k-space is sampled and the
    /// deconvolution along with it. Measured on a resampled UK Biobank grid, a TKD
    /// reconstruction was unchanged at the median (4e-7 ppm) with a tail reaching 5.7% of
    /// dynamic range at p99; WH-QSM, being iterative, moved 0.06% at the median. Worth having
    /// where an awkward grid makes the FFT expensive, but not worth changing everyone's numbers
    /// for by default.
    #[serde(default)]
    pub fft_padding: bool,
    #[serde(default = "default_crop_margin_mm")]
    pub crop_margin_mm: f64,
    /// Which grid the derivatives are written on when the run was resampled to axial. `Acquired`
    /// (the default) puts them back where the data came from, so transforms the caller already
    /// holds still apply; `Working` leaves them on the resampled grid.
    #[serde(default)]
    pub output_space: OutputSpace,
    /// Reconstruct even when the sidecar's `SliceThickness` says the slices leave a gap (a 2D
    /// multi-slice acquisition thinner than its pitch). Off by default: the FFT kernels assume a
    /// contiguous grid, so a gapped volume gives a wrong susceptibility map.
    #[serde(default)]
    pub ignore_slice_gap: bool,
    /// Taper ratio of the χ-separation toolbox's k-space Tukey window, applied to every echo's
    /// complex image on the acquired grid before any other stage reads it (qsm-core
    /// `utils::tukey_window`; the `tukey_window` step, after phase scaling). 0 (the default) applies none; the toolbox
    /// uses 0.4, and 0 for Philips, which its methods suggest through the pipeline rules when
    /// the manufacturer is known. Needs magnitude.
    #[serde(default)]
    pub tukey_window: f64,
}
impl Default for PipelineToggles {
    fn default() -> Self {
        Self { do_qsm: true, do_swi: false, do_magnitude: true, do_smwi: false,
            do_segmentation: false, do_analysis: false, do_t2starmap: false, do_r2starmap: false, do_r2map: false, do_r2primemap: false, do_chi_separation: false, export_dicom: false, axial_resampling: false, obliquity_threshold: DEFAULT_OBLIQUITY_THRESHOLD,
            crop_to_mask: false, fft_padding: false, crop_margin_mm: default_crop_margin_mm(),
            output_space: OutputSpace::Acquired, ignore_slice_gap: false, tukey_window: 0.0 }
    }
}

fn default_crop_margin_mm() -> f64 { 32.0 }


/// The obliquity (degrees) above which `axial_resampling` resamples, unless set.
pub const DEFAULT_OBLIQUITY_THRESHOLD: f64 = 10.0;

impl PipelineConfig {
    /// The overlap tiling the deep-learning networks run with, `(core, halo)` in voxels, or
    /// `None` for whole-volume: `inversion.tile_size`, with the default halo when none is set.
    pub fn dl_tile(&self) -> Option<(usize, usize)> {
        self.inversion.tile_size.map(|core| (core, self.inversion.tile_halo.unwrap_or(DEFAULT_TILE_HALO)))
    }

    /// Whether a network this run uses takes the tiling: a tileable inversion, or a tileable
    /// background removal (BFRnet, iQFM) in a run that removes the background.
    pub fn runs_a_tileable_network(&self) -> bool {
        self.inversion.algorithm.tileable()
            || (self.bg_removal.algorithm.tileable() && !crate::bridge::skips_bgremove(self))
    }
}

impl PipelineConfig {
    pub fn from_toml(s: &str) -> crate::Result<Self> {
        Self::from_toml_migrating(s).map(|(c, _)| c)
    }

    /// Parse a TOML config, mapping settings older QSMxT versions wrote onto today's, with a
    /// warning for each mapping the caller should show.
    ///
    /// QSMxT up to v9.22.0 had no `axial_resampling`: a negative `obliquity_threshold` meant
    /// "never resample", any other value "resample above it". Such a config (a threshold and no
    /// `axial_resampling`) keeps its meaning. A config that has `axial_resampling` must give a
    /// non-negative threshold.
    pub fn from_toml_migrating(s: &str) -> crate::Result<(Self, Vec<String>)> {
        Self::from_toml_noting(s).map(|(c, m)| (c, m.warnings.into_iter().chain(m.notes).collect()))
    }

    /// [`from_toml_migrating`](Self::from_toml_migrating), with the warnings apart from the notes
    /// that a setting took today's default ([`crate::migrate`]), for a caller that shows them
    /// differently.
    pub fn from_toml_noting(s: &str) -> crate::Result<(Self, crate::migrate::Migration)> {
        let parse_err = |e: toml::de::Error| crate::error::ConfigError::Parse(format!("{}", e));
        let mut value: toml::Table = toml::from_str(s).map_err(parse_err)?;
        let mut notes = crate::migrate::Migration::default();
        // Before anything is added to the file: what an older file holds says which release
        // wrote it.
        crate::migrate::migrate(&mut value, &mut notes)?;
        let warnings = &mut notes.warnings;
        if let Some(toml::Value::Table(p)) = value.get_mut("pipeline") {
            let threshold = p.get("obliquity_threshold").and_then(|v| v.as_float().or(v.as_integer().map(|i| i as f64)));
            match (threshold, p.contains_key("axial_resampling")) {
                (Some(t), false) => {
                    let on = t >= 0.0;
                    p.insert("axial_resampling".into(), toml::Value::Boolean(on));
                    if !on {
                        p.insert("obliquity_threshold".into(), toml::Value::Float(DEFAULT_OBLIQUITY_THRESHOLD));
                    }
                    warnings.push(if on {
                        format!("pipeline.obliquity_threshold = {t} without axial_resampling is deprecated: \
                                 read as axial_resampling = true, obliquity_threshold = {t}")
                    } else {
                        format!("pipeline.obliquity_threshold = {t} is deprecated: a negative threshold \
                                 meant no resampling, read as axial_resampling = false")
                    });
                }
                (Some(t), true) if t < 0.0 => {
                    return Err(crate::error::ConfigError::Parse(format!(
                        "pipeline.obliquity_threshold = {t}: the threshold is in degrees and cannot be negative; \
                         set axial_resampling = false to turn resampling off")));
                }
                _ => {}
            }
        }
        let config: Self = toml::Value::Table(value).try_into().map_err(parse_err)?;
        config.masking.validate().map_err(crate::error::ConfigError::Parse)?;
        Ok((config, notes))
    }

    /// Parse a JSON config (as a front end keeps one, e.g. qsmbly's saved settings), bringing one
    /// without `schema_version` up to date as [`from_toml_noting`](Self::from_toml_noting) does a
    /// settings file. A config with `schema_version` is read as it is.
    pub fn from_json_migrating(s: &str) -> crate::Result<(Self, crate::migrate::Migration)> {
        let parse_err = |e: String| crate::error::ConfigError::Parse(e);
        let json: serde_json::Value = serde_json::from_str(s).map_err(|e| parse_err(e.to_string()))?;
        if json.get("schema_version").is_some() {
            let config = serde_json::from_value(json).map_err(|e| parse_err(e.to_string()))?;
            return Ok((config, Default::default()));
        }
        // TOML has no null: an unset setting is simply absent, which reads the same.
        fn drop_nulls(v: serde_json::Value) -> serde_json::Value {
            match v {
                serde_json::Value::Object(m) => serde_json::Value::Object(
                    m.into_iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k, drop_nulls(v))).collect()),
                serde_json::Value::Array(a) => serde_json::Value::Array(a.into_iter().map(drop_nulls).collect()),
                v => v,
            }
        }
        let table = toml::Table::try_from(drop_nulls(json)).map_err(|e| parse_err(e.to_string()))?;
        let text = toml::to_string(&table).map_err(|e| parse_err(e.to_string()))?;
        Self::from_toml_noting(&text)
    }

    pub fn to_toml(&self) -> crate::Result<String> {
        toml::to_string_pretty(self).map_err(|e| crate::error::ConfigError::Serialize(format!("{}", e)))
    }

    pub fn to_json(&self) -> crate::Result<String> {
        serde_json::to_string(self).map_err(|e| crate::error::ConfigError::Serialize(format!("{}", e)))
    }

    /// Like [`to_toml`](Self::to_toml), but prunes the `inversion` and `bg_removal`
    /// tables down to the selected algorithm only (keeping `algorithm` + its one
    /// sub-table). The result still round-trips through [`from_toml`](Self::from_toml):
    /// the omitted algorithms simply fall back to their `#[serde(default)]` values.
    /// Useful for a compact, human-facing config that drops every unselected
    /// algorithm's parameters.
    pub fn to_toml_selected(&self) -> crate::Result<String> {
        let mut value = toml::Value::try_from(self)
            .map_err(|e| crate::error::ConfigError::Serialize(format!("{}", e)))?;
        if let toml::Value::Table(root) = &mut value {
            prune_to_selected_algorithm(root, "inversion");
            prune_to_selected_algorithm(root, "bg_removal");
        }
        toml::to_string_pretty(&value)
            .map_err(|e| crate::error::ConfigError::Serialize(format!("{}", e)))
    }
}

impl PipelineConfig {
    /// Only the settings that differ from QSMxT's defaults, as a QSMxT configuration file: a short
    /// config that says what was changed. Settings that do not apply to this config are left out
    /// ([`prune_hidden`](crate::schema::prune_hidden)), as the `qsmxt run` command leaves them
    /// out. `qsmxt run --config` fills in the rest with the defaults, so it reads back as this
    /// config, less those settings.
    pub fn to_toml_changed(&self) -> crate::Result<String> {
        let ser = |c: &PipelineConfig| toml::Value::try_from(c)
            .map_err(|e| crate::error::ConfigError::Serialize(format!("{}", e)));
        let pruned = crate::schema::prune_hidden(self, &crate::schema::ViewFacts::default());
        let (toml::Value::Table(mut mine), toml::Value::Table(defaults)) = (ser(&pruned)?, ser(&PipelineConfig::default())?) else {
            return Err(crate::error::ConfigError::Serialize("a config is not a TOML table".into()));
        };
        keep_changed(&mut mine, &defaults);
        // Always said, so the file reads back as today's settings rather than an older version's.
        mine.insert("schema_version".into(), toml::Value::Integer(crate::migrate::CONFIG_SCHEMA_VERSION as i64));
        toml::to_string_pretty(&mine).map_err(|e| crate::error::ConfigError::Serialize(format!("{}", e)))
    }
}

/// Drop from `mine` every value equal to `base`'s, and every table left empty; a value `base` does
/// not have stays. Arrays (a mask recipe's sections) are compared whole.
fn keep_changed(mine: &mut toml::value::Table, base: &toml::value::Table) {
    mine.retain(|key, value| match (value, base.get(key)) {
        (toml::Value::Table(sub), Some(toml::Value::Table(base_sub))) => {
            keep_changed(sub, base_sub);
            !sub.is_empty()
        }
        (value, Some(base_value)) => value != base_value,
        (_, None) => true,
    });
}

/// In `root[section]`, drop every nested algorithm table except the one whose key
/// matches that section's `algorithm` value. Keeps `algorithm` and any non-table keys.
/// No-op if the section or its `algorithm` field is absent.
fn prune_to_selected_algorithm(root: &mut toml::value::Table, section: &str) {
    let Some(toml::Value::Table(sub)) = root.get_mut(section) else { return };
    let Some(selected) = sub.get("algorithm").and_then(|v| v.as_str()).map(str::to_owned) else { return };
    let to_remove: Vec<String> = sub
        .iter()
        .filter(|(k, v)| k.as_str() != "algorithm" && v.is_table() && k.as_str() != selected)
        .map(|(k, _)| k.clone())
        .collect();
    for k in &to_remove {
        sub.remove(k);
    }
}

#[cfg(test)]
mod selected_toml_tests;

#[cfg(test)]
mod r2primenet_config_tests;

#[cfg(test)]
mod r2prime_strategy_tests;

#[cfg(test)]
mod reference_dependency_tests;

#[cfg(test)]
mod analysis_dependency_tests;

#[cfg(test)]
mod smwi_dependency_tests;

#[cfg(test)]
mod option_field_tests;

#[cfg(test)]
mod axial_resampling_tests;

#[cfg(test)]
mod removed_settings_tests;

#[cfg(test)]
mod qsmart_inputs_tests;
