//! Reading settings files written by earlier QSMxT versions.
//!
//! Each file QSMxT writes says which layout it was written with ([`CONFIG_SCHEMA_VERSION`],
//! `schema_version`). A file without one is read as follows.
//!
//! - **A full settings file an earlier QSMxT wrote** (`qsmxt init`, a run's
//!   `pipeline_config.toml`; recognised by its TGV tolerance, a setting every such file has and
//!   today's do not) holds every setting, so it pins the defaults of the release that wrote it as
//!   though they had been chosen. A value equal to one of the defaults an earlier release wrote
//!   ([`OLD_DEFAULTS`]) therefore counts as unset and takes today's default, with one note per
//!   setting. Any other value is kept as chosen.
//! - **Any other file without `schema_version`** (hand-written, or a changed-settings file) holds
//!   only what its author chose, so no value is reset.
//! - **A setting whose meaning changed** (TGV's α, MEDI's and TFI's λ, the phase-offset
//!   smoothing, now in mm) is refused when it holds anything but today's default (or, in a full
//!   file, an old default) and its method runs: the message says what changed and what to do,
//!   since converting it would need each scan's echo times, field strength or voxel size. When its
//!   method does not run, the value is dropped with a note.
//! - Settings that no longer exist are dropped, with a warning where dropping one changes what a
//!   run does; renamed ones keep their value under the new name.
//! - TFI's `merit` did nothing when these files were written, and is a setting again (MERIT, on
//!   by default): an old value is dropped, with a warning where it turned MERIT off in a file
//!   that runs TFI.

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

/// What reading a settings file changed: `warnings` change what a run does in a way its author may
/// not expect; `notes` say a setting took today's default.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Migration {
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
}

/// Every default an earlier release (QSMxT 9.0.0 to 10.0.0, and `main` before
/// [`CONFIG_SCHEMA_VERSION`] 2) wrote into its full settings files for a setting whose default has
/// since changed, by path, from each release's `qsmxt init`. In a full file from an earlier
/// release, a setting holding one of these takes today's default.
const OLD_DEFAULTS: &str = r#"
[bg_removal.vsharp]
min_radius = [1.0]
threshold = [0.001, 0.05]
[bg_removal.pdf]
tol = [0.00001]
[bg_removal.ismv]
tol = [0.001]
max_iter = [500]
[bg_removal.resharp]
radius = [6.0]
max_iter = [30]
[bg_removal.harperella]
tol = [0.000001]
[bg_removal.iharperella]
tol = [0.000001]
[inversion.tv]
lambda = [0.0002]
rho = [0.02]
[inversion.medi]
lambda = [0.000075, 0.001]
merit = [false]
percentage = [0.3]
cg_max_iter = [10]
max_iter = [30]
[inversion.tfi]
lambda = [0.000075]
[inversion.ilsqr]
tol = [0.01]
max_iter = [50, 100]
pad_mm = [16.0]
[inversion.tgv]
iterations = [1000]
alpha0 = [0.001, 0.0015, 0.0002002475]
alpha1 = [0.001, 0.0005, 0.00006674916]
[inversion.ndi]
max_iter = [200]
[inversion.fansi]
mu1 = [0.02]
alpha0 = [0.0004]
mu0 = [0.04]
[inversion.l1qsm]
alpha1 = [0.0002]
mu1 = [0.02]
lambda = [1.0]
[inversion.whqsm]
muh = [3.0]
max_iter = [300]
[inversion.hdqsm]
alpha_l2 = [0.0001]
mu1_l2 = [0.01]
tol_update = [1.0]
[inversion.heidi]
gradient_mask_floor = [0.1]
tol = [0.001]
[separation.chi_sep_medi]
dr_pos = [114.0]
dr_neg = [30.0]
lambda_para = [1000.0]
lambda_dia = [1000.0]
"#;

/// The phase-offset smoothing every earlier release wrote, in voxels.
const OLD_PHASE_OFFSET_SIGMA: [f64; 3] = [4.0, 4.0, 4.0];

/// The settings whose meaning changed, and the method that reads each.
#[derive(Clone, Copy, PartialEq)]
enum Changed {
    TgvAlpha,
    MediLambda,
    TfiLambda,
}

