//! A scan's facts from the files a front end holds: each echo's NIfTI headers and BIDS sidecar,
//! and the values the user typed in for what the files do not say.
//!
//! [`ScanInputs::facts`] decides the [`ScanFacts`] a run plans with, says where each fact came
//! from, and finds what is missing or inconsistent before anything is computed, attached to the
//! fact and the file concerned. A front end that loads files itself (qsmbly) builds its facts
//! here, so it decides nothing about them on its own.

use serde::{Deserialize, Serialize};

use crate::requirements::Severity;
use crate::scan::{QsmSidecar, ScanFacts};
use crate::store::Geometry;

/// One echo of a magnitude/phase scan, as the front end holds it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EchoInput {
    /// The phase image's header.
    #[serde(default)]
    pub phase: Option<Geometry>,
    /// The magnitude image's header, if there is one.
    #[serde(default)]
    pub magnitude: Option<Geometry>,
    /// The echo's BIDS sidecar (the phase image's), as parsed JSON.
    #[serde(default)]
    pub sidecar: Option<serde_json::Value>,
}

/// Values typed in by the user. Each one, when given, wins over the files.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FactOverrides {
    /// Every echo's echo time, in seconds, in echo order.
    #[serde(default)]
    pub echo_times: Option<Vec<f64>>,
    /// Tesla.
    #[serde(default)]
    pub field_strength: Option<f64>,
    /// B0 direction in voxel coordinates, as a sidecar's `B0_dir` gives it.
    #[serde(default)]
    pub b0_dir: Option<(f64, f64, f64)>,
}

/// What the front end holds of one scan: its echoes in order, and what the user typed in.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScanInputs {
    pub echoes: Vec<EchoInput>,
    #[serde(default)]
    pub overrides: FactOverrides,
}

/// Where a fact's value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// The images' NIfTI headers.
    Header,
    /// The echoes' JSON sidecars.
    Sidecar,
    /// Typed in by the user ([`FactOverrides`]).
    Typed,
    /// Which images were given.
    Files,
    /// Nothing gives it.
    Missing,
}

/// The facts a scan's view shows, by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactName {
    Dims,
    VoxelSize,
    Echoes,
    EchoTimes,
    FieldStrength,
    B0Dir,
    Magnitude,
    Manufacturer,
}

impl FactName {
    pub fn label(self) -> &'static str {
        match self {
            FactName::Dims => "Matrix size",
            FactName::VoxelSize => "Voxel size",
            FactName::Echoes => "Echoes",
            FactName::EchoTimes => "Echo times",
            FactName::FieldStrength => "Field strength",
            FactName::B0Dir => "B0 direction",
            FactName::Magnitude => "Magnitude images",
            FactName::Manufacturer => "Manufacturer",
        }
    }

    pub fn units(self) -> Option<&'static str> {
        match self {
            FactName::VoxelSize => Some("mm"),
            FactName::EchoTimes => Some("s"),
            FactName::FieldStrength => Some("T"),
            _ => None,
        }
    }

    /// Whether the user can type a value in for it ([`FactOverrides`]).
    pub fn typed(self) -> bool {
        matches!(self, FactName::EchoTimes | FactName::FieldStrength | FactName::B0Dir)
    }
}

/// One fact, its value and where it came from.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Fact {
    pub name: FactName,
    pub label: &'static str,
    pub units: Option<&'static str>,
    /// `null` when nothing gives it.
    pub value: serde_json::Value,
    pub source: Source,
    /// Whether the user can type a value in for it.
    pub typed: bool,
    /// A note on the value, e.g. how B0 is found when no sidecar gives it.
    pub note: Option<String>,
}

/// Which of an echo's files a problem concerns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    Phase,
    Magnitude,
    Sidecar,
}

/// Something missing or inconsistent in the inputs. An error means the scan cannot be run as it
/// is; a warning that it runs, with the consequence the message says.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InputProblem {
    pub severity: Severity,
    /// The fact it concerns, if one.
    pub fact: Option<FactName>,
    /// The echo it concerns (from 1), if one.
    pub echo: Option<usize>,
    /// Which of that echo's files.
    pub part: Option<Part>,
    pub message: String,
}

/// What [`ScanInputs::facts`] found.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScanReport {
    /// The facts a run plans with; `None` while a fact a plan needs is missing (the problems say
    /// which).
    pub facts: Option<ScanFacts>,
    /// Each fact, in the order a view shows them, with its source.
    pub shown: Vec<Fact>,
    pub problems: Vec<InputProblem>,
    /// Each echo's sidecar as a run reads it: the file's, with the facts above in place of what
    /// it lacked or the user typed. Empty while `facts` is `None`.
    pub sidecars: Vec<QsmSidecar>,
}

