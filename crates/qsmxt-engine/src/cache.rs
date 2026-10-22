//! Step cache keys: what decides whether a step's earlier output can be reused.
//!
//! A front end keeps outputs between runs however it likes (`qsmxt` in its step directories and
//! state file); whether one is still valid is decided here, from the configuration, so every front
//! end reuses exactly the same outputs.

use crate::config::PipelineConfig;

/// Suffix on the reliable pass's step names: two-pass runs masking, background removal and
/// inversion a second time under these names.
pub const RELIABLE_SUFFIX: &str = "-reliable";

/// Compute a hash of the pipeline config for change detection.
pub fn config_hash(config: &PipelineConfig) -> String {
    let toml = config.to_toml().unwrap_or_default();
    format!("{:x}", md5_simple(&toml))
}

/// Compute a hash of step parameters (algorithm + params JSON) for per-step cache invalidation.
pub fn step_params_hash(algorithm: Option<&str>, params: &serde_json::Value) -> String {
    let s = format!("{}:{}", algorithm.unwrap_or(""), params);
    format!("{:x}", md5_simple(&s))
}

/// Simple hash (not cryptographic, just for change detection).
fn md5_simple(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// The configuration a step reads, serialised whole, so that every field of it is part of the step's
/// cache key — a new config field is keyed without anyone remembering to (#272). The step's own
/// `parameters` (what provenance records) stay as they are; this is added beside them.
///
/// Sections are as narrow as the step actually reads: an upstream step keyed on a section it does
/// not read would be recomputed, and invalidate everything after it, whenever that section
/// changed. Two-pass steps read what their main-pass twin reads.
pub fn consumed_config(step: &str, config: &PipelineConfig) -> serde_json::Value {
    use serde_json::json;
    let base = step.strip_suffix(RELIABLE_SUFFIX).unwrap_or(step);
    let crop = json!({
        "crop_to_mask": config.pipeline.crop_to_mask,
        "fft_padding": config.pipeline.fft_padding,
        "crop_margin_mm": config.pipeline.crop_margin_mm,
    });
    // Whether the user chose the recipe only decides which recipe it is (normalising), and the
    // recipe itself is keyed.
    let masking = || {
        let mut m = serde_json::to_value(&config.masking).unwrap_or_default();
        if let Some(o) = m.as_object_mut() {
            o.remove("recipe_chosen");
        }
        m
    };
    match base {
        "scale_phase" => json!({ "coil_combination_sigma": config.field_mapping.coil_combination_sigma }),
        "magnitude" => json!({
            "inhomogeneity_correction": config.masking.inhomogeneity_correction,
            "homogeneity": config.homogeneity,
        }),
        // The main mask is not the two-pass one, so its settings stay out of the main mask's key.
        "mask" if step == base => {
            let mut m = masking();
            if let Some(o) = m.as_object_mut() {
                o.remove("two_pass");
                o.remove("two_pass_sections");
            }
            json!({ "masking": m, "bet": config.bet })
        }
        "mask" => json!({ "masking": masking(), "bet": config.bet, "homogeneity": config.homogeneity }),
        "motion" => json!({ "motion_correction": config.motion_correction, "registration": config.registration }),
        "swi" => json!({ "swi": config.swi }),
        // Whether the user chose a setting only decides which it is (normalising), and the setting
        // itself is keyed.
        "unwrap" => {
            let mut fm = serde_json::to_value(&config.field_mapping).unwrap_or_default();
            if let Some(o) = fm.as_object_mut() {
                for mark in ["unwrapping_chosen", "laplacian_solver_chosen", "b0_estimation_chosen"] {
                    o.remove(mark);
                }
            }
            json!({ "field_mapping": fm })
        }
        "bgremove" => json!({ "bg_removal": config.bg_removal, "crop": crop }),
        "invert" | "tgv" | "qsmart" => json!({ "inversion": config.inversion, "crop": crop }),
        "twopass" => json!({ "masking": masking() }),
        "reference" | "reference-singlepass" => json!({ "qsm": config.qsm }),
        "segmentation" => json!({ "segmentation": config.segmentation }),
        "smwi" => json!({ "smwi": config.smwi }),
        "chi_separation" => json!({ "separation": config.separation }),
        "output_space" => json!({ "output_space": config.pipeline.output_space }),
        // Keyed by their own parameters alone: R2*/T2* (ARLO has no settings), R2/R2′ (its
        // separation settings are in its parameters; keying the whole section would recompute the
        // MESE fit whenever the χ-separation method changed) and the statistics.
        _ => serde_json::Value::Null,
    }
}

/// A step's cache key: its algorithm and recorded parameters, plus the configuration it reads.
pub fn step_cache_key(
    step: &str, algorithm: Option<&str>, params: &serde_json::Value, config: &PipelineConfig,
) -> String {
    let consumed = consumed_config(step, config);
    if consumed.is_null() {
        return step_params_hash(algorithm, params);
    }
    step_params_hash(
        algorithm, &serde_json::json!({ "params": params, "config": consumed }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every leaf of `value`, as a JSON pointer.
    fn leaves(value: &serde_json::Value, path: String, out: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(m) => for (k, v) in m { leaves(v, format!("{path}/{k}"), out) },
            serde_json::Value::Array(a) if !a.is_empty() && !a[0].is_object() => out.push(format!("{path}/0")),
            serde_json::Value::Array(a) => for (i, v) in a.iter().enumerate() { leaves(v, format!("{path}/{i}"), out) },
            _ => out.push(path),
        }
    }

    /// A different value of the same type, or `None` where there is no generic one (strings —
    /// enum names — and absent options).
    fn changed(v: &serde_json::Value) -> Option<serde_json::Value> {
        use serde_json::Value as V;
        Some(match v {
            V::Bool(b) => V::Bool(!b),
            V::Number(n) if n.is_u64() => serde_json::json!(n.as_u64().unwrap() + 1),
            V::Number(n) if n.is_i64() => serde_json::json!(n.as_i64().unwrap() + 1),
            V::Number(n) => serde_json::json!(n.as_f64().unwrap() * 1.5 + 0.125),
            _ => return None,
        })
    }

    /// Change each leaf of a config section, one at a time, and the step that reads the section
    /// must no longer be cached. Generated from the serialised config like `param_coverage`, so a
    /// field added later is covered without anyone listing it.
    #[test]
    fn every_leaf_of_a_consumed_section_is_in_the_cache_key() {
        let base = PipelineConfig::default();
        let base_json = serde_json::to_value(&base).unwrap();
        let params = serde_json::json!({});
        for (section, step) in [("inversion", "invert"), ("bg_removal", "bgremove"),
                                ("field_mapping", "unwrap"), ("separation", "chi_separation"),
                                ("masking", "mask-reliable"), ("swi", "swi"), ("smwi", "smwi")] {
            let mut paths = Vec::new();
            leaves(&base_json[section], format!("/{section}"), &mut paths);
            let mut tested = 0;
            for p in &paths {
                let Some(new) = changed(base_json.pointer(p).unwrap()) else { continue };
                let mut j = base_json.clone();
                *j.pointer_mut(p).unwrap() = new;
                let Ok(cfg) = serde_json::from_value::<PipelineConfig>(j) else { continue };
                tested += 1;
                assert_ne!(step_cache_key(step, None, &params, &cfg), step_cache_key(step, None, &params, &base),
                           "changing {p} leaves {step} cached");
            }
            assert!(tested * 2 >= paths.len().min(4), "{section}: only {tested} of {} leaves exercised", paths.len());
        }
    }

    /// Whether the user chose the mask decides which recipe it is, when normalising; the steps
    /// are keyed by the recipe, so the mark itself keys nothing.
    #[test]
    fn the_chosen_mark_is_not_in_the_mask_keys() {
        let mut base = PipelineConfig::default();
        base.inversion.algorithm = crate::enums::QsmAlgorithm::Qsmart;
        let mut chosen = base.clone();
        chosen.masking.recipe_chosen = true;
        let p = serde_json::json!({});
        for step in ["mask", "mask-reliable", "twopass"] {
            assert_eq!(step_cache_key(step, None, &p, &chosen), step_cache_key(step, None, &p, &base), "{step}");
        }
    }

    /// Likewise the field-mapping marks: they decide which unwrapping and echo combination it is,
    /// and those are keyed. QSMART's vessel magnitude and units, and the echo fit's threshold, are
    /// keyed where they are read.
    #[test]
    fn qsmarts_input_choices_are_keyed_and_the_marks_are_not() {
        let p = serde_json::json!({});
        let base = PipelineConfig::default();
        let mut marked = base.clone();
        marked.field_mapping.unwrapping_chosen = true;
        marked.field_mapping.laplacian_solver_chosen = true;
        marked.field_mapping.b0_estimation_chosen = true;
        assert_eq!(step_cache_key("unwrap", None, &p, &marked), step_cache_key("unwrap", None, &p, &base));
        let changed = |edit: fn(&mut PipelineConfig), step: &str| {
            let mut c = base.clone();
            edit(&mut c);
            step_cache_key(step, None, &p, &c) != step_cache_key(step, None, &p, &base)
        };
        assert!(changed(|c| c.field_mapping.qsmart_fit_threshold = 30.0, "unwrap"));
        assert!(changed(|c| c.field_mapping.b0_estimation = crate::enums::B0Estimation::QsmartEchofit, "unwrap"));
        assert!(changed(|c| c.field_mapping.laplacian_solver = crate::enums::LaplacianSolver::Mirror, "unwrap"));
        assert!(changed(|c| c.inversion.qsmart.vessel_magnitude = crate::enums::QsmartVesselMagnitude::Combined, "qsmart"));
        assert!(changed(|c| c.inversion.qsmart.vessel_size_units = crate::enums::VesselSizeUnits::Mm, "qsmart"));
        // A new field map invalidates QSMART, which reads it and its reliability map.
        assert!(crate::graph::downstream_of("unwrap").iter().any(|s| s == "qsmart"));
    }

    /// Cropping changes what background removal and the inversion compute, so it is in their keys
    /// — and not in the field map's, which it does not touch.
    #[test]
    fn cropping_is_in_the_reconstruction_keys() {
        let base = PipelineConfig::default();
        let mut c = base.clone();
        c.pipeline.crop_to_mask = !c.pipeline.crop_to_mask;
        let p = serde_json::json!({});
        for step in ["bgremove", "invert", "tgv", "qsmart", "invert-reliable"] {
            assert_ne!(step_cache_key(step, None, &p, &c), step_cache_key(step, None, &p, &base), "{step}");
        }
        assert_eq!(step_cache_key("unwrap", None, &p, &c), step_cache_key("unwrap", None, &p, &base));
        // The two-pass settings are the reliable pass's business, not the main mask's.
        let mut t = base.clone();
        t.masking.two_pass = true;
        assert_eq!(step_cache_key("mask", None, &p, &t), step_cache_key("mask", None, &p, &base));
        assert_ne!(step_cache_key("mask-reliable", None, &p, &t), step_cache_key("mask-reliable", None, &p, &base));
    }
}