const UNIT_CHANGES: &[(&str, Changed)] = &[
    ("inversion.tgv.alpha0", Changed::TgvAlpha),
    ("inversion.tgv.alpha1", Changed::TgvAlpha),
    ("inversion.medi.lambda", Changed::MediLambda),
    ("inversion.tfi.lambda", Changed::TfiLambda),
];

/// Settings that no longer exist, and why: a warning when the file gives anything but the old
/// default (or, for a switch, turns it on).
const REMOVED: &[(&str, &str, &str)] = &[
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

/// Settings renamed with their meaning (and unit) unchanged.
const RENAMED: &[(&str, &str)] = &[(
    "inversion.nltv.newton_iter",
    "inversion.nltv.max_iter_newton",
)];

/// What kind of file without `schema_version` this is.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// A full file QSMxT 10 (or `main` before version 2) wrote.
    Full10,
    /// A full file a QSMxT 9 release wrote.
    Full9,
    /// Anything else: what the author chose.
    Partial,
}

/// Bring a parsed settings file to [`CONFIG_SCHEMA_VERSION`], noting each change the caller
/// should show; refuse a setting whose meaning changed and cannot be carried over.
pub fn migrate(value: &mut toml::Table, out: &mut Migration) -> Result<(), ConfigError> {
    let stated = value
        .get("schema_version")
        .and_then(toml::Value::as_integer);
    // A TGV tolerance is only in full files from before version 2, whatever they say.
    let full = get(value, "inversion.tgv.tol").is_some();
    let version = if full { 1 } else { stated.unwrap_or(1) };
    if version > CONFIG_SCHEMA_VERSION as i64 {
        out.warnings
            .push(format!(
            "schema_version = {version}: this file was written by a newer QSMxT (this one writes \
             {CONFIG_SCHEMA_VERSION}); settings it does not know are ignored"));
    }
    if version < 2 {
        let kind = match (full, get(value, "pipeline.axial_resampling").is_some()) {
            (false, _) => Kind::Partial,
            (true, true) => Kind::Full10,
            (true, false) => Kind::Full9,
        };
        from_v1(value, kind, out)?;
    }
    value.insert(
        "schema_version".into(),
        toml::Value::Integer(CONFIG_SCHEMA_VERSION as i64),
    );
    Ok(())
}

