//! Where a run's volumes live between its steps, and what decides whether a step's earlier output
//! is still valid.
//!
//! The run loop ([`crate::run::run`]) asks a [`StageStore`] for each step's inputs as the step
//! starts, hands it each output as it is produced, and tells it when a step is complete. `qsmxt`
//! implements it over its files: BIDS names, its step cache and state file, provenance sidecars.
//! [`MemoryStore`] keeps everything in memory, for qsmbly and for tests.

use std::collections::HashMap;

use crate::error::EngineError;
use qsm_core::pipeline::VolumeRef;

use crate::observe::VolumeData;
use crate::plan::Pass;
use crate::scan::{QsmSidecar, RunMetadata};
use crate::record::Decision;

/// A volume (or report) a step reads or writes, by what it is. A store keeps each under its own
/// name for it (`qsmxt`: a path).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum Item {
    /// Echo `n`'s phase (1-based), as acquired.
    AcquiredPhase(usize),
    /// Echo `n`'s magnitude, as acquired, when there is one.
    AcquiredMagnitude(usize),
    /// Echo `n`'s real part, as acquired, for a scan given as real and imaginary parts.
    AcquiredReal(usize),
    /// Echo `n`'s imaginary part, as acquired, for a scan given as real and imaginary parts.
    AcquiredImaginary(usize),
    /// Uncombined receive coil `c`'s (1-based, in [`StageStore::coils`] order) echo `n` phase, as
    /// acquired.
    CoilPhase(usize, usize),
    /// Coil `c`'s echo `n` magnitude, as acquired.
    CoilMagnitude(usize, usize),
    /// Echo `n`'s phase (radians) combined from the uncombined coils, published.
    CombinedEchoPhase(usize),
    /// Echo `n`'s magnitude combined from the uncombined coils, published.
    CombinedEchoMagnitude(usize),
    /// The mask the coil combination estimated each coil's phase offset in.
    CoilCombinationMask,
    /// Echo `n`'s phase in radians, on the working grid.
    Phase(usize),
    /// Echo `n`'s magnitude on the working grid.
    Magnitude(usize),
    /// Echo `n`'s phase after motion correction.
    CorrectedPhase(usize),
    /// Echo `n`'s magnitude after motion correction.
    CorrectedMagnitude(usize),
    /// The RSS-combined magnitude.
    CombinedMagnitude,
    /// The brain mask.
    BrainMask,
    /// A brain mask supplied with the scan (`--use-custom-masks`), as found.
    SuppliedMask,
    /// The brain mask restricted to the voxels every echo covers after motion correction.
    CoverageMask,
    /// The total field in ppm, zero outside the mask.
    FieldPpm,
    /// The total field in Hz, as it is published.
    TotalFieldHz,
    /// Echo 1's magnitude, published beside the total field.
    TotalFieldMagnitude,
    /// The robust echo fit's report (a JSON document).
    RobustFitReport,
    /// QSMART's reliability map `R_0` from its echo fit (1 = reliable), over the whole volume:
    /// where QSMART reconstructs, within the brain mask.
    FieldReliability,
    /// A pass's local field in ppm.
    LocalField(Pass),
    /// Where a pass's local field is defined.
    LocalFieldMask(Pass),
    /// A pass's raw (unreferenced) susceptibility map.
    RawChi(Pass),
    /// The reliable-phase mask of a two-pass run.
    ReliableMask,
    /// One of QSMART's intermediates, by its name in [`crate::stages::qsmart::outputs`].
    QsmartIntermediate(Pass, &'static str),
    /// The two passes' raw maps combined.
    CombinedRawChi,
    /// The final, referenced susceptibility map.
    Chi,
    /// Where the final map is defined.
    ChiSupport,
    /// A two-pass run's single-pass map, referenced.
    SinglePassChi,
    /// The run's segmentation (FreeSurfer label ids).
    Segmentation,
    /// A segmentation supplied with the scan (`--use-custom-dseg`), on whatever grid it came on.
    SuppliedSegmentation,
    /// The segmentation's lookup table: each label id and its name (text, a BIDS `dseg.tsv`).
    SegmentationLabels,
    /// SynthSeg's per-label volumes in mm³, in label-table order (a JSON array).
    SegmentationVolumes,
    /// Per-structure statistics of every map over the segmentation (text, a TSV table).
    RegionStats,
    /// The motion correction's per-echo report (a JSON document).
    MotionReport,
    /// The CLEAR-SWI image.
    Swi,
    /// The SWI's minimum-intensity projection, on its own grid ([`Data::OnGrid`]).
    SwiMip,
    /// A susceptibility map-weighted image, suppressing one sign of χ.
    Smwi(Contrast),
    /// An SMWI's minimum-intensity projection, on its own grid.
    SmwiMip(Contrast),
    /// The R2* map (Hz).
    R2star,
    /// The T2* map (s).
    T2star,
    /// Echo `n` (1-based) of a matched multi-echo spin-echo (MESE) acquisition, on its own grid
    /// ([`StageStore::mese`]).
    MeseMagnitude(usize),
    /// An R2 map supplied with the scan (`--use-custom-r2`).
    SuppliedR2,
    /// The R2 map (Hz).
    R2,
    /// An R2′ map supplied with the scan (`--use-custom-r2prime`).
    SuppliedR2prime,
    /// The R2′ map (Hz).
    R2prime,
    /// Where a measured R2′ is valid: the brain mask, narrowed to where R2 was measured.
    R2primeCoverage,
    /// A susceptibility map supplied with the scan (`--use-custom-qsm`), in place of the run's.
    SuppliedChi,
    /// One sign's χ-separation map, as a magnitude (both are positive).
    SeparatedChi(Contrast),
    /// The χ-separation net susceptibility, χ+ + χ−.
    SeparatedTotalChi,
}

impl Item {
    /// The name a front end keeps the item under: as Rust writes it (`AcquiredPhase(1)`,
    /// `LocalField(Main)`, `CoilPhase(2, 1)`, `QsmartIntermediate(Main, "vessel_mask")`).
    /// [`Item::from_key`] reads it back.
    pub fn key(&self) -> String {
        format!("{self:?}")
    }

