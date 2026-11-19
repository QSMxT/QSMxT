//! A scan's facts from the files a front end holds: each echo's NIfTI headers and BIDS sidecar,
//! a multi-echo spin-echo (MESE) acquisition's if there is one, and the values the user typed in
//! for what the files do not say.
//!
//! [`ScanInputs::facts`] decides the [`ScanFacts`] a run plans with, says where each fact came
//! from, and finds what is missing or inconsistent before anything is computed, attached to the
//! fact and the file concerned. It also says which store item each file is kept under
//! ([`ScanReport::files`]). A front end that loads files itself (qsmbly) builds its facts, the
//! store's header ([`ScanInputs::header`]) and its MESE ([`ScanInputs::mese`]) here, so it decides
//! nothing about them on its own.
//!
//! An echo is given in one of three ways ([`Encoding`]), the same for every echo of a scan:
//!
//! | Given as | Each echo's files | Kept under |
//! |---|---|---|
//! | phase (and magnitude) | `phase`, `magnitude` | `AcquiredPhase(e)`, `AcquiredMagnitude(e)` |
//! | real and imaginary parts | `real`, `imaginary` | `AcquiredReal(e)`, `AcquiredImaginary(e)` |
//! | uncombined coils | `coils[c]`: `phase`, `magnitude` | `CoilPhase(c, e)`, `CoilMagnitude(c, e)` |
//!
//! A MESE's echo `n` is kept under `MeseMagnitude(n)`, on its own grid. One coil is combined data:
//! its images are kept as `AcquiredPhase(e)` and `AcquiredMagnitude(e)`.

use serde::{Deserialize, Serialize};

use crate::requirements::Severity;
use crate::scan::{QsmSidecar, ScanFacts};
use crate::store::{Geometry, Header, Item, MemoryStore, Mese, Volumes};

mod facts;

/// One uncombined receive coil's images of one echo.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CoilInput {
    #[serde(default)]
    pub phase: Option<Geometry>,
    #[serde(default)]
    pub magnitude: Option<Geometry>,
}

/// One echo of a scan, as the front end holds it: its phase and magnitude images, its real and
/// imaginary parts, or each uncombined coil's phase and magnitude images ([`Encoding`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EchoInput {
    /// The phase image's header.
    #[serde(default)]
    pub phase: Option<Geometry>,
    /// The magnitude image's header, if there is one.
    #[serde(default)]
    pub magnitude: Option<Geometry>,
    /// The real part's header, for an echo given as real and imaginary parts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub real: Option<Geometry>,
    /// The imaginary part's header.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imaginary: Option<Geometry>,
    /// Each uncombined receive coil's images, in coil order, for an echo given per coil.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub coils: Vec<CoilInput>,
    /// The echo's BIDS sidecar, as parsed JSON.
    #[serde(default)]
    pub sidecar: Option<serde_json::Value>,
}

/// One echo of a MESE acquisition.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MeseEchoInput {
    /// The image's header.
    #[serde(default)]
    pub magnitude: Option<Geometry>,
    /// Its BIDS sidecar, as parsed JSON: its `EchoTime`.
    #[serde(default)]
    pub sidecar: Option<serde_json::Value>,
}

/// A multi-echo spin-echo acquisition of the same subject, from which R2 is measured.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MeseInput {
    /// What names it in the run's log; "MESE" when not given.
    #[serde(default)]
    pub name: Option<String>,
    /// Its echoes, in echo order.
    #[serde(default)]
    pub echoes: Vec<MeseEchoInput>,
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
    /// The scanner's manufacturer, as a sidecar's `Manufacturer` gives it.
    #[serde(default)]
    pub manufacturer: Option<String>,
    /// Every MESE echo's echo time, in seconds, in echo order.
    #[serde(default)]
    pub mese_echo_times: Option<Vec<f64>>,
}

/// What the front end holds of one scan: its echoes in order, a MESE if there is one, and what
/// the user typed in.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScanInputs {
    pub echoes: Vec<EchoInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mese: Option<MeseInput>,
    #[serde(default)]
    pub overrides: FactOverrides,
}