fn from_v1(value: &mut toml::Table, kind: Kind, out: &mut Migration) -> Result<(), ConfigError> {
    let old: toml::Table = toml::from_str(OLD_DEFAULTS).expect("OLD_DEFAULTS is TOML");
    let today = toml::Table::try_from(crate::PipelineConfig::default())
        .expect("the default config is TOML");
    let full = kind != Kind::Partial;
    // Which methods this file runs, read before anything is taken out of it.
    let runs = |c: Changed| method_runs(value, &today, c);
    let (tgv, medi, tfi) = (
        runs(Changed::TgvAlpha),
        runs(Changed::MediLambda),
        runs(Changed::TfiLambda),
    );
    let mcpc = {
        let s = |p: &str| get(value, p).or_else(|| get(&today, p)).cloned();
        s("field_mapping.unwrapping_algorithm")
            .and_then(|v| v.as_str().map(str::to_owned))
            .as_deref()
            == Some("romeo")
            && s("field_mapping.phase_offset_removal").and_then(|v| v.as_bool()) != Some(false)
    };

    for (from, to) in RENAMED {
        if let Some(v) = take(value, from) {
            if get(value, to).is_none() {
                set(value, to, v);
            }
        }
    }

    for (path, default, why) in REMOVED {
        let Some(v) = take(value, path) else { continue };
        let old_default: Option<toml::Value> = (!default.is_empty()).then(|| {
            toml::from_str::<toml::Table>(&format!("v = {default}")).expect("a TOML value")["v"]
                .clone()
        });
        if old_default.is_some_and(|d| !same(&v, &d)) {
            out.warnings.push(format!(
                "{path} = {v} is no longer a setting and is ignored: {why}"
            ));
        }
    }

    // TFI's `merit` did nothing before version 2 (TFI had no MERIT); it now turns MERIT on and off,
    // on by default, so an old value is not carried over.
    if let Some(v) = take(value, "inversion.tfi.merit") {
        let off = v.as_bool() == Some(false);
        if off && !full && tfi {
            out.warnings.push(format!(
                "inversion.tfi.merit = {v} had no effect when this file was written and is ignored: TFI \
                 now runs MERIT by default. To turn it off, keep the setting and add \
                 `schema_version = {CONFIG_SCHEMA_VERSION}` at the top of the file"));
        } else {
            out.notes.push(format!(
                "inversion.tfi.merit = {v} had no effect when this file was written and is ignored; \
                 TFI now runs MERIT by default"));
        }
    }

    // The phase-offset smoothing went from voxels to mm under a new name.
    if let Some(v) = take(value, "field_mapping.phase_offset_sigma") {
        let mm = qsm_core::pipeline::FieldMappingConfig::default().phase_offset_sigma_mm;
        let default_mm = format!("{} × {} × {} mm", mm[0], mm[1], mm[2]);
        let old_default =
            toml::Value::Array(OLD_PHASE_OFFSET_SIGMA.iter().map(|&x| x.into()).collect());
        if full && same(&v, &old_default) {
            out.notes.push(format!(
                "field_mapping.phase_offset_sigma = {v} (voxels) was QSMxT's default when this file was \
                 written; using today's default, field_mapping.phase_offset_sigma_mm = {default_mm}"));
        } else if mcpc {
            return Err(ConfigError::Parse(format!(
                "field_mapping.phase_offset_sigma = {v} is in voxels, and the phase-offset smoothing is \
                 now set in mm as field_mapping.phase_offset_sigma_mm (the same width at every \
                 resolution). Replace it with phase_offset_sigma_mm, the sigma in voxels times the \
                 voxel size, or remove it to use the default ({default_mm})")));
        } else {
            out.notes.push(format!(
                "field_mapping.phase_offset_sigma = {v} is ignored: it is in voxels, the setting is now \
                 phase_offset_sigma_mm (mm), and this configuration does not remove phase offsets"));
        }
    }

    for (path, olds) in leaves(&old, String::new()) {
        let Some(v) = get(value, &path) else { continue };
        let now = get(&today, &path);
        // Today's default is never a question.
        if now.is_some_and(|d| same(v, d)) {
            continue;
        }
        let now_text = now.map_or_else(|| "automatic".to_string(), ToString::to_string);
        let is_old_default = olds
            .as_array()
            .is_some_and(|a| a.iter().any(|o| same(v, o)));
        if full && is_old_default {
            let v = take(value, &path).expect("present");
            out.notes.push(format!(
                "{path} = {v} was QSMxT's default when this file was written; using today's default ({now_text})"));
            continue;
        }
        let Some(&(_, changed)) = UNIT_CHANGES.iter().find(|(p, _)| *p == path) else {
            continue;
        };
        let used = match changed {
            Changed::TgvAlpha => tgv,
            Changed::MediLambda => medi,
            Changed::TfiLambda => tfi,
        };
        if !used {
            let v = take(value, &path).expect("present");
            out.notes.push(format!(
                "{path} = {v} is ignored: {} does not run with this configuration, and the setting's \
                 meaning has changed since this file was written (it takes today's default, {now_text})",
                method_name(changed)));
            continue;
        }
        return Err(ConfigError::Parse(format!(
            "{path} = {v}: {} {} Remove the setting to use today's default ({now_text}), or give the \
             value in the new terms and add `schema_version = {CONFIG_SCHEMA_VERSION}` at the top of \
             the file.",
            what_changed(changed, kind), consequence(changed))));
    }
    Ok(())
}

/// Whether `changed`'s method runs with this file's settings (today's defaults where it says
/// nothing): the inversion, or QSMART's inner one.
fn method_runs(value: &toml::Table, today: &toml::Table, changed: Changed) -> bool {
    let s = |p: &str| {
        get(value, p)
            .or_else(|| get(today, p))
            .and_then(|v| v.as_str().map(str::to_owned))
    };
    let (inversion, inner) = (s("inversion.algorithm"), s("inversion.qsmart.inversion"));
    let is = |name: &str| {
        inversion.as_deref() == Some(name)
            || (inversion.as_deref() == Some("qsmart") && inner.as_deref() == Some(name))
    };
    match changed {
        Changed::TgvAlpha => is("tgv"),
        Changed::MediLambda => is("medi"),
        Changed::TfiLambda => is("tfi"),
    }
}

