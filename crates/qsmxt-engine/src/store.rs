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

/// A volume (or report) a step reads or writes, by what it is. A store keeps each under its own
/// name for it (`qsmxt`: a path).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum Item {
    /// Echo `n`'s phase (1-based), as acquired.
    AcquiredPhase(usize),
    /// Echo `n`'s magnitude, as acquired, when there is one.
    AcquiredMagnitude(usize),
    /// Echo `n`'s imaginary part, for a scan given as real and imaginary parts (whose real part
    /// is then [`Item::AcquiredPhase`]).
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
#[derive(Debug, Clone, PartialEq)]
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
}

/// Inputs, outputs and the step cache of one run, for the run loop.
///
/// A step is cached under its name and its key, a string the engine derives from the step's
/// parameters and the configuration it reads; a store compares keys, it never builds one. A step
/// recomputed under a different key than it last had makes everything [downstream of it]
/// stale.
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

    /// Record the load step under `key`, with the grid it decided.
    fn record_scan(&mut self, key: &str, meta: &RunMetadata) -> Result<(), Self::Error>;

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

    /// Merge `fields` into what is recorded about `item` (`qsmxt`: its JSON sidecar).
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
            working_grid: HashMap::new(), steps: HashMap::new(), scan: None,
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
        self.steps.remove("output_space");
    }

    fn forget(&mut self, step: &str) {
        self.steps.remove(step);
        for ds in crate::graph::downstream_of(step) {
            self.steps.remove(&ds);
        }
    }

    fn present(&self, item: &Item) -> bool {
        self.volumes.contains(item) || self.reports.contains_key(item)
    }

    fn volume(&self, item: &Item) -> Result<VolumeData, EngineError> {
        Volumes::get(&self.volumes, item)?
            .ok_or_else(|| EngineError::Config(format!("{item:?} is not in the store")))
    }
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
                None
            }
            None => None,
        }
    }

    fn record_scan(&mut self, key: &str, meta: &RunMetadata) -> Result<(), EngineError> {
        self.scan = Some((key.to_string(), meta.clone()));
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

    fn identity(&mut self, item: &Item) -> serde_json::Value {
        serde_json::json!(format!("{item:?}"))
    }

    fn recorded_key(&mut self, step: &str) -> Option<String> {
        self.steps.get(step).map(|(k, _)| k.clone())
    }

    fn cached(&mut self, step: &str, key: &str) -> bool {
        let cached = match self.steps.get(step) {
            Some((recorded, _)) if recorded != key => {
                self.forget(step);
                false
            }
            Some((_, outputs)) => outputs.iter().all(|o| self.present(o)),
            None => false,
        };
        if !cached && step != "output_space" {
            self.restore_working_grid();
        }
        cached
    }

    fn start(&mut self, step: &str) {
        self.current = Some(step.to_string());
    }

    fn put(&mut self, item: Item, data: Data<'_>) -> Result<(), EngineError> {
        if self.current.as_deref() == Some("output_space") && !self.working_grid.contains_key(&item)
            && self.volumes.set_aside(&item)
        {
            self.working_grid.insert(item, self.geometries.get(&item).cloned());
        }
        self.geometries.remove(&item);
        match data {
            Data::Real(v) => self.volumes.insert(item, VolumeRef::Real(v))?,
            Data::Mask(m) => self.volumes.insert(item, VolumeRef::Mask(m))?,
            Data::OnGrid(v, geometry) => {
                self.volumes.insert(item, VolumeRef::Real(v))?;
                self.geometries.insert(item, geometry.clone());
            }
            Data::Json(j) => { self.reports.insert(item, j.clone()); }
            Data::Text(t) => { self.reports.insert(item, serde_json::Value::String(t.to_string())); }
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
        }
        Ok(())
    }

    fn record(&mut self, c: &Completed<'_>) -> Result<(), EngineError> {
        if self.steps.get(c.step).map(|(k, _)| k.as_str()) != Some(c.key) {
            for ds in crate::graph::downstream_of(c.step) {
                self.steps.remove(&ds);
            }
        }
        self.steps.insert(c.step.to_string(), (c.key.to_string(), c.outputs.to_vec()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Volumes kept as `qsmxt` keeps them: real outputs rounded to float32.
    #[derive(Default)]
    struct Single {
        held: HashMap<Item, (Vec<f32>, bool)>,
        aside: HashMap<Item, (Vec<f32>, bool)>,
        grid: Option<(usize, usize, usize)>,
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
}