/// How a scan's echoes are given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    /// A phase image per echo, with or without a magnitude image.
    MagnitudePhase,
    /// A real and an imaginary part per echo; the magnitude and phase are computed from them.
    RealImaginary,
    /// Each uncombined receive coil's phase and magnitude per echo; they are combined into one.
    Coils,
}

impl Encoding {
    fn describe(self) -> &'static str {
        match self {
            Encoding::MagnitudePhase => "phase and magnitude images",
            Encoding::RealImaginary => "real and imaginary parts",
            Encoding::Coils => "images of each uncombined coil",
        }
    }
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
    /// Whether the echoes are given as real and imaginary parts.
    RealImaginary,
    /// How many uncombined receive coils (1 for combined data).
    Coils,
    /// How many echoes the MESE has (0 without one).
    MeseEchoes,
    MeseEchoTimes,
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
            FactName::RealImaginary => "Real and imaginary parts",
            FactName::Coils => "Uncombined coils",
            FactName::MeseEchoes => "Spin echoes (MESE)",
            FactName::MeseEchoTimes => "Spin-echo times",
        }
    }

    pub fn units(self) -> Option<&'static str> {
        match self {
            FactName::VoxelSize => Some("mm"),
            FactName::EchoTimes | FactName::MeseEchoTimes => Some("s"),
            FactName::FieldStrength => Some("T"),
            _ => None,
        }
    }

    /// Whether the user can type a value in for it ([`FactOverrides`]).
    pub fn typed(self) -> bool {
        matches!(self, FactName::EchoTimes | FactName::FieldStrength | FactName::B0Dir
            | FactName::Manufacturer | FactName::MeseEchoTimes)
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

/// Which file a problem concerns: one of an echo's (or of a coil's, given
/// [`InputProblem::coil`]), or one of the MESE's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    Phase,
    Magnitude,
    Real,
    Imaginary,
    Sidecar,
    /// A MESE echo's image ([`InputProblem::echo`] is the MESE's echo).
    MeseMagnitude,
    /// A MESE echo's sidecar.
    MeseSidecar,
}

impl Part {
    fn is_mese(self) -> bool {
        matches!(self, Part::MeseMagnitude | Part::MeseSidecar)
    }
}

/// Something missing or inconsistent in the inputs. An error means the scan cannot be run as it
/// is; a warning that it runs, with the consequence the message says.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InputProblem {
    pub severity: Severity,
    /// The fact it concerns, if one.
    pub fact: Option<FactName>,
    /// The echo it concerns (from 1), if one: the MESE's echo for a MESE part.
    pub echo: Option<usize>,
    /// The coil it concerns (from 1), if one.
    pub coil: Option<usize>,
    /// Which of that echo's files.
    pub part: Option<Part>,
    pub message: String,
}

impl InputProblem {
    fn new(severity: Severity, message: String) -> Self {
        InputProblem { severity, fact: None, echo: None, coil: None, part: None, message }
    }

    fn error(message: impl Into<String>) -> Self {
        Self::new(Severity::Error, message.into())
    }

    fn warning(message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, message.into())
    }

    fn fact(self, fact: FactName) -> Self {
        InputProblem { fact: Some(fact), ..self }
    }

    fn echo(self, echo: usize) -> Self {
        InputProblem { echo: Some(echo), ..self }
    }

    fn part(self, part: Part) -> Self {
        InputProblem { part: Some(part), ..self }
    }

    fn at(self, slot: &Slot) -> Self {
        InputProblem { echo: Some(slot.echo), coil: slot.coil, part: Some(slot.part), ..self }
    }
}

/// One file of the inputs and the store item it is kept under.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InputFile {
    /// The item's key ([`Item::key`]), e.g. `AcquiredReal(2)`, `CoilPhase(3, 1)`.
    pub key: String,
    /// Its echo (from 1): the MESE's echo for a MESE part.
    pub echo: usize,
    /// Its coil (from 1), for uncombined coil data.
    pub coil: Option<usize>,
    pub part: Part,
    /// On a grid of its own (a MESE echo): the store is given its geometry with it
    /// ([`MemoryStore::insert_on_grid`], or [`MemoryStore::set_geometry`]).
    pub own_grid: bool,
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
    /// Every image given, with the store item it is kept under.
    pub files: Vec<InputFile>,
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

