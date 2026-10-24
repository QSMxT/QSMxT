//! Reading settings files written by earlier QSMxT versions.
//!
//! A settings file QSMxT writes holds every setting, so a file written before a default changed
//! keeps the old default as though it had been chosen. Each file therefore says which layout it
//! was written with ([`CONFIG_SCHEMA_VERSION`], `schema_version`). A file without one, or with an
//! older one, is read as follows:
//!
//! - A value equal to the default it had when the file was written ([`V1_DEFAULTS`]) counts as
//!   unset, so it takes today's default, with one log line per setting.
//! - A setting whose unit changed (TGV's α, MEDI's and TFI's λ, the phase-offset smoothing) is
//!   refused when it holds anything but that old default, with a message saying what changed:
//!   converting it would need each scan's echo times, field strength or voxel size.
//! - Settings that no longer exist are dropped, with a warning where dropping one changes what a
//!   run does.

use crate::error::ConfigError;

/// The layout of the settings files this QSMxT writes. Files without `schema_version` are version
/// 1: every QSMxT before it.
pub const CONFIG_SCHEMA_VERSION: u32 = 2;

/// The settings version of a [`crate::PipelineConfig`]: [`CONFIG_SCHEMA_VERSION`] unless a file
/// says otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SchemaVersion(pub u32);

impl Default for SchemaVersion {
    fn default() -> Self {
        SchemaVersion(CONFIG_SCHEMA_VERSION)
    }
}

/// The defaults of version-1 settings files that have since changed, by path, as those files
/// wrote them. A version-1 file holding one of these values gets today's default instead.
const V1_DEFAULTS: &str = r#"
[bg_removal.vsharp]
min_radius = 1.0
[bg_removal.pdf]
tol = 0.00001
[bg_removal.resharp]
radius = 6.0
max_iter = 30
[bg_removal.harperella]
tol = 0.000001
[bg_removal.iharperella]
tol = 0.000001
[inversion.tv]
lambda = 0.0002
rho = 0.02
[inversion.medi]
lambda = 0.001
merit = false
percentage = 0.3
cg_max_iter = 10
max_iter = 30
[inversion.tfi]
lambda = 0.000075
[inversion.ilsqr]
max_iter = 100
pad_mm = 16.0
[inversion.tgv]
iterations = 1000
alpha0 = 0.0002002475
alpha1 = 0.00006674916
[inversion.fansi]
mu1 = 0.02
alpha0 = 0.0004
mu0 = 0.04
[inversion.l1qsm]
alpha1 = 0.0002
mu1 = 0.02
lambda = 1.0
[inversion.whqsm]
muh = 3.0
max_iter = 300
[inversion.hdqsm]
alpha_l2 = 0.0001
mu1_l2 = 0.01
tol_update = 1.0
[inversion.heidi]
gradient_mask_floor = 0.1
tol = 0.001
"#;

/// The settings whose unit changed after version 1, with what changed and what to do. A
/// version-1 file that gives one of them anything but its old default is refused.
const V1_UNIT_CHANGES: &[(&str, &str)] = &[
    ("inversion.tgv.alpha0", TGV_ALPHA),
    ("inversion.tgv.alpha1", TGV_ALPHA),
    ("inversion.medi.lambda",
     "MEDI's λ is now MEDI_L1's (MEDI toolbox), the weight of the data term on the field in radians \
      over the echo spacing, so the same number means a much weaker fit"),
    ("inversion.tfi.lambda",
     "TFI's λ is now MEDI_L1's (MEDI toolbox), the weight of the data term on the field in radians \
      over the echo spacing, so the same number means a much weaker fit"),
];

const TGV_ALPHA: &str =
    "TGV's weights were in ppm and are now in radians at the scan's first echo \
    time and field strength (as in QuantitativeSusceptibilityMappingTGV.jl), so the same number \
    regularises 2π·γ·B0·TE1 times more weakly";

