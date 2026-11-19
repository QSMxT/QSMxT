//! [`MemoryStore`]: a [`StageStore`] that keeps everything in memory, its volumes in [`InMemory`]
//! or wherever the front end chooses ([`Shadow`] for a dry run).

use std::collections::HashMap;

use qsm_core::pipeline::VolumeRef;

use super::{sharing_outputs, Completed, Data, Geometry, Header, Item, Mese, Recorded, StageStore, Supplied, SuppliedGrid, Volumes, OUTPUT_SPACE};
use crate::error::EngineError;
use crate::observe::VolumeData;
use crate::record::Decision;
use crate::scan::{QsmSidecar, RunMetadata};

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
    /// What identifies each file supplied with the scan ([`Self::supply`], [`Self::supply_mask`]).
    supplied_identities: HashMap<Item, String>,
    mese: Option<Mese>,
    /// The step being computed.
    current: Option<String>,
    /// The outputs returned to the acquired grid whose working-grid versions are kept aside, with
    /// the grid each of those was on when not the run's.
    working_grid: HashMap<Item, Option<Geometry>>,
    /// Each recorded step's key, and the outputs it recorded.
    steps: HashMap<String, (String, Vec<Item>)>,
    /// What each recorded step read ([`Completed::inputs`]); none for a step restored from a record
    /// made before stores kept it. An entry for a step no longer recorded means nothing.
    reads: HashMap<String, Vec<Item>>,
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
            annotations: HashMap::new(), supplied: None, supplied_identities: HashMap::new(), mese: None, current: None,
            working_grid: HashMap::new(), steps: HashMap::new(), reads: HashMap::new(), scan: None, decisions: HashMap::new(),
        }
    }

    /// Replace the scan's input data: the next run recomputes everything.
    pub fn set_identity(&mut self, identity: serde_json::Value) {
        self.identity = identity;
    }

    /// Supply a brain mask with the scan. `identity` is anything that changes when the mask does
    /// (its name and a version, or a hash of its file): it is in the mask step's key, so a new mask
    /// recomputes the mask and what follows from it, and nothing else.
    pub fn supply_mask(&mut self, mask: Supplied, identity: impl Into<String>) {
        self.supplied = Some(mask);
        self.supplied_identities.insert(Item::SuppliedMask, identity.into());
    }

    /// A map supplied with the scan ([`Item::SuppliedChi`], [`Item::SuppliedR2`],
    /// [`Item::SuppliedR2prime`], [`Item::SuppliedSegmentation`]), which the volumes hold as
    /// `item`, on `geometry`'s grid. `identity` names it in the log and is in the keys of the steps
    /// that read it: give it something that changes when the file does (its name and a version, or
    /// a hash of its file), so replacing the file recomputes those steps and nothing else.
    pub fn supply(&mut self, item: Item, geometry: Geometry, identity: impl Into<String>) {
        self.geometries.insert(item, geometry);
        self.supplied_identities.insert(item, identity.into());
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
            reads: self.reads.iter().filter(|(s, _)| self.steps.contains_key(*s)).map(|(s, r)| (s.clone(), keys(r))).collect(),
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
        // A step that read an item this engine does not have is left as one recorded without what
        // it read: the fixed table says what makes it stale.
        self.reads = recorded.reads.into_iter().filter_map(|(step, read)| {
            Some((step, read.iter().map(|k| Item::from_key(k)).collect::<Option<Vec<_>>>()?))
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

    /// Forget `step` and every recorded step that read what it recorded writing.
    fn forget(&mut self, step: &str) {
        let outputs = self.steps.remove(step).map(|(_, outputs)| outputs).unwrap_or_default();
        for r in self.readers(step, &outputs) {
            self.steps.remove(&r);
        }
        self.changed();
    }

    /// The recorded steps that read one of `outputs` of `step` ([`crate::graph::readers`]).
    fn readers(&self, step: &str, outputs: &[Item]) -> Vec<String> {
        crate::graph::readers(step, outputs, self.steps.keys().map(|s| (s.as_str(), self.reads.get(s).map(Vec::as_slice))))
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
            // In the keys, which a dry run does compute.
            supplied_identities: self.supplied_identities.clone(),
            mese: self.mese.clone(),
            current: None,
            working_grid: self.working_grid.clone(),
            steps: self.steps.clone(),
            reads: self.reads.clone(),
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
        let origin = self.supplied_identities.get(item).cloned().unwrap_or_else(|| format!("{item:?}"));
        let (Ok(Some(VolumeData::Real(data))), Some(g)) = (Volumes::get(&self.volumes, item), self.geometries.get(item)) else {
            return Supplied::Missing { searched: "the loaded files".into() };
        };
        Supplied::Found { origin, data, dims: g.dims, affine: g.affine }
    }

    /// The grid it was given ([`Self::supply`], [`Self::insert_on_grid`], [`Self::set_geometry`]),
    /// named by the identity [`Self::supply`] gave; `None` for one held on the run's grid.
    fn supplied_grid(&mut self, item: &Item) -> Option<SuppliedGrid> {
        let g = self.geometries.get(item)?;
        let origin = self.supplied_identities.get(item).cloned().unwrap_or_else(|| format!("{item:?}"));
        self.volumes.contains(item).then_some(SuppliedGrid { origin, dims: g.dims, affine: g.affine })
    }

    /// What [`Self::supply`] or [`Self::supply_mask`] gave for a supplied file; otherwise the
    /// item's name, which stays the same whatever the item holds.
    fn identity(&mut self, item: &Item) -> serde_json::Value {
        serde_json::json!(self.supplied_identities.get(item).cloned().unwrap_or_else(|| format!("{item:?}")))
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
        match self.steps.get(c.step) {
            Some((key, _)) if key == c.key => {}
            // Whatever read what it wrote before, or writes now, read something else.
            before => {
                let mut written = c.outputs.to_vec();
                written.extend(before.into_iter().flat_map(|(_, out)| out.iter().copied()));
                for r in self.readers(c.step, &written) {
                    self.steps.remove(&r);
                }
            }
        }
        self.steps.insert(c.step.to_string(), (c.key.to_string(), c.outputs.to_vec()));
        self.reads.insert(c.step.to_string(), c.inputs.to_vec());
        self.decisions.insert(c.step.to_string(), c.decisions.to_vec());
        self.changed();
        Ok(())
    }
}