/// A place for one image in the inputs.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Slot {
    item: Item,
    echo: usize,
    coil: Option<usize>,
    part: Part,
    /// Must be given.
    required: bool,
}

impl Slot {
    /// How a message names its image ("echo 2's real part").
    fn name(&self) -> String {
        let what = match self.part {
            Part::Phase => "phase image",
            Part::Magnitude => "magnitude image",
            Part::Real => "real part",
            Part::Imaginary => "imaginary part",
            Part::MeseMagnitude => "image",
            Part::Sidecar | Part::MeseSidecar => "sidecar",
        };
        let echo = if self.part.is_mese() { format!("MESE echo {}", self.echo) } else { format!("echo {}", self.echo) };
        match self.coil {
            Some(c) => format!("{echo}'s coil {c} {what}"),
            None => format!("{echo}'s {what}"),
        }
    }
}

fn capitalised(s: String) -> String {
    let mut chars = s.chars();
    chars.next().map_or(s.clone(), |c| c.to_uppercase().chain(chars).collect())
}

fn size(g: &Geometry) -> String {
    format!("{}×{}×{}", g.dims.0, g.dims.1, g.dims.2)
}

impl EchoInput {
    /// Every way the echo's images give it.
    fn encodings(&self) -> Vec<Encoding> {
        let parts = self.real.is_some() || self.imaginary.is_some();
        let coils = !self.coils.is_empty();
        let mut out = Vec::new();
        if self.phase.is_some() || (self.magnitude.is_some() && !parts && !coils) {
            out.push(Encoding::MagnitudePhase);
        }
        if parts {
            out.push(Encoding::RealImaginary);
        }
        if coils {
            out.push(Encoding::Coils);
        }
        out
    }
}

impl ScanInputs {
    /// How the scan's echoes are given: as its first echo with any image gives it (per coil over
    /// real and imaginary parts over phase, when that echo mixes them).
    pub fn encoding(&self) -> Encoding {
        self.echoes.iter().map(EchoInput::encodings).find(|e| !e.is_empty())
            .and_then(|e| e.into_iter().next_back())
            .unwrap_or(Encoding::MagnitudePhase)
    }

    /// Uncombined coils: as many as the first echo has; 1 for combined data.
    fn n_coils(&self) -> usize {
        match self.encoding() {
            Encoding::Coils => self.echoes.first().map_or(1, |e| e.coils.len().max(1)),
            _ => 1,
        }
    }

    /// Every place for an image the scan's encoding has, echo by echo, then the MESE's.
    fn slots(&self) -> Vec<Slot> {
        let encoding = self.encoding();
        let combined = self.n_coils() == 1;
        let mut slots = Vec::new();
        for (i, echo) in self.echoes.iter().enumerate() {
            let e = i + 1;
            let slot = |item, coil, part, required| Slot { item, echo: e, coil, part, required };
            match encoding {
                Encoding::MagnitudePhase => slots.extend([
                    slot(Item::AcquiredPhase(e), None, Part::Phase, true),
                    slot(Item::AcquiredMagnitude(e), None, Part::Magnitude, false),
                ]),
                Encoding::RealImaginary => slots.extend([
                    slot(Item::AcquiredReal(e), None, Part::Real, true),
                    slot(Item::AcquiredImaginary(e), None, Part::Imaginary, true),
                ]),
                Encoding::Coils => for c in 1..=echo.coils.len() {
                    let (phase, magnitude) = if combined {
                        (Item::AcquiredPhase(e), Item::AcquiredMagnitude(e))
                    } else {
                        (Item::CoilPhase(c, e), Item::CoilMagnitude(c, e))
                    };
                    slots.extend([slot(phase, Some(c), Part::Phase, true), slot(magnitude, Some(c), Part::Magnitude, true)]);
                },
            }
        }
        for (i, _) in self.mese.iter().flat_map(|m| &m.echoes).enumerate() {
            slots.push(Slot { item: Item::MeseMagnitude(i + 1), echo: i + 1, coil: None, part: Part::MeseMagnitude, required: true });
        }
        slots
    }