/// Settings that no longer exist, and why: a warning when the file gives anything but the old
/// default (or, for a switch, turns it on).
const V1_REMOVED: &[(&str, &str, &str)] = &[
    (
        "bg_removal.msmv_refine",
        "false",
        "mSMV is no longer a step after background removal: it is now MEDI's option \
      (inversion.medi.msmv, --medi-msmv), which takes the local field from the background removal",
    ),
    ("bg_removal.msmv", "", ""),
    (
        "inversion.tgv.tol",
        "0.00001",
        "TGV now runs every iteration, without an early stop, as the reference does",
    ),
    (
        "inversion.fansi.phase_scale",
        "1.0",
        "FANSI converts the field to radians itself, at the scan's echo time and field strength",
    ),
    (
        "inversion.l1qsm.phase_scale",
        "1.0",
        "L1-QSM converts the field to radians itself, at the scan's echo time and field strength",
    ),
    (
        "inversion.whqsm.phase_scale",
        "1.0",
        "WH-QSM converts the field to radians itself, at the scan's echo time and field strength",
    ),
];

/// Settings renamed after version 1 with their meaning (and unit) unchanged.
const V1_RENAMED: &[(&str, &str)] = &[(
    "inversion.nltv.newton_iter",
    "inversion.nltv.max_iter_newton",
)];

/// Bring a parsed settings file to [`CONFIG_SCHEMA_VERSION`], adding a warning for each change the
/// caller should show; refuse a setting whose meaning changed and cannot be carried over.
pub fn migrate(value: &mut toml::Table, warnings: &mut Vec<String>) -> Result<(), ConfigError> {
    let stated = value
        .get("schema_version")
        .and_then(toml::Value::as_integer);
    // A TGV tolerance is only in files from before version 2, whatever they say.
    let version = if get(value, "inversion.tgv.tol").is_some() {
        1
    } else {
        stated.unwrap_or(1)
    };
    if version > CONFIG_SCHEMA_VERSION as i64 {
        warnings
            .push(format!(
            "schema_version = {version}: this file was written by a newer QSMxT (this one writes \
             {CONFIG_SCHEMA_VERSION}); settings it does not know are ignored"));
    }
    if version < 2 {
        from_v1(value, warnings)?;
    }
    value.insert(
        "schema_version".into(),
        toml::Value::Integer(CONFIG_SCHEMA_VERSION as i64),
    );
    Ok(())
}

fn from_v1(value: &mut toml::Table, warnings: &mut Vec<String>) -> Result<(), ConfigError> {
    let old: toml::Table = toml::from_str(V1_DEFAULTS).expect("V1_DEFAULTS is TOML");
    let today = toml::Table::try_from(crate::PipelineConfig::default())
        .expect("the default config is TOML");

    for (from, to) in V1_RENAMED {
        if let Some(v) = take(value, from) {
            if get(value, to).is_none() {
                set(value, to, v);
            }
        }
    }

    for (path, default, why) in V1_REMOVED {
        let Some(v) = take(value, path) else { continue };
        let old_default: Option<toml::Value> = (!default.is_empty()).then(|| {
            toml::from_str::<toml::Table>(&format!("v = {default}")).expect("a TOML value")["v"]
                .clone()
        });
        if old_default.is_some_and(|d| !same(&v, &d)) {
            warnings.push(format!(
                "{path} = {v} is no longer a setting and is ignored: {why}"
            ));
        }
    }

    // The phase-offset smoothing went from voxels to mm under a new name.
    if let Some(v) = take(value, "field_mapping.phase_offset_sigma") {
        let old_default = toml::Value::Array(vec![4.0.into(), 4.0.into(), 4.0.into()]);
        if !same(&v, &old_default) {
            let mm = qsm_core::pipeline::FieldMappingConfig::default().phase_offset_sigma_mm;
            return Err(ConfigError::Parse(format!(
                "field_mapping.phase_offset_sigma = {v} is in voxels, and the phase-offset smoothing \
                 is now set in mm (field_mapping.phase_offset_sigma_mm, the same width at every \
                 resolution). Replace it with phase_offset_sigma_mm (the sigma in voxels times the \
                 voxel size), or remove it for the default ({} × {} × {} mm)",
                mm[0], mm[1], mm[2])));
        }
        warnings.push(default_note(
            "field_mapping.phase_offset_sigma",
            &v,
            "field_mapping.phase_offset_sigma_mm",
            &today,
        ));
    }

    for (path, old_default) in leaves(&old, String::new()) {
        let Some(v) = get(value, &path) else { continue };
        if same(v, &old_default) {
            let v = take(value, &path).expect("present");
            warnings.push(default_note(&path, &v, &path, &today));
        } else if let Some((_, why)) = V1_UNIT_CHANGES.iter().find(|(p, _)| *p == path) {
            return Err(ConfigError::Parse(format!(
                "{path} = {v} comes from a settings file written by an earlier QSMxT, and {why}. \
                 Remove the setting to use the new default ({}), or give the value in the new \
                 unit and add `schema_version = {CONFIG_SCHEMA_VERSION}` at the top of the file",
                get(&today, &path).map_or_else(|| "automatic".to_string(), ToString::to_string)
            )));
        }
    }
    Ok(())
}