    /// The item [`Item::key`] wrote as `key`; `None` for one this engine does not have.
    pub fn from_key(key: &str) -> Option<Item> {
        let (name, args) = match key.split_once('(') {
            Some((name, rest)) => (name, Some(rest.strip_suffix(')')?)),
            None => (key, None),
        };
        let pass = |a: &str| match a.trim() {
            "Main" => Some(Pass::Main),
            "Reliable" => Some(Pass::Reliable),
            _ => None,
        };
        let contrast = |a: Option<&str>| {
            let a = a?.trim();
            Contrast::BOTH.into_iter().find(|c| format!("{c:?}") == a)
        };
        let n = |a: &str| a.trim().parse::<usize>().ok();
        let echo = |a: Option<&str>| n(a?);
        let pair = |a: Option<&str>| {
            let (c, e) = a?.split_once(',')?;
            Some((n(c)?, n(e)?))
        };
        use qsm_core::pipeline::qsmart::outputs as q;
        Some(match (name, args) {
            ("AcquiredPhase", a) => Item::AcquiredPhase(echo(a)?),
            ("AcquiredMagnitude", a) => Item::AcquiredMagnitude(echo(a)?),
            ("AcquiredReal", a) => Item::AcquiredReal(echo(a)?),
            ("AcquiredImaginary", a) => Item::AcquiredImaginary(echo(a)?),
            ("CoilPhase", a) => { let (c, e) = pair(a)?; Item::CoilPhase(c, e) }
            ("CoilMagnitude", a) => { let (c, e) = pair(a)?; Item::CoilMagnitude(c, e) }
            ("CombinedEchoPhase", a) => Item::CombinedEchoPhase(echo(a)?),
            ("CombinedEchoMagnitude", a) => Item::CombinedEchoMagnitude(echo(a)?),
            ("Phase", a) => Item::Phase(echo(a)?),
            ("Magnitude", a) => Item::Magnitude(echo(a)?),
            ("CorrectedPhase", a) => Item::CorrectedPhase(echo(a)?),
            ("CorrectedMagnitude", a) => Item::CorrectedMagnitude(echo(a)?),
            ("MeseMagnitude", a) => Item::MeseMagnitude(echo(a)?),
            ("LocalField", Some(a)) => Item::LocalField(pass(a)?),
            ("LocalFieldMask", Some(a)) => Item::LocalFieldMask(pass(a)?),
            ("RawChi", Some(a)) => Item::RawChi(pass(a)?),
            ("QsmartIntermediate", Some(a)) => {
                let (p, n) = a.split_once(',')?;
                let n = n.trim().strip_prefix('"')?.strip_suffix('"')?;
                let name = [crate::stages::qsmart::VESSEL_MAGNITUDE, q::VESSEL_MASK, q::LOCAL_FIELD_STAGE1, q::CHI_STAGE1,
                            q::LOCAL_FIELD_STAGE2, q::CHI_STAGE2]
                    .into_iter().find(|&known| known == n)?;
                Item::QsmartIntermediate(pass(p)?, name)
            }
            ("Smwi", a) => Item::Smwi(contrast(a)?),
            ("SmwiMip", a) => Item::SmwiMip(contrast(a)?),
            ("SeparatedChi", a) => Item::SeparatedChi(contrast(a)?),
            (name, None) => [
                Item::CoilCombinationMask, Item::CombinedMagnitude, Item::BrainMask, Item::SuppliedMask,
                Item::CoverageMask, Item::FieldPpm, Item::TotalFieldHz, Item::TotalFieldMagnitude,
                Item::RobustFitReport, Item::FieldReliability, Item::ReliableMask, Item::CombinedRawChi, Item::Chi, Item::ChiSupport,
                Item::SinglePassChi, Item::Segmentation, Item::SuppliedSegmentation, Item::SegmentationLabels,
                Item::SegmentationVolumes, Item::RegionStats, Item::MotionReport, Item::Swi, Item::SwiMip,
                Item::R2star, Item::T2star, Item::SuppliedR2, Item::R2, Item::SuppliedR2prime, Item::R2prime,
                Item::R2primeCoverage, Item::SuppliedChi, Item::SeparatedTotalChi,
            ].into_iter().find(|item| item.key() == name)?,
            _ => return None,
        })
    }
}

/// Which sign of susceptibility an output is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum Contrast {
    /// Positive χ (iron, haemorrhage).
    Paramagnetic,
    /// Negative χ (calcification, myelin).
    Diamagnetic,
}

impl Contrast {
    pub const BOTH: [Contrast; 2] = [Contrast::Paramagnetic, Contrast::Diamagnetic];

    pub fn name(self) -> &'static str {
        match self {
            Contrast::Paramagnetic => "paramagnetic",
            Contrast::Diamagnetic => "diamagnetic",
        }
    }
}

/// A grid other than the run's: its dimensions, voxel size and voxel-to-world affine.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Geometry {
    pub dims: (usize, usize, usize),
    pub voxel_size: (f64, f64, f64),
    pub affine: [f64; 16],
}