impl ScanReport {
    /// Whether any problem is an error.
    pub fn has_errors(&self) -> bool {
        self.problems.iter().any(|p| p.severity == Severity::Error)
    }
}

/// A sidecar's numeric field, if it has it.
fn number(sidecar: Option<&serde_json::Value>, field: &str) -> Option<f64> {
    sidecar?.get(field)?.as_f64()
}

impl ScanInputs {
    /// The scan's facts, where each came from, and what is wrong with the inputs.
    ///
    /// - Matrix size and voxel size are echo 1's phase header's. Every other image must have the
    ///   same matrix size (an error otherwise); one placed or oriented differently with the same
    ///   size is a warning, since its voxels are then combined as if on one grid.
    /// - Every echo needs a phase image. Magnitude is used when every echo has one; some but not
    ///   all is an error.
    /// - Echo times are the typed ones, else each sidecar's `EchoTime`. Field strength is the
    ///   typed one, else the sidecars' `MagneticFieldStrength` (the last echo that gives one, as
    ///   `qsmxt` reads a BIDS run). `B0_dir` likewise; without one, a run takes B0 from the
    ///   image's affine (or resamples, as the run's settings decide).
    pub fn facts(&self) -> ScanReport {
        let mut problems = Vec::new();
        let mut problem = |severity, fact, echo, part, message: String| {
            problems.push(InputProblem { severity, fact, echo, part, message });
        };
        let n = self.echoes.len();
        let first = self.echoes.first().and_then(|e| e.phase.as_ref());
        if n == 0 {
            problem(Severity::Error, Some(FactName::Echoes), None, Some(Part::Phase),
                "No phase images: add each echo's phase image".into());
        }

        // Every image on echo 1's phase grid.
        for (e, echo) in self.echoes.iter().enumerate() {
            let number = e + 1;
            if echo.phase.is_none() {
                problem(Severity::Error, None, Some(number), Some(Part::Phase),
                    format!("Echo {number} has no phase image"));
            }
            let Some(reference) = first else { continue };
            for (part, grid) in [(Part::Phase, &echo.phase), (Part::Magnitude, &echo.magnitude)] {
                let Some(grid) = grid else { continue };
                if number == 1 && part == Part::Phase {
                    continue;
                }
                let what = match part {
                    Part::Phase => "phase",
                    _ => "magnitude",
                };
                if grid.dims != reference.dims {
                    let size = |g: &Geometry| format!("{}×{}×{}", g.dims.0, g.dims.1, g.dims.2);
                    problem(Severity::Error, Some(FactName::Dims), Some(number), Some(part), format!(
                        "Echo {number}'s {what} image is {} but echo 1's phase image is {}: every \
                         image of a scan must come from the same acquisition",
                        size(grid), size(reference)));
                } else if !crate::stages::r2::same_grid(grid.dims, &grid.affine, reference.dims, &reference.affine) {
                    problem(Severity::Warning, Some(FactName::Dims), Some(number), Some(part), format!(
                        "Echo {number}'s {what} image has the same matrix size as echo 1's phase \
                         image but a different position, orientation or voxel size: they are \
                         combined as if on one grid"));
                }
            }
        }

        // Magnitude: all echoes, or none.
        let with_magnitude = self.echoes.iter().filter(|e| e.magnitude.is_some()).count();
        let has_magnitude = n > 0 && with_magnitude == n;
        if with_magnitude > 0 && with_magnitude < n {
            for (e, echo) in self.echoes.iter().enumerate() {
                if echo.magnitude.is_none() {
                    problem(Severity::Error, Some(FactName::Magnitude), Some(e + 1), Some(Part::Magnitude),
                        format!("Echo {} has no magnitude image: give every echo's magnitude, or none", e + 1));
                }
            }
        }

        // Echo times.
        let typed_tes = self.overrides.echo_times.as_ref();
        let mut echo_times = Vec::with_capacity(n);
        let te_source = if let Some(tes) = typed_tes {
            if tes.len() != n {
                problem(Severity::Error, Some(FactName::EchoTimes), None, None, format!(
                    "{} echo times were typed for {n} echo{}", tes.len(), if n == 1 { "" } else { "es" }));
            } else if let Some(bad) = tes.iter().position(|t| !(t.is_finite() && *t > 0.0)) {
                problem(Severity::Error, Some(FactName::EchoTimes), Some(bad + 1), None,
                    format!("Echo {}'s echo time must be a positive number", bad + 1));
            } else {
                echo_times = tes.clone();
            }
            Source::Typed
        } else {
            for (e, echo) in self.echoes.iter().enumerate() {
                match number(echo.sidecar.as_ref(), "EchoTime") {
                    Some(te) => echo_times.push(te),
                    None => problem(Severity::Error, Some(FactName::EchoTimes), Some(e + 1),
                        Some(Part::Sidecar), format!(
                            "Echo {} has no echo time: add its JSON sidecar, or type the echo times",
                            e + 1)),
                }
            }
            if echo_times.len() == n && n > 0 { Source::Sidecar } else { Source::Missing }
        };
        let echo_times_known = echo_times.len() == n && n > 0;

        // Field strength and B0 direction: typed, else the last sidecar that gives one.
        let sidecars = || self.echoes.iter().filter_map(|e| e.sidecar.as_ref());
        let (field_strength, field_source) = match self.overrides.field_strength {
            Some(t) => (Some(t), Source::Typed),
            None => match sidecars().rev().find_map(|s| number(Some(s), "MagneticFieldStrength")) {
                Some(t) => (Some(t), Source::Sidecar),
                None => (None, Source::Missing),
            },
        };
        match field_strength {
            None if n > 0 => problem(Severity::Error, Some(FactName::FieldStrength), None, Some(Part::Sidecar),
                "No field strength: add the JSON sidecars, or type it".into()),
            Some(t) if !(t.is_finite() && t > 0.0) => problem(Severity::Error, Some(FactName::FieldStrength),
                None, None, "The field strength must be a positive number of tesla".into()),
            _ => {}
        }
        let mut b0_dir = None;
        let mut b0_source = Source::Missing;
        if let Some(dir) = self.overrides.b0_dir {
            b0_dir = Some(dir);
            b0_source = Source::Typed;
        } else {
            for (e, echo) in self.echoes.iter().enumerate() {
                let Some(value) = echo.sidecar.as_ref().and_then(|s| s.get("B0_dir")) else { continue };
                let parts: Vec<f64> = value.as_array().map(|a| a.iter().filter_map(|x| x.as_f64()).collect())
                    .unwrap_or_default();
                if let [x, y, z] = parts[..] {
                    b0_dir = Some((x, y, z));
                    b0_source = Source::Sidecar;
                } else {
                    problem(Severity::Warning, Some(FactName::B0Dir), Some(e + 1), Some(Part::Sidecar), format!(
                        "Echo {}'s sidecar gives a B0 direction with {} components (three are needed): \
                         it is ignored", e + 1, parts.len()));
                }
            }
        }

        let manufacturer = sidecars().rev().find_map(|s| s.get("Manufacturer")?.as_str());
        let fact = |name: FactName, value: serde_json::Value, source: Source, note: Option<String>| Fact {
            name, label: name.label(), units: name.units(), value, source, typed: name.typed(), note,
        };
        let grid_value = |f: &dyn Fn(&Geometry) -> serde_json::Value| first.map_or(serde_json::Value::Null, f);
        let header = |known: bool| if known { Source::Header } else { Source::Missing };
        let shown = vec![
            fact(FactName::Dims, grid_value(&|g| serde_json::json!([g.dims.0, g.dims.1, g.dims.2])),
                header(first.is_some()), None),
            fact(FactName::VoxelSize, grid_value(&|g| serde_json::json!([g.voxel_size.0, g.voxel_size.1, g.voxel_size.2])),
                header(first.is_some()), None),
            fact(FactName::Echoes, n.into(), Source::Files, None),
            fact(FactName::EchoTimes, if echo_times_known { echo_times.clone().into() } else { serde_json::Value::Null },
                if echo_times_known { te_source } else { Source::Missing }, None),
            fact(FactName::FieldStrength, field_strength.map_or(serde_json::Value::Null, Into::into), field_source, None),
            fact(FactName::B0Dir, b0_dir.map_or(serde_json::Value::Null, |(x, y, z)| serde_json::json!([x, y, z])),
                b0_source, b0_dir.is_none().then(|| {
                    "From the image's orientation, or resampled to axial where the chosen methods need it".to_string()
                })),
            fact(FactName::Magnitude, has_magnitude.into(), Source::Files, None),
            fact(FactName::Manufacturer, manufacturer.map_or(serde_json::Value::Null, Into::into),
                if manufacturer.is_some() { Source::Sidecar } else { Source::Missing }, None),
        ];

        let usable = n > 0 && echo_times_known && first.is_some()
            && field_strength.is_some_and(|t| t.is_finite() && t > 0.0);
        let (facts, sidecars) = match (usable, first, field_strength) {
            (true, Some(grid), Some(field_strength)) => {
                let sidecars = self.echoes.iter().zip(&echo_times).map(|(echo, &te)| {
                    let mut sc = echo.sidecar.as_ref()
                        .and_then(|v| {
                            let mut v = v.clone();
                            let obj = v.as_object_mut()?;
                            obj.insert("EchoTime".into(), te.into());
                            obj.insert("MagneticFieldStrength".into(), field_strength.into());
                            QsmSidecar::from_json(&v).ok()
                        })
                        .unwrap_or_else(|| QsmSidecar::bare(te, field_strength));
                    sc.b0_dir = b0_dir.map(|(x, y, z)| vec![x, y, z]);
                    sc
                }).collect();
                let facts = ScanFacts {
                    dims: grid.dims,
                    n_echoes: n,
                    echo_times,
                    has_magnitude,
                    n_coils: 1,
                    real_imaginary: false,
                    field_strength,
                    b0_dir,
                    mese_echo_times: None,
                    custom_mask: false,
                    custom_dseg: false,
                    custom_qsm: false,
                    custom_r2: false,
                    custom_r2prime: false,
                };
                (Some(facts), sidecars)
            }
            _ => (None, Vec::new()),
        };
        ScanReport { facts, shown, problems, sidecars }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const AFFINE: [f64; 16] = [1., 0., 0., -2., 0., 1., 0., -2., 0., 0., 1., -2., 0., 0., 0., 1.];

    fn grid(dims: (usize, usize, usize)) -> Geometry {
        Geometry { dims, voxel_size: (1.0, 1.0, 1.0), affine: AFFINE }
    }

    fn echo(te: Option<f64>, magnitude: bool) -> EchoInput {
        EchoInput {
            phase: Some(grid((4, 4, 4))),
            magnitude: magnitude.then(|| grid((4, 4, 4))),
            sidecar: te.map(|te| json!({ "EchoTime": te, "MagneticFieldStrength": 3.0, "Manufacturer": "Siemens" })),
        }
    }

    fn source(report: &ScanReport, name: FactName) -> Source {
        report.shown.iter().find(|f| f.name == name).unwrap().source
    }

    #[test]
    fn facts_come_from_the_headers_and_sidecars() {
        let inputs = ScanInputs { echoes: vec![echo(Some(0.004), true), echo(Some(0.008), true)], ..Default::default() };
        let report = inputs.facts();
        assert!(report.problems.is_empty(), "{:?}", report.problems);
        let facts = report.facts.clone().unwrap();
        assert_eq!((facts.dims, facts.n_echoes, facts.has_magnitude), ((4, 4, 4), 2, true));
        assert_eq!(facts.echo_times, [0.004, 0.008]);
        assert_eq!((facts.field_strength, facts.b0_dir), (3.0, None));
        assert_eq!(source(&report, FactName::Dims), Source::Header);
        assert_eq!(source(&report, FactName::EchoTimes), Source::Sidecar);
        assert_eq!(source(&report, FactName::FieldStrength), Source::Sidecar);
        assert_eq!(source(&report, FactName::B0Dir), Source::Missing);
        assert_eq!(source(&report, FactName::Manufacturer), Source::Sidecar);
        // The sidecars a run reads keep what the files say.
        assert_eq!(report.sidecars.len(), 2);
        assert_eq!(report.sidecars[1].echo_time, 0.008);
        assert_eq!(report.sidecars[0].manufacturer.as_deref(), Some("Siemens"));
        // The same facts as from the sidecars alone.
        let sidecars: Vec<_> = inputs.echoes.iter().map(|e| QsmSidecar::from_json(e.sidecar.as_ref().unwrap()).unwrap()).collect();
        assert_eq!(report.facts, Some(ScanFacts::from_sidecars((4, 4, 4), &sidecars, true).unwrap()));
    }

    #[test]
    fn typed_values_win_and_fill_what_the_files_lack() {
        let mut inputs = ScanInputs { echoes: vec![echo(None, false), echo(None, false)], ..Default::default() };
        let report = inputs.facts();
        assert!(report.facts.is_none());
        let missing: Vec<_> = report.problems.iter().map(|p| (p.fact, p.echo)).collect();
        assert_eq!(missing, [(Some(FactName::EchoTimes), Some(1)), (Some(FactName::EchoTimes), Some(2)),
            (Some(FactName::FieldStrength), None)]);
        assert!(report.problems.iter().all(|p| p.severity == Severity::Error));

        inputs.overrides = FactOverrides { echo_times: Some(vec![0.005, 0.01]), field_strength: Some(7.0), b0_dir: Some((0.0, 0.1, 0.99)) };
        let report = inputs.facts();
        assert!(report.problems.is_empty(), "{:?}", report.problems);
        let facts = report.facts.clone().unwrap();
        assert_eq!((facts.echo_times.clone(), facts.field_strength, facts.b0_dir), (vec![0.005, 0.01], 7.0, Some((0.0, 0.1, 0.99))));
        assert!(!facts.has_magnitude);
        for name in [FactName::EchoTimes, FactName::FieldStrength, FactName::B0Dir] {
            assert_eq!(source(&report, name), Source::Typed);
        }
        assert_eq!(report.sidecars[1].echo_time, 0.01);
        assert_eq!(report.sidecars[0].b0_dir, Some(vec![0.0, 0.1, 0.99]));

        // A typed value wins over a sidecar's.
        let mut inputs = ScanInputs { echoes: vec![echo(Some(0.004), true)], ..Default::default() };
        inputs.overrides.field_strength = Some(2.89);
        let report = inputs.facts();
        assert_eq!(report.facts.unwrap().field_strength, 2.89);
        assert_eq!(report.sidecars[0].magnetic_field_strength, 2.89);

        // Typed echo times must be one per echo.
        inputs.overrides.echo_times = Some(vec![0.004, 0.008]);
        let report = inputs.facts();
        assert!(report.facts.is_none());
        assert_eq!(report.problems[0].fact, Some(FactName::EchoTimes));
    }

    #[test]
    fn images_must_share_a_grid_and_magnitude_is_all_or_none() {
        let mut inputs = ScanInputs { echoes: vec![echo(Some(0.004), true), echo(Some(0.008), true)], ..Default::default() };
        inputs.echoes[1].magnitude = Some(grid((4, 4, 5)));
        let report = inputs.facts();
        let p = &report.problems[0];
        assert_eq!((p.severity, p.echo, p.part), (Severity::Error, Some(2), Some(Part::Magnitude)));
        assert!(p.message.contains("4×4×5") && p.message.contains("4×4×4"), "{}", p.message);
        assert!(report.has_errors());

        // Same size, moved: a warning.
        let mut moved = grid((4, 4, 4));
        moved.affine[3] += 5.0;
        inputs.echoes[1].magnitude = Some(moved);
        let report = inputs.facts();
        assert_eq!(report.problems.len(), 1);
        assert_eq!(report.problems[0].severity, Severity::Warning);
        assert!(report.facts.is_some() && !report.has_errors());

        // Magnitude for some echoes only.
        inputs.echoes[1].magnitude = None;
        let report = inputs.facts();
        assert_eq!(report.problems.len(), 1);
        assert_eq!((report.problems[0].fact, report.problems[0].echo), (Some(FactName::Magnitude), Some(2)));

        // An echo without its phase image.
        inputs.echoes[1].phase = None;
        assert!(inputs.facts().problems.iter().any(|p| p.part == Some(Part::Phase) && p.echo == Some(2)));
        assert!(ScanInputs::default().facts().has_errors());
    }

    #[test]
    fn b0_direction_is_read_as_qsmxt_reads_it() {
        let mut inputs = ScanInputs { echoes: vec![echo(Some(0.004), true), echo(Some(0.008), true)], ..Default::default() };
        inputs.echoes[0].sidecar.as_mut().unwrap()["B0_dir"] = json!([0.0, 0.1, 0.99]);
        inputs.echoes[1].sidecar.as_mut().unwrap()["B0_dir"] = json!([1.0]);
        let report = inputs.facts();
        assert_eq!(report.facts.as_ref().unwrap().b0_dir, Some((0.0, 0.1, 0.99)));
        assert_eq!(source(&report, FactName::B0Dir), Source::Sidecar);
        assert_eq!(report.problems.len(), 1);
        assert_eq!(report.problems[0].severity, Severity::Warning);
    }

    #[test]
    fn the_report_serialises_for_a_front_end() {
        let inputs: ScanInputs = serde_json::from_value(json!({
            "echoes": [{ "phase": { "dims": [4, 4, 4], "voxel_size": [1, 1, 1], "affine": AFFINE },
                         "sidecar": { "EchoTime": 0.004 } }],
            "overrides": { "field_strength": 3.0 },
        })).unwrap();
        let value = serde_json::to_value(inputs.facts()).unwrap();
        assert_eq!(value["facts"]["field_strength"], 3.0);
        assert_eq!(value["shown"][4]["name"], "field_strength");
        assert_eq!(value["shown"][4]["source"], "typed");
        assert_eq!(value["shown"][4]["label"], "Field strength");
        assert_eq!(value["sidecars"][0]["EchoTime"], 0.004);
        assert_eq!(value["problems"], json!([]));
    }
}
