//! Where a run's volumes live between its steps, and what decides whether a step's earlier output
//! is still valid.
//!
//! The run loop ([`crate::run::run`]) asks a [`StageStore`] for each step's inputs as the step
//! starts, hands it each output as it is produced, and tells it when a step is complete. `qsmxt`
//! implements it over its files: BIDS names, its step cache and state file, provenance sidecars.
//! [`MemoryStore`] keeps everything in memory, for qsmbly and for tests.

use crate::error::EngineError;
use qsm_core::pipeline::VolumeRef;

use crate::observe::VolumeData;
use crate::scan::{QsmSidecar, RunMetadata};
use crate::record::Decision;

mod item;
mod memory_store;
pub use item::{Contrast, Item};
pub use memory_store::{InMemory, MemoryStore, Shadow};

/// A grid other than the run's: its dimensions, voxel size and voxel-to-world affine.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Geometry {
    pub dims: (usize, usize, usize),
    pub voxel_size: (f64, f64, f64),
    pub affine: [f64; 16],
    /// The orientation fields of the header the grid was read from, when it was read from a
    /// file's header ([`crate::scan::HeaderOrientation::read`]); `None` for a grid the run made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orientation: Option<crate::scan::HeaderOrientation>,
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
    /// The first echo's header orientation fields as the file stores them, when the front end
    /// read them: the image geometry N4 reads ([`crate::scan::itk_geometry`]).
    pub orientation: Option<crate::scan::HeaderOrientation>,
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
    /// What it read, in the order provenance lists them. A store keeps it with the step: it is
    /// what makes the step stale ([`crate::graph::readers`]).
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
/// recomputed under a different key than it last had makes every recorded step that read what it
/// wrote ([its readers]) stale: what each step read is recorded with it ([`Completed::inputs`]).
///
/// A step's record holds only while the outputs it names are still the ones it wrote. Steps of
/// different names can write the same output (each inversion algorithm's step writes the pass's
/// raw map), so a step is not cached while another step's record names one of its outputs
/// ([`sharing_outputs`]), and recording a step forgets every other step that named one of the
/// outputs it has just replaced, with every step that read what it wrote.
///
/// [its readers]: crate::graph::readers
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

    /// What names `item` in a cache key (`qsmxt`: its path, as earlier releases keyed it). For a
    /// file supplied with the scan it is in the keys of the steps that read it, so it must change
    /// when the file does; it names the file in the log too, so make it a string.
    fn identity(&mut self, item: &Item) -> serde_json::Value;

    /// The key `step` was last recorded under, if it is recorded.
    fn recorded_key(&mut self, step: &str) -> Option<String>;

    /// Whether `step`'s outputs are kept under `key`. A step recorded under another key is stale,
    /// and so is every recorded step that read what it wrote.
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
    /// and forget the step and every recorded step that read what it wrote.
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
/// them as float32 (its NIfTI files) except those [`Item::kept_in_double`], so volumes that round
/// the others to float32 likewise give every later step the values `qsmxt run` reads back. The
/// default keeps them all as they are.
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
    /// What each completed step read ([`Completed::inputs`]), which decides what makes it stale
    /// ([`crate::graph::readers`]). Records from before it was kept have none.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub reads: std::collections::BTreeMap<String, Vec<String>>,
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::plan::Pass;
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
        Header { dims: (2, 1, 1), voxel_size: (1.0, 1.0, 1.0), affine: [0.0; 16], orientation: None, sidecar: None }
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
        let done = |store: &mut MemoryStore, step: &str, key: &str, inputs: &[Item], outputs: &[Item]| {
            store.start(step);
            for &item in outputs {
                store.put(item, Data::Real(&[1.0, 2.0])).unwrap();
            }
            store.record(&Completed {
                step, algorithm: None, params: &serde_json::json!({}), outcome: None, key, inputs, outputs,
                decisions: &[],
            }).unwrap();
        };
        let mut store = MemoryStore::new(header(), serde_json::json!(1));
        done(&mut store, "invert", "a", &[], &[raw]);
        done(&mut store, "reference", "r", &[raw], &[Item::Chi]);
        let mut fields = serde_json::Map::new();
        fields.insert("QsmReferenceOffsetPpm".into(), 0.5.into());
        store.annotate(&Item::Chi, fields).unwrap();
        done(&mut store, "swi", "s", &[], &[Item::Swi]);
        done(&mut store, "output_space", "o", &[Item::Chi, Item::Swi], &[Item::Chi, Item::Swi]);
        assert!(store.annotations(&Item::Chi).is_some(), "returned to the acquired grid, it is the same output");
        done(&mut store, "tgv", "b", &[], &[raw]);
        let steps = store.recorded().steps;
        assert!(!steps.contains_key("invert") && !steps.contains_key("reference"), "{steps:?}");
        assert!(store.cached("tgv", "b"));
        done(&mut store, "reference", "r", &[raw], &[Item::Chi]);
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
    /// two-pass, with every output on: only the inversions' raw map, and the working echoes, which
    /// the last of the phase steps writes (phase scaling, or the resampling where an axial-only
    /// inversion may resample). Not a rule the stores rely on ([`sharing_outputs`] holds for any),
    /// but the list of what they meet.
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
        let names = |s: &[&str]| s.iter().map(|n| n.to_string()).collect::<std::collections::BTreeSet<String>>();
        let mut want = HashMap::from([
            (Item::RawChi(Pass::Main), names(&["invert", "qsmart", "tgv"])),
            (Item::RawChi(Pass::Reliable), names(&["invert-reliable", "qsmart-reliable", "tgv-reliable"])),
        ]);
        for e in 1..=facts.n_echoes {
            want.insert(Item::Phase(e), names(&["scale_phase", "resample_axial"]));
            want.insert(Item::Magnitude(e), names(&["scale_phase", "resample_axial"]));
        }
        assert_eq!(writers, want);
    }

    #[test]
    fn items_are_read_back_from_their_keys() {
        use qsm_core::pipeline::qsmart::outputs as q;
        let mut items = vec![
            Item::AcquiredPhase(1), Item::AcquiredMagnitude(12), Item::AcquiredReal(3), Item::AcquiredImaginary(2), Item::CoilPhase(3, 1),
            Item::CoilMagnitude(1, 4), Item::CombinedEchoPhase(2), Item::CombinedEchoMagnitude(3),
            Item::CoilCombinationMask, Item::ScaledPhase(2), Item::ScaledMagnitude(3), Item::WindowedPhase(4),
            Item::WindowedMagnitude(1), Item::Phase(3), Item::Magnitude(2), Item::CorrectedPhase(4),
            Item::CorrectedMagnitude(5), Item::HomogeneousMagnitude(2), Item::VesselMagnitude, Item::CombinedMagnitude, Item::BrainMask, Item::SuppliedMask,
            Item::CoverageMask, Item::FieldPpm, Item::TotalFieldHz, Item::TotalFieldMagnitude,
            Item::RobustFitReport, Item::LocalField(Pass::Main), Item::LocalFieldMask(Pass::Reliable),
            Item::RawChi(Pass::Reliable), Item::ReliableMask, Item::QsmartIntermediate(Pass::Main, q::VESSEL_MASK),
            Item::QsmartIntermediate(Pass::Reliable, q::CHI_STAGE2), Item::CombinedRawChi, Item::Chi,
            Item::ChiSupport, Item::SinglePassChi, Item::Segmentation, Item::SuppliedSegmentation,
            Item::SegmentationLabels, Item::SegmentationVolumes, Item::RegionStats, Item::MotionReport, Item::Swi,
            Item::SwiMip, Item::SwiPhase, Item::R2star, Item::T2star, Item::MeseMagnitude(6), Item::SuppliedR2, Item::R2,
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
        let mip = Geometry { dims: (2, 1, 1), voxel_size: (1.0, 1.0, 4.0), affine: [0.0; 16], orientation: None };
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