fn method_name(changed: Changed) -> &'static str {
    match changed {
        Changed::TgvAlpha => "TGV",
        Changed::MediLambda => "MEDI",
        Changed::TfiLambda => "TFI",
    }
}

/// What changed about the setting, for the kind of file it came from.
fn what_changed(changed: Changed, kind: Kind) -> String {
    let source = match kind {
        Kind::Full10 => "This file was written by QSMxT 10.",
        Kind::Full9 => "This file was written by QSMxT 9.",
        Kind::Partial => {
            "This file has no schema_version, so it is read as written for an earlier QSMxT."
        }
    };
    let before = match (changed, kind) {
        (Changed::TgvAlpha, Kind::Full10) => {
            "QSMxT 10 read TGV's weights in ppm, the same at every echo time and field strength."
        }
        (Changed::TgvAlpha, Kind::Full9) => {
            "QSMxT 9 applied TGV's weights to the first echo's phase for a single echo, but to the \
             field map in ppm, read as though it were phase, for several echoes."
        }
        (Changed::TgvAlpha, Kind::Partial) => {
            "Earlier QSMxT versions read TGV's weights in other units (ppm in QSMxT 10)."
        }
        (Changed::MediLambda | Changed::TfiLambda, _) => {
            "Earlier QSMxT versions passed λ to a solver of the field in radians at the first echo \
             time, weighted uniformly."
        }
    };
    format!("{source} {before}")
}