/// An output, as a step hands it to the store.
#[derive(Debug, Clone, Copy)]
pub enum Data<'a> {
    Real(&'a [f64]),
    Mask(&'a [u8]),
    /// A volume on a grid of its own rather than the run's (a projection's shorter one).
    OnGrid(&'a [f64], &'a Geometry),
    /// A report.
    Json(&'a serde_json::Value),
    /// A table or other text.
    Text(&'a str),
    /// This input, unchanged: the store keeps it as it was given (`qsmxt` copies the file).
    Same(Item),
}

/// The first echo's grid as acquired, and the sidecar fields read with it.
#[derive(Debug, Clone)]
pub struct Header {
    pub dims: (usize, usize, usize),
    pub voxel_size: (f64, f64, f64),
    pub affine: [f64; 16],
    /// The first echo's sidecar, when it could be read.
    pub sidecar: Option<QsmSidecar>,
}

/// A volume supplied with the scan (a brain mask, a MESE echo), as the store found it.
#[derive(Debug, Clone)]
pub enum Supplied {
    /// `origin` names it in the log (`qsmxt`: its path); `dims` and `affine` are its own grid's.
    Found { origin: String, data: Vec<f64>, dims: (usize, usize, usize), affine: [f64; 16] },
    /// Found, but it could not be read.
    Unreadable { origin: String, error: String },
    /// None was found; `searched` says where it was looked for.
    Missing { searched: String },
}

/// Where a map supplied with the scan sits ([`StageStore::supplied_grid`]): what names it in the
/// log (`qsmxt`: its path), and its own grid.
#[derive(Debug, Clone)]
pub struct SuppliedGrid {
    pub origin: String,
    pub dims: (usize, usize, usize),
    pub affine: [f64; 16],
}

/// A multi-echo spin-echo (MESE) acquisition matched to the scan, for R2: its echoes are
/// [`Item::MeseMagnitude`].
#[derive(Debug, Clone)]
pub struct Mese {
    /// Names it in the log (`qsmxt`: its BIDS key).
    pub name: String,
    /// Its grid, from its first echo.
    pub dims: (usize, usize, usize),
    pub affine: [f64; 16],
    /// Echo times in seconds, in echo order.
    pub echo_times: Vec<f64>,
}

/// A step the run loop has computed, for the store to record.
#[derive(Debug, Clone)]
pub struct Completed<'a> {
    /// The step's name ([`crate::plan::Stage::name`]).
    pub step: &'a str,
    pub algorithm: Option<&'a str>,
    /// The step's parameters, as provenance records them.
    pub params: &'a serde_json::Value,
    /// What the step found in the data, recorded beside its parameters (not part of the key).
    pub outcome: Option<(&'static str, serde_json::Value)>,
    /// Its cache key ([`crate::cache::step_cache_key`]).
    pub key: &'a str,
    /// What it read, in the order provenance lists them.
    pub inputs: &'a [Item],
    /// What it wrote, in the order provenance lists them.
    pub outputs: &'a [Item],
    /// What it decided while it was computed ([`crate::record`]): kept with the step, for a later
    /// run that reuses it to report ([`StageStore::decisions`]). Not part of the key.
    pub decisions: &'a [Decision],
}

/// Inputs, outputs and the step cache of one run, for the run loop.
///
/// A step is cached under its name and its key, a string the engine derives from the step's
/// parameters and the configuration it reads; a store compares keys, it never builds one. A step
/// recomputed under a different key than it last had makes everything [downstream of it]
/// stale.
///
/// A step's record holds only while the outputs it names are still the ones it wrote. Steps of
/// different names can write the same output (each inversion algorithm's step writes the pass's
/// raw map), so a step is not cached while another step's record names one of its outputs
/// ([`sharing_outputs`]), and recording a step forgets every other step that named one of the
/// outputs it has just replaced, with everything downstream of it.
///
/// [downstream of it]: crate::graph::downstream_of
pub trait StageStore {
    /// The store's own errors: file I/O for `qsmxt`. The engine's become one of them.
    type Error: From<EngineError>;

    /// What identifies the scan's input data: anything that changes when an input is replaced
    /// (`qsmxt`: every input file's path, size and modification time). Part of the load step's
    /// key, so new input data recomputes the run.
    fn input_identity(&mut self) -> serde_json::Value;

    /// The first echo's header, every other input checked against it.
    fn header(&mut self) -> Result<Header, Self::Error>;

    /// The first echo's sidecar, when there is one.
    fn sidecar(&mut self) -> Result<Option<QsmSidecar>, Self::Error>;

    /// The uncombined receive coils' numbers, in order ([`Item::CoilPhase`]); none for combined
    /// data.
    fn coils(&mut self) -> Vec<u32> {
        Vec::new()
    }

    /// The grid a cached load step decided, when the load step is cached under `key`. A different
    /// key leaves nothing cached for the run.
    fn cached_scan(&mut self, key: &str) -> Option<RunMetadata>;

    /// Record the load step under `key`, with the grid it decided and what it decided about the
    /// scan's header ([`Self::decisions`] of `load`).
    fn record_scan(&mut self, key: &str, meta: &RunMetadata, decisions: &[Decision]) -> Result<(), Self::Error>;

    /// What `step` decided when it was last computed ([`Completed::decisions`]; for `load`, what
    /// [`Self::record_scan`] was given): what a run that reuses it reports.
    fn decisions(&mut self, step: &str) -> Vec<Decision>;

    /// The run's grid is known: everything after loading works on it.
    fn begin(&mut self, _meta: &RunMetadata) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Whether `item` is there to be read.
    fn has(&mut self, item: &Item) -> bool;

    fn real(&mut self, item: &Item) -> Result<Vec<f64>, Self::Error>;

    fn mask(&mut self, item: &Item) -> Result<Vec<u8>, Self::Error>;

    /// The brain mask supplied with the scan (asked for only when one is configured).
    fn supplied_mask(&mut self) -> Supplied;

    /// The MESE acquisition matched to the scan, if there is one.
    fn mese(&mut self) -> Option<Mese> {
        None
    }

    /// A volume supplied with the scan on a grid of its own (a MESE echo), as the store found it.
    fn supplied(&mut self, item: &Item) -> Supplied;

    /// The name and grid (dims, affine) of a map supplied with the scan ([`Item::SuppliedChi`],
    /// [`Item::SuppliedR2`], [`Item::SuppliedR2prime`], [`Item::SuppliedSegmentation`]), checked
    /// against the run's grid before its first step; `None` when none was found or it could not
    /// be read. A store that knows the grid without reading the voxels says so here.
    fn supplied_grid(&mut self, item: &Item) -> Option<SuppliedGrid> {
        match self.supplied(item) {
            Supplied::Found { origin, dims, affine, .. } => Some(SuppliedGrid { origin, dims, affine }),
            Supplied::Unreadable { .. } | Supplied::Missing { .. } => None,
        }
    }

    /// What names `item` in a cache key (`qsmxt`: its path, as earlier releases keyed it).
    fn identity(&mut self, item: &Item) -> serde_json::Value;

    /// The key `step` was last recorded under, if it is recorded.
    fn recorded_key(&mut self, step: &str) -> Option<String>;

    /// Whether `step`'s outputs are kept under `key`. A step recorded under another key is stale,
    /// and so is everything downstream of it.
    fn cached(&mut self, step: &str, key: &str) -> bool;

    /// `step` is about to be computed.
    fn start(&mut self, _step: &str) {}

    /// Have the weights of `models` ready (`qsmxt` downloads them; qsmbly provides them before
    /// running).
    fn models(&mut self, _models: &[&'static str]) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Keep an output of the step being computed.
    fn put(&mut self, item: Item, data: Data<'_>) -> Result<(), Self::Error>;

    /// Merge `fields` into what is recorded about `item` (`qsmxt`: its JSON sidecar). What was
    /// recorded about an output goes with it: a step that writes `item` anew ([`Self::put`]) starts
    /// from nothing recorded, the return to the acquired grid excepted.
    fn annotate(&mut self, item: &Item, fields: serde_json::Map<String, serde_json::Value>) -> Result<(), Self::Error>;

    /// A report or text output, as it was kept ([`Data::Json`], [`Data::Text`] as a string).
    fn report(&mut self, item: &Item) -> Option<serde_json::Value>;

    /// What is recorded about `item` ([`Self::annotate`]).
    fn annotation(&mut self, item: &Item) -> serde_json::Map<String, serde_json::Value>;

    /// `step` did not run this time, and what it left (`outputs`) no longer applies: drop them,
    /// and forget the step and everything downstream of it.
    fn discard(&mut self, step: &str, outputs: &[Item]) -> Result<(), Self::Error>;

    /// The run's outputs stay on the working grid it was resampled to (`--output-space working`):
    /// a store that names its outputs by the grid they are on records which grid that is.
    fn keep_working_grid(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    /// The step is complete: its outputs are in, and are valid under `completed.key`.
    fn record(&mut self, completed: &Completed<'_>) -> Result<(), Self::Error>;

    /// The run is complete.
    fn finish(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// The steps other than `step` whose records name one of `outputs`, of the `recorded` steps (each
/// one's name and recorded outputs, as a store names them: [`Item`]s, `qsmxt`'s paths). Whichever
/// wrote last holds the output, so only one of them can be cached ([`StageStore`]).
///
/// The return to the acquired grid (`output_space`) rewrites other steps' outputs in place, and a
/// store undoes it before any of them is computed again or read: it shares outputs with no step.
pub fn sharing_outputs<'a, T: PartialEq + 'a>(
    step: &str, outputs: &[T], recorded: impl IntoIterator<Item = (&'a str, &'a [T])>,
) -> Vec<String> {
    if step == OUTPUT_SPACE {
        return Vec::new();
    }
    recorded.into_iter()
        .filter(|&(other, theirs)| other != step && other != OUTPUT_SPACE && theirs.iter().any(|o| outputs.contains(o)))
        .map(|(other, _)| other.to_string())
        .collect()
}

/// The return to the acquired grid's step name.
pub const OUTPUT_SPACE: &str = "output_space";

/// Where a [`MemoryStore`] keeps its volumes: in its own memory ([`InMemory`], the default), or
/// wherever the front end chooses (qsmbly: in the page, outside the WASM heap, so the heap holds
/// only the step being computed).
///
/// The store decides what is cached; the volumes only hold data. Precision is the volumes': a
/// step's real-valued outputs reach [`Volumes::insert`] in double precision, and `qsmxt` keeps
/// them as float32 (its NIfTI files), so volumes that round them to float32 likewise give every
/// later step the values `qsmxt run` reads back. The default keeps them as they are.
pub trait Volumes {
    /// Whether `item` is held.
    fn contains(&self, item: &Item) -> bool;

    /// `item`'s data, if it is held.
    fn get(&self, item: &Item) -> Result<Option<VolumeData>, EngineError>;

    /// Hold `volume` as `item`, replacing what was there.
    fn insert(&mut self, item: Item, volume: VolumeRef<'_>) -> Result<(), EngineError>;

    /// Hold a copy of `from` as `to`; `false` if `from` is not held.
    fn copy(&mut self, from: &Item, to: Item) -> Result<bool, EngineError> {
        let Some(v) = self.get(from)? else { return Ok(false) };
        self.insert(to, v.as_ref())?;
        Ok(true)
    }

    /// Stop holding `item`; `false` if it was not held.
    fn remove(&mut self, item: &Item) -> bool;

    /// Move `item` out of its place and keep it aside: the working-grid version of an output that
    /// is about to be returned to the acquired grid. `false` if it is not held.
    fn set_aside(&mut self, item: &Item) -> bool;

    /// Put the version of `item` kept aside back in its place, replacing what is there; `false`
    /// if none is aside.
    fn put_back(&mut self, item: &Item) -> bool;

    /// Drop the version of `item` kept aside, if there is one.
    fn drop_aside(&mut self, item: &Item);

    /// The run's grid is known ([`StageStore::begin`]): every volume a step writes is on it.
    fn begin(&mut self, _meta: &RunMetadata) {}

    /// What the store has recorded changed (a step completed or went stale, a report or an
    /// annotation was kept): volumes that outlive the store keep it, to restore a store over them
    /// later ([`MemoryStore::restore`]) with every completed step still cached.
    fn recorded(&mut self, _recorded: &Recorded) {}
}

/// What a [`MemoryStore`] has recorded besides the volumes: which steps are complete under which
/// keys and what they wrote, the grid of the last load, the reports and annotations steps kept,
/// the grids of volumes not on the run's, and which outputs have their working-grid versions
/// kept aside. Items are named by [`Item::key`].
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Recorded {
    /// Each completed step: the key it was recorded under, and its outputs.
    pub steps: std::collections::BTreeMap<String, (String, Vec<String>)>,
    /// The load step's key and the grid it decided.
    pub scan: Option<(String, RunMetadata)>,
    pub reports: std::collections::BTreeMap<String, serde_json::Value>,
    pub annotations: std::collections::BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
    /// What each completed step (and the load step, as `load`) decided when it was computed.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub decisions: std::collections::BTreeMap<String, Vec<Decision>>,
    /// The grid of each volume not on the run's.
    #[serde(default)]
    pub geometries: std::collections::BTreeMap<String, Geometry>,
    /// The outputs returned to the acquired grid whose working-grid versions the volumes keep
    /// aside, with the grid each of those was on when not the run's.
    #[serde(default)]
    pub working_grid: std::collections::BTreeMap<String, Option<Geometry>>,
}

/// A [`MemoryStore`]'s own volumes, as computed (double precision).
#[derive(Debug, Default)]
pub struct InMemory {
    held: HashMap<Item, VolumeData>,
    aside: HashMap<Item, VolumeData>,
}

impl Volumes for InMemory {
    fn contains(&self, item: &Item) -> bool {
        self.held.contains_key(item)
    }

    fn get(&self, item: &Item) -> Result<Option<VolumeData>, EngineError> {
        Ok(self.held.get(item).cloned())
    }

    fn insert(&mut self, item: Item, volume: VolumeRef<'_>) -> Result<(), EngineError> {
        self.held.insert(item, VolumeData::from(volume));
        Ok(())
    }

    fn copy(&mut self, from: &Item, to: Item) -> Result<bool, EngineError> {
        let Some(v) = self.held.get(from).cloned() else { return Ok(false) };
        self.held.insert(to, v);
        Ok(true)
    }

    fn remove(&mut self, item: &Item) -> bool {
        self.held.remove(item).is_some()
    }

    fn set_aside(&mut self, item: &Item) -> bool {
        let Some(v) = self.held.remove(item) else { return false };
        self.aside.insert(*item, v);
        true
    }

    fn put_back(&mut self, item: &Item) -> bool {
        let Some(v) = self.aside.remove(item) else { return false };
        self.held.insert(*item, v);
        true
    }

    fn drop_aside(&mut self, item: &Item) {
        self.aside.remove(item);
    }
}

/// A [`StageStore`] that keeps everything in memory: the scan as given, every output, and the
/// step cache, with the same invalidation rules `qsmxt` follows. Running the same plan again
/// recomputes only the steps whose keys changed and what is downstream of them.
///
/// The volumes are kept in `V` ([`Volumes`]); everything else (their grids when not the run's,
/// reports, annotations, the step cache) in the store itself.
///
/// Outputs returned to the acquired grid replace their working-grid versions, as they do in
/// `qsmxt`'s files; the working-grid versions are kept aside ([`Volumes::set_aside`]) and put
/// back as soon as a later run has a step to compute that may read them.
#[derive(Debug, Default)]
pub struct MemoryStore<V = InMemory> {
    header: Option<Header>,
    identity: serde_json::Value,
    volumes: V,
    /// The grid of each volume that is not on the run's.
    geometries: HashMap<Item, Geometry>,
    reports: HashMap<Item, serde_json::Value>,
    annotations: HashMap<Item, serde_json::Map<String, serde_json::Value>>,
    supplied: Option<Supplied>,
    mese: Option<Mese>,
    /// The step being computed.
    current: Option<String>,
    /// The outputs returned to the acquired grid whose working-grid versions are kept aside, with
    /// the grid each of those was on when not the run's.
    working_grid: HashMap<Item, Option<Geometry>>,
    /// Each recorded step's key, and the outputs it recorded.
    steps: HashMap<String, (String, Vec<Item>)>,
    scan: Option<(String, RunMetadata)>,
    /// What each recorded step, and the load step, decided.
    decisions: HashMap<String, Vec<Decision>>,
}

impl MemoryStore {
    /// A store holding a scan: its first echo's header, and `identity` (anything that changes
    /// when the data does, e.g. a load counter). Add the echoes with [`Self::insert`].
    pub fn new(header: Header, identity: serde_json::Value) -> Self {
        Self::with_volumes(header, identity, InMemory::default())
    }

    /// Put an input (or replace any volume) in the store.
    pub fn insert(&mut self, item: Item, volume: VolumeData) {
        self.volumes.held.insert(item, volume);
    }

    /// Put an input on a grid of its own in the store.
    pub fn insert_on_grid(&mut self, item: Item, data: Vec<f64>, geometry: Geometry) {
        self.volumes.held.insert(item, VolumeData::Real(data));
        self.geometries.insert(item, geometry);
    }

    pub fn get(&self, item: &Item) -> Option<&VolumeData> {
        self.volumes.held.get(item)
    }

    /// Take a volume out of the store, to stay inside a memory budget. A step that wrote it is no
    /// longer cached.
    pub fn take(&mut self, item: &Item) -> Option<VolumeData> {
        self.volumes.held.remove(item)
    }
}

impl<V: Volumes> MemoryStore<V> {
    /// A store holding a scan, as [`MemoryStore::new`], whose volumes are kept in `volumes`. The
    /// echoes (`AcquiredPhase`, `AcquiredMagnitude`) are expected there.
    pub fn with_volumes(header: Header, identity: serde_json::Value, volumes: V) -> Self {
        Self {
            header: Some(header), identity, volumes, geometries: HashMap::new(), reports: HashMap::new(),
            annotations: HashMap::new(), supplied: None, mese: None, current: None,
            working_grid: HashMap::new(), steps: HashMap::new(), scan: None, decisions: HashMap::new(),
        }
    }

    /// Replace the scan's input data: the next run recomputes everything.
    pub fn set_identity(&mut self, identity: serde_json::Value) {
        self.identity = identity;
    }

    /// Supply a brain mask with the scan.
    pub fn supply_mask(&mut self, mask: Supplied) {
        self.supplied = Some(mask);
    }

    /// Match a MESE acquisition to the scan; add its echoes with [`MemoryStore::insert_on_grid`],
    /// or put them in the volumes and give their grid with [`Self::set_geometry`].
    pub fn supply_mese(&mut self, mese: Mese) {
        self.mese = Some(mese);
    }

    /// The input the volumes hold as `item` is on a grid of its own.
    pub fn set_geometry(&mut self, item: Item, geometry: Geometry) {
        self.geometries.insert(item, geometry);
    }

    /// Where the volumes are kept.
    pub fn volumes(&mut self) -> &mut V {
        &mut self.volumes
    }

    /// The grid the last run worked on, once its load step has run.
    pub fn grid(&self) -> Option<&RunMetadata> {
        self.scan.as_ref().map(|(_, meta)| meta)
    }

    /// The grid `item` is on, when it is not the run's.
    pub fn geometry(&self, item: &Item) -> Option<&Geometry> {
        self.geometries.get(item)
    }

    /// A report an output holds.
    pub fn report(&self, item: &Item) -> Option<&serde_json::Value> {
        self.reports.get(item)
    }

    /// What was recorded about `item` ([`StageStore::annotate`]).
    pub fn annotations(&self, item: &Item) -> Option<&serde_json::Map<String, serde_json::Value>> {
        self.annotations.get(item)
    }

    /// What the store has recorded ([`Recorded`]).
    pub fn recorded(&self) -> Recorded {
        let keys = |items: &[Item]| items.iter().map(Item::key).collect();
        Recorded {
            steps: self.steps.iter().map(|(s, (k, out))| (s.clone(), (k.clone(), keys(out)))).collect(),
            scan: self.scan.clone(),
            reports: self.reports.iter().map(|(i, v)| (i.key(), v.clone())).collect(),
            annotations: self.annotations.iter().map(|(i, a)| (i.key(), a.clone())).collect(),
            geometries: self.geometries.iter().map(|(i, g)| (i.key(), g.clone())).collect(),
            working_grid: self.working_grid.iter().map(|(i, g)| (i.key(), g.clone())).collect(),
            decisions: self.decisions.iter().filter(|(s, d)| !d.is_empty() && (s.as_str() == "load" || self.steps.contains_key(*s)))
                .map(|(s, d)| (s.clone(), d.clone())).collect(),
        }
    }

    /// Take up what an earlier store over the same volumes recorded, so its completed steps are
    /// cached here too (whatever the volumes no longer hold is not). A step naming an item this
    /// engine does not have is left out, and so computed again.
    pub fn restore(&mut self, recorded: Recorded) {
        fn items<T>(m: std::collections::BTreeMap<String, T>) -> impl Iterator<Item = (Item, T)> {
            m.into_iter().filter_map(|(k, v)| Some((Item::from_key(&k)?, v)))
        }
        self.steps = recorded.steps.into_iter().filter_map(|(step, (key, outputs))| {
            let outputs = outputs.iter().map(|k| Item::from_key(k)).collect::<Option<Vec<_>>>()?;
            Some((step, (key, outputs)))
        }).collect();
        self.scan = recorded.scan;
        self.decisions = recorded.decisions.into_iter().collect();
        self.reports = items(recorded.reports).collect();
        self.annotations = items(recorded.annotations).collect();
        self.geometries.extend(items(recorded.geometries));
        self.working_grid = items(recorded.working_grid).collect();
    }

    /// Tell the volumes what is recorded now.
    fn changed(&mut self) {
        let recorded = self.recorded();
        self.volumes.recorded(&recorded);
    }

    /// Put the working-grid versions back, for a step that may read them; the return to the
    /// acquired grid is then to do again. One the volumes no longer have aside is dropped rather
    /// than left on the acquired grid, so whatever wrote it is computed again.
    fn restore_working_grid(&mut self) {
        if self.working_grid.is_empty() {
            return;
        }
        for (item, geometry) in std::mem::take(&mut self.working_grid) {
            if !self.volumes.put_back(&item) {
                self.volumes.remove(&item);
            }
            match geometry {
                Some(g) => { self.geometries.insert(item, g); }
                None => { self.geometries.remove(&item); }
            }
        }
        self.steps.remove(OUTPUT_SPACE);
        self.changed();
    }

    fn forget(&mut self, step: &str) {
        self.steps.remove(step);
        for ds in crate::graph::downstream_of(step) {
            self.steps.remove(&ds);
        }
        self.changed();
    }

    /// The other recorded steps naming one of `outputs` ([`sharing_outputs`]).
    fn sharing(&self, step: &str, outputs: &[Item]) -> Vec<String> {
        sharing_outputs(step, outputs, self.steps.iter().map(|(s, (_, out))| (s.as_str(), out.as_slice())))
    }

    fn present(&self, item: &Item) -> bool {
        self.volumes.contains(item) || self.reports.contains_key(item)
    }

    fn volume(&self, item: &Item) -> Result<VolumeData, EngineError> {
        Volumes::get(&self.volumes, item)?
            .ok_or_else(|| EngineError::Config(format!("{item:?} is not in the store")))
    }
}

impl<V: Volumes> MemoryStore<V> {
    /// A store that starts out as this one and takes whatever is done to it without touching
    /// this one: what is recorded is copied, and the volumes are only looked at
    /// ([`Shadow`]). What a run would do is found by running it on one ([`crate::run::dry_run`]).
    pub fn shadow(&self) -> MemoryStore<Shadow<'_, V>> {
        MemoryStore {
            header: self.header.clone(),
            identity: self.identity.clone(),
            volumes: Shadow::new(&self.volumes),
            geometries: self.geometries.clone(),
            reports: self.reports.clone(),
            annotations: self.annotations.clone(),
            // Read only by a step being computed, which a dry run never does.
            supplied: None,
            mese: self.mese.clone(),
            current: None,
            working_grid: self.working_grid.clone(),
            steps: self.steps.clone(),
            scan: self.scan.clone(),
            decisions: self.decisions.clone(),
        }
    }

    /// The key each recorded step was recorded under, the load step's as `load`.
    pub fn recorded_keys(&self) -> HashMap<String, String> {
        let mut keys: HashMap<String, String> = self.steps.iter().map(|(s, (k, _))| (s.clone(), k.clone())).collect();
        if let Some((k, _)) = &self.scan {
            keys.insert("load".into(), k.clone());
        }
        keys
    }
}

/// Another store's volumes as a dry run sees them ([`MemoryStore::shadow`]): which items are
/// held, and nothing of their data. What is written, copied, moved or removed changes only which
/// items this says are held.
pub struct Shadow<'a, V> {
    base: &'a V,
    added: std::collections::HashSet<Item>,
    removed: std::collections::HashSet<Item>,
}

impl<'a, V: Volumes> Shadow<'a, V> {
    fn new(base: &'a V) -> Self {
        Self { base, added: Default::default(), removed: Default::default() }
    }
}

impl<V: Volumes> Volumes for Shadow<'_, V> {
    fn contains(&self, item: &Item) -> bool {
        self.added.contains(item) || (!self.removed.contains(item) && self.base.contains(item))
    }

    /// Nothing: a dry run reads no data.
    fn get(&self, _item: &Item) -> Result<Option<VolumeData>, EngineError> {
        Ok(None)
    }

    fn insert(&mut self, item: Item, _volume: VolumeRef<'_>) -> Result<(), EngineError> {
        self.removed.remove(&item);
        self.added.insert(item);
        Ok(())
    }

    fn copy(&mut self, from: &Item, to: Item) -> Result<bool, EngineError> {
        let held = self.contains(from);
        if held {
            self.insert(to, VolumeRef::Mask(&[]))?;
        }
        Ok(held)
    }

    fn remove(&mut self, item: &Item) -> bool {
        let held = self.contains(item);
        self.added.remove(item);
        self.removed.insert(*item);
        held
    }

    fn set_aside(&mut self, item: &Item) -> bool {
        self.remove(item)
    }

    /// Taken to succeed: the store keeps aside what it says it does.
    fn put_back(&mut self, item: &Item) -> bool {
        self.removed.remove(item);
        self.added.insert(*item);
        true
    }

    fn drop_aside(&mut self, _item: &Item) {}
}

impl<V: Volumes> StageStore for MemoryStore<V> {
    type Error = EngineError;

    fn input_identity(&mut self) -> serde_json::Value {
        self.identity.clone()
    }

    fn header(&mut self) -> Result<Header, EngineError> {
        self.header.clone().ok_or_else(|| EngineError::Config("no scan in the store".into()))
    }

    fn sidecar(&mut self) -> Result<Option<QsmSidecar>, EngineError> {
        Ok(self.header.as_ref().and_then(|h| h.sidecar.clone()))
    }

    /// The coils whose echoes were inserted ([`Item::CoilPhase`]), numbered by their position
    /// from 1: as many as have a first echo, in turn.
    fn coils(&mut self) -> Vec<u32> {
        (1..).take_while(|&c| self.volumes.contains(&Item::CoilPhase(c, 1))).map(|c| c as u32).collect()
    }

    fn cached_scan(&mut self, key: &str) -> Option<RunMetadata> {
        match &self.scan {
            Some((k, meta)) if k == key => Some(meta.clone()),
            Some(_) => {
                self.steps.clear();
                for item in std::mem::take(&mut self.working_grid).into_keys() {
                    self.volumes.drop_aside(&item);
                }
                self.scan = None;
                self.changed();
                None
            }
            None => None,
        }
    }

    fn record_scan(&mut self, key: &str, meta: &RunMetadata, decisions: &[Decision]) -> Result<(), EngineError> {
        self.scan = Some((key.to_string(), meta.clone()));
        self.decisions.insert("load".into(), decisions.to_vec());
        self.changed();
        Ok(())
    }

    fn begin(&mut self, meta: &RunMetadata) -> Result<(), EngineError> {
        self.volumes.begin(meta);
        Ok(())
    }

    fn has(&mut self, item: &Item) -> bool {
        self.present(item)
    }

    fn real(&mut self, item: &Item) -> Result<Vec<f64>, EngineError> {
        Ok(match self.volume(item)? {
            VolumeData::Real(v) => v,
            VolumeData::Mask(m) => m.iter().map(|&v| v as f64).collect(),
        })
    }

    fn mask(&mut self, item: &Item) -> Result<Vec<u8>, EngineError> {
        Ok(match self.volume(item)? {
            VolumeData::Mask(m) => m,
            VolumeData::Real(v) => v.iter().map(|&v| u8::from(v > 0.5)).collect(),
        })
    }

    fn supplied_mask(&mut self) -> Supplied {
        self.supplied.clone().unwrap_or(Supplied::Missing { searched: "the loaded files".into() })
    }

    fn mese(&mut self) -> Option<Mese> {
        self.mese.clone()
    }

    fn supplied(&mut self, item: &Item) -> Supplied {
        let origin = format!("{item:?}");
        let (Ok(Some(VolumeData::Real(data))), Some(g)) = (Volumes::get(&self.volumes, item), self.geometries.get(item)) else {
            return Supplied::Missing { searched: "the loaded files".into() };
        };
        Supplied::Found { origin, data, dims: g.dims, affine: g.affine }
    }

    /// The grid it was given ([`Self::insert_on_grid`], [`Self::set_geometry`]); `None` for one
    /// held on the run's grid.
    fn supplied_grid(&mut self, item: &Item) -> Option<SuppliedGrid> {
        let g = self.geometries.get(item)?;
        self.volumes.contains(item).then(|| SuppliedGrid { origin: format!("{item:?}"), dims: g.dims, affine: g.affine })
    }

    fn identity(&mut self, item: &Item) -> serde_json::Value {
        serde_json::json!(format!("{item:?}"))
    }

    fn recorded_key(&mut self, step: &str) -> Option<String> {
        self.steps.get(step).map(|(k, _)| k.clone())
    }

    fn decisions(&mut self, step: &str) -> Vec<Decision> {
        self.decisions.get(step).cloned().unwrap_or_default()
    }

    fn cached(&mut self, step: &str, key: &str) -> bool {
        let cached = match self.steps.get(step) {
            Some((recorded, outputs)) if recorded != key || !self.sharing(step, outputs).is_empty() => {
                self.forget(step);
                false
            }
            Some((_, outputs)) => outputs.iter().all(|o| self.present(o)),
            None => false,
        };
        if !cached && step != OUTPUT_SPACE {
            self.restore_working_grid();
        }
        cached
    }

    fn start(&mut self, step: &str) {
        self.current = Some(step.to_string());
    }

    fn put(&mut self, item: Item, data: Data<'_>) -> Result<(), EngineError> {
        if self.current.as_deref() == Some(OUTPUT_SPACE) {
            if !self.working_grid.contains_key(&item) && self.volumes.set_aside(&item) {
                self.working_grid.insert(item, self.geometries.get(&item).cloned());
            }
        } else if self.annotations.remove(&item).is_some() {
            // What was recorded about the output it replaces.
            self.changed();
        }
        self.geometries.remove(&item);
        match data {
            Data::Real(v) => self.volumes.insert(item, VolumeRef::Real(v))?,
            Data::Mask(m) => self.volumes.insert(item, VolumeRef::Mask(m))?,
            Data::OnGrid(v, geometry) => {
                self.volumes.insert(item, VolumeRef::Real(v))?;
                self.geometries.insert(item, geometry.clone());
            }
            Data::Json(j) => {
                self.reports.insert(item, j.clone());
                self.changed();
            }
            Data::Text(t) => {
                self.reports.insert(item, serde_json::Value::String(t.to_string()));
                self.changed();
            }
            Data::Same(source) => {
                if !self.volumes.copy(&source, item)? {
                    return Err(EngineError::Config(format!("{source:?} is not in the store")));
                }
            }
        }
        Ok(())
    }

    fn annotate(&mut self, item: &Item, fields: serde_json::Map<String, serde_json::Value>) -> Result<(), EngineError> {
        self.annotations.entry(*item).or_default().extend(fields);
        self.changed();
        Ok(())
    }

    fn report(&mut self, item: &Item) -> Option<serde_json::Value> {
        self.reports.get(item).cloned()
    }

    fn annotation(&mut self, item: &Item) -> serde_json::Map<String, serde_json::Value> {
        self.annotations.get(item).cloned().unwrap_or_default()
    }

    fn discard(&mut self, step: &str, outputs: &[Item]) -> Result<(), EngineError> {
        for item in outputs {
            self.volumes.remove(item);
            self.reports.remove(item);
        }
        if self.steps.contains_key(step) {
            self.forget(step);
        } else {
            self.changed();
        }
        Ok(())
    }

    fn record(&mut self, c: &Completed<'_>) -> Result<(), EngineError> {
        for other in self.sharing(c.step, c.outputs) {
            self.forget(&other);
        }
        if self.steps.get(c.step).map(|(k, _)| k.as_str()) != Some(c.key) {
            for ds in crate::graph::downstream_of(c.step) {
                self.steps.remove(&ds);
            }
        }
        self.steps.insert(c.step.to_string(), (c.key.to_string(), c.outputs.to_vec()));
        self.decisions.insert(c.step.to_string(), c.decisions.to_vec());
        self.changed();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{Choice, Topic};

    /// Volumes kept as `qsmxt` keeps them: real outputs rounded to float32.
    #[derive(Default)]
    struct Single {
        held: HashMap<Item, (Vec<f32>, bool)>,
        aside: HashMap<Item, (Vec<f32>, bool)>,
        grid: Option<(usize, usize, usize)>,
        kept: Option<Recorded>,
    }

    impl Volumes for Single {
        fn contains(&self, item: &Item) -> bool {
            self.held.contains_key(item)
        }

        fn get(&self, item: &Item) -> Result<Option<VolumeData>, EngineError> {
            Ok(self.held.get(item).map(|(v, mask)| if *mask {
                VolumeData::Mask(v.iter().map(|&x| x as u8).collect())
            } else {
                VolumeData::Real(v.iter().map(|&x| x as f64).collect())
            }))
        }

        fn insert(&mut self, item: Item, volume: VolumeRef<'_>) -> Result<(), EngineError> {
            let kept = match volume {
                VolumeRef::Real(v) => (v.iter().map(|&x| x as f32).collect(), false),
                VolumeRef::Mask(m) => (m.iter().map(|&x| x as f32).collect(), true),
            };
            self.held.insert(item, kept);
            Ok(())
        }

        fn remove(&mut self, item: &Item) -> bool {
            self.held.remove(item).is_some()
        }

        fn set_aside(&mut self, item: &Item) -> bool {
            let Some(v) = self.held.remove(item) else { return false };
            self.aside.insert(*item, v);
            true
        }

        fn put_back(&mut self, item: &Item) -> bool {
            let Some(v) = self.aside.remove(item) else { return false };
            self.held.insert(*item, v);
            true
        }

        fn drop_aside(&mut self, item: &Item) {
            self.aside.remove(item);
        }

        fn begin(&mut self, meta: &RunMetadata) {
            self.grid = Some(meta.dims);
        }

        fn recorded(&mut self, recorded: &Recorded) {
            self.kept = Some(recorded.clone());
        }
    }

    fn header() -> Header {
        Header { dims: (2, 1, 1), voxel_size: (1.0, 1.0, 1.0), affine: [0.0; 16], sidecar: None }
    }

    /// The store reads and writes through its volumes, at their precision, and tells them the grid.
    #[test]
    fn a_store_keeps_its_volumes_where_they_are_given() {
        let mut store = MemoryStore::with_volumes(header(), serde_json::json!(1), Single::default());
        let third = 1.0 / 3.0;
        store.put(Item::FieldPpm, Data::Real(&[third, 2.0])).unwrap();
        assert_eq!(store.real(&Item::FieldPpm).unwrap(), [third as f32 as f64, 2.0]);
        store.put(Item::BrainMask, Data::Mask(&[1, 0])).unwrap();
        assert_eq!(store.mask(&Item::BrainMask).unwrap(), [1, 0]);
        assert_eq!(store.real(&Item::BrainMask).unwrap(), [1.0, 0.0]);
        store.put(Item::Chi, Data::Same(Item::FieldPpm)).unwrap();
        assert_eq!(store.real(&Item::Chi).unwrap(), store.real(&Item::FieldPpm).unwrap());
        assert!(store.put(Item::RawChi(Pass::Main), Data::Same(Item::LocalField(Pass::Main))).is_err());
        assert!(store.has(&Item::Chi) && !store.has(&Item::LocalField(Pass::Main)));

        let meta = crate::stages::magnitude::tests::meta_4x4x4();
        store.begin(&meta).unwrap();
        assert_eq!(store.volumes().grid, Some(meta.dims));
        // Returning an output to the acquired grid keeps its working-grid version aside in the
        // volumes; a later step to compute puts it back.
        store.put(Item::Chi, Data::Real(&[1.0, 2.0])).unwrap();
        store.start("output_space");
        store.put(Item::Chi, Data::Real(&[3.0, 4.0])).unwrap();
        assert!(store.volumes().aside.contains_key(&Item::Chi));
        assert_eq!(store.real(&Item::Chi).unwrap(), [3.0, 4.0]);
        assert!(!store.cached("invert", "k"));
        assert_eq!(store.real(&Item::Chi).unwrap(), [1.0, 2.0]);
        assert!(store.volumes().aside.is_empty());
        assert!(store.volumes().remove(&Item::Chi) && !store.has(&Item::Chi));
    }

    /// Two steps that write the same output: recording one forgets the other and what read it
    /// (#373), and a store restored with both recorded caches neither. What was recorded about an
    /// output goes with it.
    #[test]
    fn a_step_that_writes_another_ones_output_takes_its_place() {
        let raw = Item::RawChi(Pass::Main);
        let done = |store: &mut MemoryStore, step: &str, key: &str, outputs: &[Item]| {
            store.start(step);
            for &item in outputs {
                store.put(item, Data::Real(&[1.0, 2.0])).unwrap();
            }
            store.record(&Completed {
                step, algorithm: None, params: &serde_json::json!({}), outcome: None, key, inputs: &[], outputs,
                decisions: &[],
            }).unwrap();
        };
        let mut store = MemoryStore::new(header(), serde_json::json!(1));
        done(&mut store, "invert", "a", &[raw]);
        done(&mut store, "reference", "r", &[Item::Chi]);
        let mut fields = serde_json::Map::new();
        fields.insert("QsmReferenceOffsetPpm".into(), 0.5.into());
        store.annotate(&Item::Chi, fields).unwrap();
        done(&mut store, "swi", "s", &[Item::Swi]);
        done(&mut store, "output_space", "o", &[Item::Chi, Item::Swi]);
        assert!(store.annotations(&Item::Chi).is_some(), "returned to the acquired grid, it is the same output");
        done(&mut store, "tgv", "b", &[raw]);
        let steps = store.recorded().steps;
        assert!(!steps.contains_key("invert") && !steps.contains_key("reference"), "{steps:?}");
        assert!(store.cached("tgv", "b"));
        done(&mut store, "reference", "r", &[Item::Chi]);
        assert!(store.annotations(&Item::Chi).is_none(), "a new map keeps nothing recorded about the old one");

        let mut both = store.recorded();
        both.steps.insert("invert".into(), ("a".into(), vec![raw.key()]));
        let volumes = std::mem::take(store.volumes());
        let mut again = MemoryStore::with_volumes(header(), serde_json::json!(1), volumes);
        again.restore(both);
        assert!(!again.cached("invert", "a"));
        assert!(!again.recorded().steps.contains_key("invert") && !again.recorded().steps.contains_key("reference"));
        assert!(again.cached("tgv", "b"), "only one of them is left to be recomputed");
    }

    /// Which differently named steps write the same output, over every inversion, single- and
    /// two-pass, with every output on: only the inversions' raw map. Not a rule the stores rely on
    /// ([`sharing_outputs`] holds for any), but the list of what they meet.
    #[test]
    fn only_the_inversion_steps_write_the_same_output() {
        use crate::enums::QsmAlgorithm;
        let mut facts = crate::plan::tests::facts();
        facts.mese_echo_times = Some(vec![0.01, 0.02, 0.03]);
        let mut writers: HashMap<Item, std::collections::BTreeSet<String>> = HashMap::new();
        for algorithm in QsmAlgorithm::ALL {
            for two_pass in [false, true] {
                let mut config = crate::config::PipelineConfig::default();
                config.inversion.algorithm = algorithm;
                config.masking.two_pass = two_pass;
                let p = &mut config.pipeline;
                (p.do_swi, p.do_smwi, p.do_t2starmap, p.do_r2starmap, p.do_r2map, p.do_r2primemap) = (true, true, true, true, true, true);
                (p.do_chi_separation, p.do_segmentation, p.do_analysis) = (true, true, true);
                for step in crate::plan::plan(&config, &facts).steps.iter().filter(|s| s.name != OUTPUT_SPACE) {
                    for &item in &step.outputs {
                        writers.entry(item).or_default().insert(step.name.clone());
                    }
                }
            }
        }
        writers.retain(|_, steps| steps.len() > 1);
        let names = |s: &[&str]| s.iter().map(|n| n.to_string()).collect();
        assert_eq!(writers, HashMap::from([
            (Item::RawChi(Pass::Main), names(&["invert", "qsmart", "tgv"])),
            (Item::RawChi(Pass::Reliable), names(&["invert-reliable", "qsmart-reliable", "tgv-reliable"])),
        ]));
    }

    #[test]
    fn items_are_read_back_from_their_keys() {
        use qsm_core::pipeline::qsmart::outputs as q;
        let mut items = vec![
            Item::AcquiredPhase(1), Item::AcquiredMagnitude(12), Item::AcquiredReal(3), Item::AcquiredImaginary(2), Item::CoilPhase(3, 1),
            Item::CoilMagnitude(1, 4), Item::CombinedEchoPhase(2), Item::CombinedEchoMagnitude(3),
            Item::CoilCombinationMask, Item::Phase(3), Item::Magnitude(2), Item::CorrectedPhase(4),
            Item::CorrectedMagnitude(5), Item::CombinedMagnitude, Item::BrainMask, Item::SuppliedMask,
            Item::CoverageMask, Item::FieldPpm, Item::TotalFieldHz, Item::TotalFieldMagnitude,
            Item::RobustFitReport, Item::LocalField(Pass::Main), Item::LocalFieldMask(Pass::Reliable),
            Item::RawChi(Pass::Reliable), Item::ReliableMask, Item::QsmartIntermediate(Pass::Main, q::VESSEL_MASK),
            Item::QsmartIntermediate(Pass::Reliable, q::CHI_STAGE2), Item::CombinedRawChi, Item::Chi,
            Item::ChiSupport, Item::SinglePassChi, Item::Segmentation, Item::SuppliedSegmentation,
            Item::SegmentationLabels, Item::SegmentationVolumes, Item::RegionStats, Item::MotionReport, Item::Swi,
            Item::SwiMip, Item::R2star, Item::T2star, Item::MeseMagnitude(6), Item::SuppliedR2, Item::R2,
            Item::SuppliedR2prime, Item::R2prime, Item::R2primeCoverage, Item::SuppliedChi, Item::SeparatedTotalChi,
        ];
        for c in Contrast::BOTH {
            items.extend([Item::Smwi(c), Item::SmwiMip(c), Item::SeparatedChi(c)]);
        }
        for item in items {
            assert_eq!(Item::from_key(&item.key()), Some(item), "{}", item.key());
        }
        for key in ["Chi(1)", "Phase", "Phase(x)", "CoilPhase(1)", "LocalField(Other)", "Smwi(Neither)",
                    "QsmartIntermediate(Main, \"x\")", "Nope"] {
            assert_eq!(Item::from_key(key), None, "{key}");
        }
    }

    /// What one store recorded reaches its volumes as it changes, and a new store over the same
    /// volumes takes it up with the completed steps still cached.
    #[test]
    fn a_new_store_restores_what_an_earlier_one_recorded() {
        let mut store = MemoryStore::with_volumes(header(), serde_json::json!(1), Single::default());
        let meta = crate::stages::magnitude::tests::meta_4x4x4();
        let gap = Decision::new(Topic::SliceGap, Choice::Ignored, "Reconstructed across a slice gap.");
        store.record_scan("load-key", &meta, std::slice::from_ref(&gap)).unwrap();
        store.put(Item::FieldPpm, Data::Real(&[1.0, 2.0])).unwrap();
        store.put(Item::RobustFitReport, Data::Json(&serde_json::json!({ "fit": 1 }))).unwrap();
        let mip = Geometry { dims: (2, 1, 1), voxel_size: (1.0, 1.0, 4.0), affine: [0.0; 16] };
        store.put(Item::SwiMip, Data::OnGrid(&[1.0, 2.0], &mip)).unwrap();
        let mut fields = serde_json::Map::new();
        fields.insert("Units".into(), "ppm".into());
        store.annotate(&Item::FieldPpm, fields).unwrap();
        let box_full = Decision::new(Topic::ReconstructionBox, Choice::Full, "Reconstructed on the whole grid.");
        store.record(&Completed {
            step: "unwrap", algorithm: None, params: &serde_json::json!({}), outcome: None, key: "k1",
            inputs: &[], outputs: &[Item::FieldPpm, Item::RobustFitReport], decisions: std::slice::from_ref(&box_full),
        }).unwrap();
        store.start("output_space");
        store.put(Item::FieldPpm, Data::Real(&[3.0, 4.0])).unwrap();
        store.record(&Completed {
            step: "output_space", algorithm: None, params: &serde_json::json!({}), outcome: None, key: "k2",
            inputs: &[], outputs: &[Item::FieldPpm], decisions: &[],
        }).unwrap();
        let kept = store.volumes().kept.clone().unwrap();
        assert_eq!(serde_json::to_value(&kept).unwrap(), serde_json::to_value(store.recorded()).unwrap());
        assert_eq!(kept.steps["unwrap"], ("k1".to_string(), vec!["FieldPpm".to_string(), "RobustFitReport".to_string()]));
        assert_eq!(kept.geometries["SwiMip"], mip);
        assert_eq!(kept.working_grid["FieldPpm"], None);

        let volumes = std::mem::take(store.volumes());
        let mut again = MemoryStore::with_volumes(header(), serde_json::json!(1), volumes);
        again.restore(serde_json::from_value(serde_json::to_value(&kept).unwrap()).unwrap());
        assert!(again.cached("output_space", "k2"));
        assert_eq!(again.cached_scan("load-key").map(|m| m.dims), Some(meta.dims));
        assert_eq!(again.report(&Item::RobustFitReport), Some(&serde_json::json!({ "fit": 1 })));
        assert!(again.annotations(&Item::FieldPpm).is_some_and(|a| a["Units"] == "ppm"));
        assert_eq!(again.geometry(&Item::SwiMip), Some(&mip));
        // What the steps decided comes back with them.
        assert_eq!(again.decisions("load"), [gap]);
        assert_eq!(again.decisions("unwrap"), [box_full]);
        assert!(again.decisions("output_space").is_empty());
        // A step to compute puts the working-grid version back, and the return to the acquired grid
        // is to do again.
        assert!(!again.cached("invert", "k3"));
        assert_eq!(again.real(&Item::FieldPpm).unwrap(), [1.0, 2.0]);
        assert!(again.cached("unwrap", "k1") && !again.cached("output_space", "k2"));
        // A key the step was not recorded under makes it stale, as ever.
        assert!(!again.cached("unwrap", "k2"));
        assert!(!again.volumes().kept.as_ref().unwrap().steps.contains_key("unwrap"));
    }
}