    fn geometry(&self, s: &Slot) -> Option<&Geometry> {
        if s.part == Part::MeseMagnitude {
            return self.mese.as_ref()?.echoes.get(s.echo - 1)?.magnitude.as_ref();
        }
        let echo = self.echoes.get(s.echo - 1)?;
        match (s.coil, s.part) {
            (Some(c), Part::Phase) => echo.coils.get(c - 1)?.phase.as_ref(),
            (Some(c), _) => echo.coils.get(c - 1)?.magnitude.as_ref(),
            (None, Part::Phase) => echo.phase.as_ref(),
            (None, Part::Magnitude) => echo.magnitude.as_ref(),
            (None, Part::Real) => echo.real.as_ref(),
            (None, Part::Imaginary) => echo.imaginary.as_ref(),
            _ => None,
        }
    }

    fn geometry_mut(&mut self, s: &Slot) -> Option<&mut Option<Geometry>> {
        if s.part == Part::MeseMagnitude {
            return Some(&mut self.mese.as_mut()?.echoes.get_mut(s.echo - 1)?.magnitude);
        }
        let echo = self.echoes.get_mut(s.echo - 1)?;
        Some(match (s.coil, s.part) {
            (Some(c), Part::Phase) => &mut echo.coils.get_mut(c - 1)?.phase,
            (Some(c), _) => &mut echo.coils.get_mut(c - 1)?.magnitude,
            (None, Part::Phase) => &mut echo.phase,
            (None, Part::Magnitude) => &mut echo.magnitude,
            (None, Part::Real) => &mut echo.real,
            (None, Part::Imaginary) => &mut echo.imaginary,
            _ => return None,
        })
    }

    /// Read every image's header again from where the front end keeps it, by the store item it
    /// is kept under ([`ScanReport::files`]): the headers the run will read, in place of what the
    /// inputs said.
    pub fn read_headers<E>(&mut self, mut read: impl FnMut(&Item) -> Result<Option<Geometry>, E>) -> Result<(), E> {
        for slot in self.slots() {
            let geometry = read(&slot.item)?;
            if let Some(place) = self.geometry_mut(&slot) {
                *place = geometry;
            }
        }
        Ok(())
    }

    /// The header a [`MemoryStore`] holding this scan starts from: the first echo's first image's
    /// grid (its phase image, real part, or first coil's phase image) and the first echo's
    /// sidecar as a run reads it. `None` while the report has no facts.
    pub fn header(&self, report: &ScanReport) -> Option<Header> {
        report.facts.as_ref()?;
        let first = self.slots().into_iter().find(|s| s.echo == 1 && s.required && !s.part.is_mese())?;
        let grid = self.geometry(&first)?;
        Some(Header {
            dims: grid.dims, voxel_size: grid.voxel_size, affine: grid.affine,
            orientation: grid.orientation,
            sidecar: report.sidecars.first().cloned(),
        })
    }

    /// The MESE a [`MemoryStore`] holding this scan is given ([`MemoryStore::supply_mese`]): its
    /// grid from its first echo, and its echo times as the report decided them. `None` without a
    /// usable MESE.
    pub fn mese(&self, report: &ScanReport) -> Option<Mese> {
        let echo_times = report.facts.as_ref()?.mese_echo_times.clone()?;
        let mese = self.mese.as_ref()?;
        let first = mese.echoes.first()?.magnitude.as_ref()?;
        Some(Mese {
            name: mese.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "MESE".into()),
            dims: first.dims, affine: first.affine, echo_times,
        })
    }

    /// Give `store` what it needs of the inputs beyond its header and volumes: the MESE
    /// ([`Self::mese`]) and each MESE echo's grid. The echoes' data go in the volumes under
    /// [`ScanReport::files`]' keys.
    pub fn supply<V: Volumes>(&self, report: &ScanReport, store: &mut MemoryStore<V>) {
        let Some(mese) = self.mese(report) else { return };
        for slot in self.slots().iter().filter(|s| s.part == Part::MeseMagnitude) {
            if let Some(geometry) = self.geometry(slot) {
                store.set_geometry(slot.item, geometry.clone());
            }
        }
        store.supply_mese(mese);
    }
}

#[cfg(test)]
mod tests;