/// What the setting means now, and what that does to an old value.
fn consequence(changed: Changed) -> &'static str {
    match changed {
        Changed::TgvAlpha =>
            "They are now in radians at the scan's first echo time and field strength, as \
             QuantitativeSusceptibilityMappingTGV.jl defines them (a multi-echo field map is \
             expressed as phase at the first echo time), so the same number regularises differently.",
        Changed::MediLambda =>
            "MEDI is now MEDI_L1, whose λ weights the data term on the field in radians over the echo \
             spacing, weighted by the noise map, so the same number is a much weaker fit.",
        Changed::TfiLambda =>
            "TFI's λ now weights the data term on the field in radians over the echo spacing, weighted \
             by the noise map, as MEDI_L1's does, so the same number is a much weaker fit.",
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
    use crate::{PipelineConfig, QsmAlgorithm};

    /// A full settings file `main` wrote before version 2 (QSMxT 10's layout): every setting at its
    /// default then.
    const V10_FULL: &str = include_str!("../tests/fixtures/config_v1_defaults.toml");
    /// A full settings file QSMxT 9.22.0's `qsmxt init` wrote.
    const V9_FULL: &str = include_str!("../tests/fixtures/config_v9_22_0.toml");

    fn read(toml: &str) -> crate::Result<(PipelineConfig, super::Migration)> {
        PipelineConfig::from_toml_noting(toml)
    }

    /// Every old default gives way to today's, one note each; the result is today's defaults.
    #[test]
    fn a_full_old_file_at_its_defaults_reads_as_todays_defaults() {
        let (c, m) = read(V10_FULL).unwrap();
        assert_eq!(c, PipelineConfig::default());
        assert!(
            m.notes
                .iter()
                .any(|w| w.contains("inversion.medi.lambda = 0.001 was QSMxT's default")),
            "{m:#?}"
        );
        assert!(
            m.notes
                .iter()
                .any(|w| w.contains("field_mapping.phase_offset_sigma_mm")),
            "{m:#?}"
        );
        assert!(m.warnings.is_empty(), "{m:#?}");
        // A file written today reads back with nothing to say.
        let (back, m) = read(&c.to_toml().unwrap()).unwrap();
        assert_eq!((back, m), (c, super::Migration::default()));
    }

    /// QSMxT 9.22.0's full file, with iLSQR: it loads, one note per setting reset, and its TGV
    /// weights (QSMxT 9's defaults) take today's even though TGV does not run.
    #[test]
    fn a_v9_full_file_loads() {
        let file = V9_FULL.replacen("algorithm = \"rts\"", "algorithm = \"ilsqr\"", 1);
        assert_ne!(file, V9_FULL);
        let (c, m) = read(&file).unwrap();
        assert_eq!(c.inversion.algorithm, QsmAlgorithm::Ilsqr);
        assert_eq!(c.inversion.tgv, PipelineConfig::default().inversion.tgv);
        assert_eq!(c.inversion.ilsqr, PipelineConfig::default().inversion.ilsqr);
        let resets = m
            .notes
            .iter()
            .filter(|n| n.contains("using today's default"))
            .count();
        assert!(resets >= 10, "{m:#?}");
        assert_eq!(
            m.notes
                .iter()
                .filter(|n| n.starts_with("inversion.tgv.alpha1 "))
                .count(),
            1,
            "{m:#?}"
        );
    }

    /// The same file with TGV selected: QSMxT 9's default weights take today's; a weight of its
    /// own is refused, saying what QSMxT 9 did with it and what to do.
    #[test]
    fn a_v9_file_with_tgv_and_its_own_weights_is_refused() {
        let tgv = V9_FULL.replacen("algorithm = \"rts\"", "algorithm = \"tgv\"", 1);
        let (c, _) = read(&tgv).unwrap();
        assert_eq!(
            c.inversion.tgv.alpha1,
            PipelineConfig::default().inversion.tgv.alpha1
        );
        let own = tgv.replacen("alpha1 = 0.0005", "alpha1 = 0.0008", 1);
        assert_ne!(own, tgv);
        let e = read(&own).unwrap_err().to_string();
        assert!(
            e.contains("inversion.tgv.alpha1 = 0.0008")
                && e.contains("QSMxT 9 applied")
                && e.contains("radians at the scan's first echo time")
                && e.contains("schema_version = 2")
                && !e.contains("were in ppm"),
            "{e}"
        );
        // Under another inversion the same value is dropped with a note.
        let (_, m) =
            read(&own.replacen("algorithm = \"tgv\"", "algorithm = \"ilsqr\"", 1)).unwrap();
        assert!(
            m.notes
                .iter()
                .any(|n| n.contains("inversion.tgv.alpha1 = 0.0008 is ignored: TGV does not run")),
            "{m:#?}"
        );
    }

    /// QSMxT 10's (ppm) weights, and MEDI's and TFI's λ, are refused when their method runs.
    #[test]
    fn a_v10_file_with_its_own_weights_is_refused_when_they_are_used() {
        let tgv = V10_FULL
            .replacen("algorithm = \"ilsqr\"", "algorithm = \"tgv\"", 1)
            .replacen("alpha1 = 0.00006674916", "alpha1 = 0.0001", 1);
        let e = read(&tgv).unwrap_err().to_string();
        assert!(e.contains("QSMxT 10 read TGV's weights in ppm"), "{e}");
        let medi = V10_FULL
            .replacen("algorithm = \"ilsqr\"", "algorithm = \"medi\"", 1)
            .replacen(
                "[inversion.medi]\nlambda = 0.001",
                "[inversion.medi]\nlambda = 0.002",
                1,
            );
        assert!(read(&medi).unwrap_err().to_string().contains("MEDI_L1"));
        // QSMART's inner MEDI runs MEDI too.
        let inner = V10_FULL
            .replacen("algorithm = \"ilsqr\"", "algorithm = \"qsmart\"", 1)
            .replacen("inversion = \"ilsqr\"", "inversion = \"medi\"", 1)
            .replacen(
                "[inversion.medi]\nlambda = 0.001",
                "[inversion.medi]\nlambda = 0.002",
                1,
            );
        assert!(read(&inner).is_err());
    }

    /// A hand-written file holds what its author chose: today's MEDI λ is accepted silently, and
    /// iLSQR's old iteration count is kept, not reset.
    #[test]
    fn a_hand_written_file_keeps_its_values() {
        let (c, m) =
            read("[inversion]\nalgorithm = \"medi\"\n[inversion.medi]\nlambda = 1000.0\n").unwrap();
        assert_eq!(
            (c.inversion.medi.lambda, m),
            (1000.0, super::Migration::default())
        );
        let (c, m) = read("[inversion.ilsqr]\nmax_iter = 100\n").unwrap();
        assert_eq!(
            (c.inversion.ilsqr.max_iter, m),
            (100, super::Migration::default())
        );
        // A value whose meaning changed is still refused when its method runs.
        let e = read("[inversion]\nalgorithm = \"medi\"\n[inversion.medi]\nlambda = 0.002\n")
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("has no schema_version") && e.contains("schema_version = 2"),
            "{e}"
        );
        // Saying the file is today's takes the value as given.
        let c = PipelineConfig::from_toml("schema_version = 2\n[inversion.medi]\nlambda = 0.002\n")
            .unwrap();
        assert_eq!(c.inversion.medi.lambda, 0.002);
    }

    /// A phase-offset σ in voxels is refused when phase offsets are removed, dropped otherwise.
    #[test]
    fn a_phase_offset_sigma_in_voxels() {
        let e = read("[field_mapping]\nphase_offset_sigma = [3.0, 3.0, 3.0]\n")
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("phase_offset_sigma_mm") && e.contains("7 × 7 × 7 mm"),
            "{e}"
        );
        let (_, m) = read(
            "[field_mapping]\nphase_offset_removal = false\nphase_offset_sigma = [3.0, 3.0, 3.0]\n",
        )
        .unwrap();
        assert!(m.notes.iter().any(|n| n.contains("is ignored")), "{m:#?}");
    }

    /// Settings that moved or went away: the NLTV Newton iterations keep their value under the new
    /// name, mSMV refinement is dropped with a warning, a phase scale is dropped.
    #[test]
    fn moved_and_removed_settings() {
        let (c, m) = read(
            "[inversion.nltv]\nnewton_iter = 7\n[bg_removal]\nmsmv_refine = true\n[bg_removal.msmv]\nmaxk = 9\n\
             [inversion.fansi]\nphase_scale = 2.0\n").unwrap();
        assert_eq!(c.inversion.nltv.max_iter_newton, 7);
        assert!(!c.inversion.medi.msmv);
        assert!(
            m.warnings
                .iter()
                .any(|w| w.contains("bg_removal.msmv_refine") && w.contains("--medi-msmv")),
            "{m:?}"
        );
        assert!(
            m.warnings
                .iter()
                .any(|w| w.contains("inversion.fansi.phase_scale = 2.0")),
            "{m:?}"
        );
        let saved = c.to_toml().unwrap();
        assert!(saved.contains("schema_version = 2"), "{saved}");
        assert!(
            !saved.contains("msmv_refine")
                && !saved.contains("newton_iter =")
                && !saved.contains("phase_scale"),
            "{saved}"
        );
    }

    /// A front end's saved JSON config (qsmbly keeps one in the browser) is migrated the same way:
    /// an old full one takes today's defaults; one with `schema_version` is read as it is.
    #[test]
    fn a_saved_json_config_is_migrated_too() {
        let old: toml::Table = toml::from_str(V10_FULL).unwrap();
        let json = serde_json::to_string(&old).unwrap();
        let (c, m) = PipelineConfig::from_json_migrating(&json).unwrap();
        assert_eq!(c, PipelineConfig::default());
        assert!(!m.notes.is_empty());
        let mut today = PipelineConfig::default();
        today.inversion.medi.lambda = 0.002;
        let (back, m) = PipelineConfig::from_json_migrating(&today.to_json().unwrap()).unwrap();
        assert_eq!((back, m), (today, super::Migration::default()));
    }

    /// A file from a newer QSMxT is read with a warning.
    #[test]
    fn a_newer_file_is_read_with_a_warning() {
        let (_, m) = read("schema_version = 99\n").unwrap();
        assert!(
            m.warnings.len() == 1 && m.warnings[0].contains("newer QSMxT"),
            "{m:?}"
        );
    }

    /// TFI's `merit` did nothing before version 2: it is dropped from older files (a warning where
    /// it turned MERIT off for TFI, by hand), and a version 2 file keeps it.
    #[test]
    fn an_old_tfi_merit_is_dropped() {
        let partial = "[inversion]\nalgorithm = \"tfi\"\n[inversion.tfi]\nmerit = false\n";
        let (c, m) = read(partial).unwrap();
        assert!(
            c.inversion.tfi.merit
                && m.warnings.len() == 1
                && m.warnings[0].contains("inversion.tfi.merit"),
            "{m:?}"
        );
        let (c, m) = read("[inversion.tfi]\nmerit = false\n").unwrap();
        assert!(
            c.inversion.tfi.merit && m.warnings.is_empty() && m.notes.len() == 1,
            "TFI does not run: {m:?}"
        );
        let (c, _) = read(&format!("schema_version = 2\n{partial}")).unwrap();
        assert!(
            !c.inversion.tfi.merit,
            "a version 2 file says what it means"
        );
    }
}