/// The log line for an old default that gives way to today's.
fn default_note(path: &str, v: &toml::Value, now: &str, today: &toml::Table) -> String {
    let new = get(today, now).map_or_else(|| "automatic".to_string(), ToString::to_string);
    if path == now {
        format!("{path} = {v} was the default when this file was written; using today's default ({new})")
    } else {
        format!("{path} = {v} was the default when this file was written; using today's default ({now} = {new})")
    }
}

/// Every leaf of `table` with its dotted path.
fn leaves(table: &toml::Table, prefix: String) -> Vec<(String, toml::Value)> {
    table
        .iter()
        .flat_map(|(k, v)| {
            let path = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            match v {
                toml::Value::Table(t) => leaves(t, path),
                v => vec![(path, v.clone())],
            }
        })
        .collect()
}

/// Whether two values are the same setting: numbers compared as numbers (to rounding), so `1e-3`,
/// `0.001` and an integer `1` for `1.0` all match.
fn same(a: &toml::Value, b: &toml::Value) -> bool {
    let num = |v: &toml::Value| v.as_float().or_else(|| v.as_integer().map(|i| i as f64));
    match (a, b) {
        (toml::Value::Array(x), toml::Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        _ => match (num(a), num(b)) {
            (Some(x), Some(y)) => (x - y).abs() <= 1e-9 * x.abs().max(y.abs()),
            _ => a == b,
        },
    }
}

fn get<'a>(table: &'a toml::Table, path: &str) -> Option<&'a toml::Value> {
    let (parent, key) = match path.rsplit_once('.') {
        Some((p, k)) => (Some(p), k),
        None => (None, path),
    };
    let mut t = table;
    for part in parent.into_iter().flat_map(|p| p.split('.')) {
        t = t.get(part)?.as_table()?;
    }
    t.get(key)
}

fn take(table: &mut toml::Table, path: &str) -> Option<toml::Value> {
    let (parent, key) = match path.rsplit_once('.') {
        Some((p, k)) => (Some(p), k),
        None => (None, path),
    };
    let mut t = table;
    for part in parent.into_iter().flat_map(|p| p.split('.')) {
        t = t.get_mut(part)?.as_table_mut()?;
    }
    t.remove(key)
}

fn set(table: &mut toml::Table, path: &str, v: toml::Value) {
    let (parent, key) = path.rsplit_once('.').unwrap_or(("", path));
    let mut t = table;
    for part in parent.split('.').filter(|p| !p.is_empty()) {
        t = t
            .entry(part)
            .or_insert_with(|| toml::Value::Table(Default::default()))
            .as_table_mut()
            .expect("a table");
    }
    t.insert(key.into(), v);
}

#[cfg(test)]
mod tests {
    use crate::PipelineConfig;

    /// A full settings file the previous QSMxT wrote: every setting at its default then.
    const V1_FULL: &str = include_str!("../tests/fixtures/config_v1_defaults.toml");

