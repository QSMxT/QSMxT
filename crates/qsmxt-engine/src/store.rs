//! Where a run's volumes live between its steps, and what decides whether a step's earlier output
//! is still valid.
//!
//! The run loop ([`crate::run::run`]) asks a [`StageStore`] for each step's inputs as the step
//! starts, hands it each output as it is produced, and tells it when a step is complete. `qsmxt`
//! implements it over its files: BIDS names, its step cache and state file, provenance sidecars.
//! [`MemoryStore`] keeps everything in memory, for qsmbly and for tests.

use std::collections::HashMap;

use crate::error::EngineError;
use crate::observe::{Observer, VolumeData};
use crate::plan::{Pass, Plan, Step};
use crate::scan::{QsmSidecar, RunMetadata};

/// A volume (or report) a step reads or writes, by what it is. A store keeps each under its own
/// name for it (`qsmxt`: a path).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum Item {
    /// Echo `n`'s phase (1-based), as acquired.
    AcquiredPhase(usize),
    /// Echo `n`'s magnitude, as acquired, when there is one.
    AcquiredMagnitude(usize),
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
    /// The segmentation: the run's own, or one supplied with the scan (`--use-custom-dseg`).
    Segmentation,
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

    /// The step is complete: its outputs are in, and are valid under `completed.key`.
    fn record(&mut self, completed: &Completed<'_>) -> Result<(), Self::Error>;

    /// Run a step the engine does not run itself ([`Step::host`]).
    fn host(&mut self, step: &Step, plan: &Plan, observer: &mut dyn Observer) -> Result<(), Self::Error>;

    /// The run is complete.
    fn finish(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// A [`StageStore`] that keeps everything in memory: the scan as given, every output, and the
/// step cache, with the same invalidation rules `qsmxt` follows. Running the same plan again
/// recomputes only the steps whose keys changed and what is downstream of them.
#[derive(Debug, Default)]
pub struct MemoryStore {
    header: Option<Header>,
    identity: serde_json::Value,
    volumes: HashMap<Item, VolumeData>,
    /// The grid of each volume that is not on the run's.
    geometries: HashMap<Item, Geometry>,
    reports: HashMap<Item, serde_json::Value>,
    annotations: HashMap<Item, serde_json::Map<String, serde_json::Value>>,
    supplied: Option<Supplied>,
    mese: Option<Mese>,
    /// Each recorded step's key, and the outputs it recorded.
    steps: HashMap<String, (String, Vec<Item>)>,
    scan: Option<(String, RunMetadata)>,
}

impl MemoryStore {
    /// A store holding a scan: its first echo's header, and `identity` (anything that changes
    /// when the data does, e.g. a load counter). Add the echoes with [`Self::insert`].
    pub fn new(header: Header, identity: serde_json::Value) -> Self {
        Self { header: Some(header), identity, ..Self::default() }
    }

    /// Replace the scan's input data: the next run recomputes everything.
    pub fn set_identity(&mut self, identity: serde_json::Value) {
        self.identity = identity;
    }

    /// Put an input (or replace any volume) in the store.
    pub fn insert(&mut self, item: Item, volume: VolumeData) {
        self.volumes.insert(item, volume);
    }

    /// Supply a brain mask with the scan.
    pub fn supply_mask(&mut self, mask: Supplied) {
        self.supplied = Some(mask);
    }

    /// Match a MESE acquisition to the scan; add its echoes with [`Self::insert_on_grid`].
    pub fn supply_mese(&mut self, mese: Mese) {
        self.mese = Some(mese);
    }

    /// Put an input on a grid of its own in the store.
    pub fn insert_on_grid(&mut self, item: Item, data: Vec<f64>, geometry: Geometry) {
        self.volumes.insert(item, VolumeData::Real(data));
        self.geometries.insert(item, geometry);
    }

    pub fn get(&self, item: &Item) -> Option<&VolumeData> {
        self.volumes.get(item)
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

    /// Take a volume out of the store, to stay inside a memory budget. A step that wrote it is no
    /// longer cached.
    pub fn take(&mut self, item: &Item) -> Option<VolumeData> {
        self.volumes.remove(item)
    }

    fn forget(&mut self, step: &str) {
        self.steps.remove(step);
        for ds in crate::graph::downstream_of(step) {
            self.steps.remove(&ds);
        }
    }

    fn present(&self, item: &Item) -> bool {
        self.volumes.contains_key(item) || self.reports.contains_key(item)
    }
}

impl StageStore for MemoryStore {
    type Error = EngineError;

    fn input_identity(&mut self) -> serde_json::Value {
        self.identity.clone()
    }

    fn header(&mut self) -> Result<Header, EngineError> {
        self.header.clone().ok_or_else(|| EngineError::Config("no scan in the store".into()))
    }

    fn cached_scan(&mut self, key: &str) -> Option<RunMetadata> {
        match &self.scan {
            Some((k, meta)) if k == key => Some(meta.clone()),
            Some(_) => {
                self.steps.clear();
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

    fn has(&mut self, item: &Item) -> bool {
        self.present(item)
    }

    fn real(&mut self, item: &Item) -> Result<Vec<f64>, EngineError> {
        match self.volumes.get(item) {
            Some(VolumeData::Real(v)) => Ok(v.clone()),
            Some(VolumeData::Mask(m)) => Ok(m.iter().map(|&v| v as f64).collect()),
            None => Err(EngineError::Config(format!("{item:?} is not in the store"))),
        }
    }

    fn mask(&mut self, item: &Item) -> Result<Vec<u8>, EngineError> {
        match self.volumes.get(item) {
            Some(VolumeData::Mask(m)) => Ok(m.clone()),
            Some(VolumeData::Real(v)) => Ok(v.iter().map(|&v| u8::from(v > 0.5)).collect()),
            None => Err(EngineError::Config(format!("{item:?} is not in the store"))),
        }
    }

    fn supplied_mask(&mut self) -> Supplied {
        self.supplied.clone().unwrap_or(Supplied::Missing { searched: "the loaded files".into() })
    }

    fn mese(&mut self) -> Option<Mese> {
        self.mese.clone()
    }

    fn supplied(&mut self, item: &Item) -> Supplied {
        let origin = format!("{item:?}");
        let (Some(VolumeData::Real(data)), Some(g)) = (self.volumes.get(item), self.geometries.get(item)) else {
            return Supplied::Missing { searched: "the loaded files".into() };
        };
        Supplied::Found { origin, data: data.clone(), dims: g.dims, affine: g.affine }
    }

    fn identity(&mut self, item: &Item) -> serde_json::Value {
        serde_json::json!(format!("{item:?}"))
    }

    fn recorded_key(&mut self, step: &str) -> Option<String> {
        self.steps.get(step).map(|(k, _)| k.clone())
    }

    fn cached(&mut self, step: &str, key: &str) -> bool {
        let Some((recorded, outputs)) = self.steps.get(step) else { return false };
        if recorded != key {
            self.forget(step);
            return false;
        }
        outputs.iter().all(|o| self.present(o))
    }

    fn put(&mut self, item: Item, data: Data<'_>) -> Result<(), EngineError> {
        self.geometries.remove(&item);
        match data {
            Data::Real(v) => { self.volumes.insert(item, VolumeData::Real(v.to_vec())); }
            Data::Mask(m) => { self.volumes.insert(item, VolumeData::Mask(m.to_vec())); }
            Data::OnGrid(v, geometry) => {
                self.volumes.insert(item, VolumeData::Real(v.to_vec()));
                self.geometries.insert(item, geometry.clone());
            }
            Data::Json(j) => { self.reports.insert(item, j.clone()); }
            Data::Same(source) => {
                let v = self.volumes.get(&source).cloned()
                    .ok_or_else(|| EngineError::Config(format!("{source:?} is not in the store")))?;
                self.volumes.insert(item, v);
            }
        }
        Ok(())
    }

    fn annotate(&mut self, item: &Item, fields: serde_json::Map<String, serde_json::Value>) -> Result<(), EngineError> {
        self.annotations.entry(*item).or_default().extend(fields);
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

    /// Only the steps that have nothing to do on this scan: motion correction when it is off, and
    /// returning the outputs to the acquired grid when they never left it. The rest are not in the
    /// engine yet.
    fn host(&mut self, step: &Step, plan: &Plan, _observer: &mut dyn Observer) -> Result<(), EngineError> {
        use crate::plan::Stage;
        let resampled = self.scan.as_ref().is_some_and(|(_, m)| m.source_geometry.is_some());
        match step.stage {
            Stage::Motion if !plan.motion_corrected => Ok(()),
            Stage::OutputSpace if !resampled => Ok(()),
            stage => Err(EngineError::Config(format!(
                "the {} step is not available in the engine yet", stage.name()))),
        }
    }
}