    /// Every old default gives way to today's, one line each; the result is today's defaults.
    #[test]
    fn a_full_old_file_at_its_defaults_reads_as_todays_defaults() {
        let (c, warnings) = PipelineConfig::from_toml_migrating(V1_FULL).unwrap();
        assert_eq!(c, PipelineConfig::default());
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("inversion.medi.lambda = 0.001 was the default")),
            "{warnings:#?}"
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("field_mapping.phase_offset_sigma_mm")),
            "{warnings:#?}"
        );
        // Dropping a setting at its default says nothing.
        assert!(
            !warnings
                .iter()
                .any(|w| w.contains("msmv_refine") || w.contains("tgv.tol")),
            "{warnings:#?}"
        );
        // A file written today reads back with no notes.
        let (back, warnings) = PipelineConfig::from_toml_migrating(&c.to_toml().unwrap()).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(back, c);
    }

    /// A non-default value whose unit did not change is kept as chosen.
    #[test]
    fn a_chosen_value_is_kept() {
        let old = V1_FULL.replace("radius = 6.0", "radius = 4.5");
        let c = PipelineConfig::from_toml(&old).unwrap();
        assert_eq!(c.bg_removal.resharp.radius, 4.5);
    }

    /// A chosen TGV α, MEDI λ or TFI λ, or a phase-offset σ in voxels, is refused with what
    /// changed; at the old default each gives way to today's.
    #[test]
    fn a_chosen_value_in_an_old_unit_is_refused() {
        for (from, to, says) in [
            (
                "alpha1 = 0.00006674916",
                "alpha1 = 0.0001",
                "radians at the scan's first echo time",
            ),
            (
                "[inversion.tfi]\nlambda = 0.000075",
                "[inversion.tfi]\nlambda = 0.0002",
                "TFI's λ is now MEDI_L1's",
            ),
        ] {
            let old = V1_FULL.replacen(from, to, 1);
            assert_ne!(old, V1_FULL);
            let e = PipelineConfig::from_toml(&old).unwrap_err().to_string();
            assert!(e.contains(says) && e.contains("schema_version = 2"), "{e}");
        }
        let medi = V1_FULL.replacen(
            "[inversion.medi]\nlambda = 0.001",
            "[inversion.medi]\nlambda = 0.002",
            1,
        );
        assert!(PipelineConfig::from_toml(&medi)
            .unwrap_err()
            .to_string()
            .contains("inversion.medi.lambda = 0.002"));
        let sigma = "[field_mapping]\nphase_offset_sigma = [3.0, 3.0, 3.0]\n";
        let e = PipelineConfig::from_toml(sigma).unwrap_err().to_string();
        assert!(
            e.contains("phase_offset_sigma_mm") && e.contains("7 × 7 × 7 mm"),
            "{e}"
        );
        // Saying the file is today's takes the value as given.
        let c = PipelineConfig::from_toml("schema_version = 2\n[inversion.medi]\nlambda = 0.002\n")
            .unwrap();
        assert_eq!(c.inversion.medi.lambda, 0.002);
    }

    /// An old file is recognised by its TGV tolerance even when it claims to be today's.
    #[test]
    fn an_old_file_is_recognised_by_its_tgv_tolerance() {
        let (c, w) = PipelineConfig::from_toml_migrating(
            "schema_version = 2\n[inversion.tgv]\ntol = 0.00001\niterations = 1000\n",
        )
        .unwrap();
        assert_eq!(c.inversion.tgv.iterations, None, "{w:?}");
        let (_, w) = PipelineConfig::from_toml_migrating("[inversion.tgv]\ntol = 0.001\n").unwrap();
        assert!(
            w.iter()
                .any(|w| w.contains("inversion.tgv.tol = 0.001 is no longer a setting")),
            "{w:?}"
        );
    }

    /// Settings that moved or went away: the NLTV Newton iterations keep their value under the new
    /// name, mSMV refinement is dropped with a warning, a phase scale is dropped.
    #[test]
    fn moved_and_removed_settings() {
        let (c, w) = PipelineConfig::from_toml_migrating(
            "[inversion.nltv]\nnewton_iter = 7\n[bg_removal]\nmsmv_refine = true\n[bg_removal.msmv]\nmaxk = 9\n\
             [inversion.fansi]\nphase_scale = 2.0\n").unwrap();
        assert_eq!(c.inversion.nltv.max_iter_newton, 7);
        assert!(!c.inversion.medi.msmv);
        assert!(
            w.iter()
                .any(|w| w.contains("bg_removal.msmv_refine") && w.contains("--medi-msmv")),
            "{w:?}"
        );
        assert!(
            w.iter()
                .any(|w| w.contains("inversion.fansi.phase_scale = 2.0")),
            "{w:?}"
        );
        let saved = c.to_toml().unwrap();
        assert!(
            saved.starts_with("description") || saved.contains("schema_version = 2"),
            "{saved}"
        );
        assert!(
            !saved.contains("msmv_refine")
                && !saved.contains("newton_iter =")
                && !saved.contains("phase_scale"),
            "{saved}"
        );
    }

    /// A file from a newer QSMxT is read with a warning.
    #[test]
    fn a_newer_file_is_read_with_a_warning() {
        let (_, w) = PipelineConfig::from_toml_migrating("schema_version = 99\n").unwrap();
        assert!(w.len() == 1 && w[0].contains("newer QSMxT"), "{w:?}");
    }
}
