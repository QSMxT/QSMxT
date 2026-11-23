//! The run tests' scan and configuration: a phantom in memory, a quick configuration for it, and
//! the observers and stores the tests watch a run through.

use super::*;
use crate::enums::{BfAlgorithm, QsmAlgorithm};
use crate::masking::{MaskOp, MaskSection, MaskThresholdMethod, MaskingInput};
use crate::observe::VolumeData;
use crate::store::{Header, MemoryStore};
use std::collections::HashMap;

pub(crate) const N: usize = 16;
pub(crate) const TES: [f64; 3] = [0.005, 0.010, 0.015];

/// A sphere of tissue in a 16³ box at 1 mm, three echoes: a smooth local field inside it
/// (wrapped into the phase at each echo time) and a magnitude that is bright inside, dark out.
pub(crate) fn phantom() -> (ScanFacts, MemoryStore) {
    let mut affine = [0.0; 16];
    for i in 0..4 { affine[5 * i] = 1.0; }
    phantom_on(affine)
}

/// The phantom, its voxels placed in the world by `affine`.
pub(crate) fn phantom_on(affine: [f64; 16]) -> (ScanFacts, MemoryStore) {
    let header = Header { dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), affine, orientation: None, sidecar: None };
    let mut store = MemoryStore::new(header, json!("phantom-1"));
    let c = (N as f64 - 1.0) / 2.0;
    let inside = |x: usize, y: usize, z: usize| {
        let (dx, dy, dz) = (x as f64 - c, y as f64 - c, z as f64 - c);
        dx * dx + dy * dy + dz * dz < 36.0
    };
    let (mut field_hz, mut magnitude) = (vec![0.0; N * N * N], vec![0.0; N * N * N]);
    for z in 0..N { for y in 0..N { for x in 0..N {
        let i = x + y * N + z * N * N;
        // Tens of Hz, so the later echoes wrap, and nonzero outside the object too, as
        // acquired phase is: phase that never reaches ±π is taken as radians as it is
        // (scale_phases_to_pi), and phase that is exactly 0 reads as "the scanner wrote none".
        field_hz[i] = 40.0 * ((x as f64 - c) / 6.0) + 20.0 * ((z as f64 - c) / 6.0).powi(2);
        if inside(x, y, z) {
            magnitude[i] = 100.0;
        } else {
            magnitude[i] = 1.0;
        }
    }}}
    for (e, te) in TES.iter().enumerate() {
        let phase: Vec<f64> = field_hz.iter()
            .map(|f| (2.0 * std::f64::consts::PI * f * te + std::f64::consts::PI).rem_euclid(2.0 * std::f64::consts::PI)
                - std::f64::consts::PI)
            .collect();
        let mag: Vec<f64> = magnitude.iter().map(|m| m * (-te / 0.04).exp()).collect();
        store.insert(Item::AcquiredPhase(e + 1), VolumeData::Real(phase));
        store.insert(Item::AcquiredMagnitude(e + 1), VolumeData::Real(mag));
    }
    let facts = ScanFacts {
        dims: (N, N, N), n_echoes: TES.len(), echo_times: TES.to_vec(), has_magnitude: true,
        voxel_size: None,
        n_coils: 1, real_imaginary: false, field_strength: 3.0, nominal_field_strength: Some(3.0),
        // 3 T exactly: an ordinary scan converts with its centre frequency and records nothing.
        imaging_frequency: Some(3.0 * qsm_core::constants::GAMMA_BAR_MHZ_PER_T), b0_dir: None,
        mese_echo_times: None, custom_mask: false, custom_dseg: false, custom_qsm: false,
        custom_r2: false, custom_r2prime: false, manufacturer: None, readout_polarity: None,
    };
    (facts, store)
}

/// A quick configuration for a tiny phantom: a magnitude threshold for the mask, Laplacian
/// unwrapping, LBV (which does not erode a small brain away) and TKD. The iterative
/// algorithms a test switches to (TGV, QSMART, iLSQR, LSQR, χ-separation) get a few
/// iterations, no padding and small kernels: the tests check what a run computes, keeps and records, not how good
/// the maps are.
pub(crate) fn config() -> PipelineConfig {
    let mut config = PipelineConfig::default();
    config.masking.sections = vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        refinements: vec![],
    }];
    config.masking.refinements = vec![];
    config.masking.inhomogeneity_correction = false;
    config.field_mapping.unwrapping_algorithm = crate::enums::UnwrappingAlgorithm::Laplacian;
    config.bg_removal.algorithm = BfAlgorithm::Lbv;
    config.inversion.algorithm = QsmAlgorithm::Tkd;
    config.inversion.tgv.iterations = Some(10);
    let qsmart = &mut config.inversion.qsmart;
    qsmart.ilsqr_max_iter = 5;
    qsmart.ilsqr_pad_mm = 0.0;
    qsmart.vasc_sphere_radius = 2;
    qsmart.frangi_scale_max = 1.0;
    config.separation.chi_sep_ilsqr.cg_max_iter_lsqr = 3;
    config.separation.chi_sep_ilsqr.cg_max_iter_sa = 3;
    config.separation.chi_sep_ilsqr.pad = [0, 0, 0];
    config.separation.chi_sep_ilsqr.qsm_ilsqr.pad_mm = 0.0;
    config.separation.chi_sep_ilsqr.qsm_ilsqr.max_iter = 2;
    (config.inversion.ilsqr.pad_mm, config.inversion.ilsqr.max_iter) = (0.0, 2);
    config.inversion.lsqr.max_iter = 5;
    config
}

/// A store that also keeps what each computed step recorded reading and writing: each step's
/// name, its inputs and its outputs.
pub(crate) struct Recorder<St> {
    pub inner: St,
    pub completed: Vec<(String, Vec<Item>, Vec<Item>)>,
    /// Every item each step asked the store for while it was computed (from its `start` to
    /// its `record`), by step; what is read outside a step is under `""`.
    pub asked: HashMap<String, Vec<Item>>,
    current: Option<String>,
}

impl<St> Recorder<St> {
    pub(crate) fn new(inner: St) -> Self {
        Recorder { inner, completed: Vec::new(), asked: HashMap::new(), current: None }
    }

    fn ask(&mut self, item: &Item) {
        let step = self.current.clone().unwrap_or_default();
        let asked = self.asked.entry(step).or_default();
        if !asked.contains(item) {
            asked.push(*item);
        }
    }
}

impl<St: StageStore<Error = crate::EngineError>> StageStore for Recorder<St> {
    type Error = crate::EngineError;
    fn input_identity(&mut self) -> Value { self.inner.input_identity() }
    fn header(&mut self) -> Result<Header, Self::Error> { self.inner.header() }
    fn sidecar(&mut self) -> Result<Option<crate::scan::QsmSidecar>, Self::Error> { self.inner.sidecar() }
    fn coils(&mut self) -> Vec<u32> { self.inner.coils() }
    fn cached_scan(&mut self, key: &str) -> Option<RunMetadata> { self.inner.cached_scan(key) }
    fn record_scan(&mut self, key: &str, meta: &RunMetadata, decisions: &[Decision]) -> Result<(), Self::Error> {
        self.inner.record_scan(key, meta, decisions)
    }
    fn decisions(&mut self, step: &str) -> Vec<Decision> { self.inner.decisions(step) }
    fn begin(&mut self, meta: &RunMetadata) -> Result<(), Self::Error> { self.inner.begin(meta) }
    // Outside a step, whether an item is there decides a key (the statistics': which maps it
    // summarises), which is what keys are for: only a step's own look counts as reading.
    fn has(&mut self, item: &Item) -> bool {
        if self.current.is_some() {
            self.ask(item);
        }
        self.inner.has(item)
    }
    fn real(&mut self, item: &Item) -> Result<Vec<f64>, Self::Error> { self.ask(item); self.inner.real(item) }
    fn mask(&mut self, item: &Item) -> Result<Vec<u8>, Self::Error> { self.ask(item); self.inner.mask(item) }
    fn supplied_mask(&mut self) -> Supplied { self.inner.supplied_mask() }
    fn mese(&mut self) -> Option<crate::store::Mese> { self.inner.mese() }
    fn supplied(&mut self, item: &Item) -> Supplied { self.ask(item); self.inner.supplied(item) }
    fn supplied_grid(&mut self, item: &Item) -> Option<crate::store::SuppliedGrid> {
        self.inner.supplied_grid(item)
    }
    fn identity(&mut self, item: &Item) -> Value { self.inner.identity(item) }
    fn recorded_key(&mut self, step: &str) -> Option<String> { self.inner.recorded_key(step) }
    fn cached(&mut self, step: &str, key: &str) -> bool { self.inner.cached(step, key) }
    fn start(&mut self, step: &str) { self.current = Some(step.to_string()); self.inner.start(step) }
    fn put(&mut self, item: Item, data: Data<'_>) -> Result<(), Self::Error> { self.inner.put(item, data) }
    fn annotate(&mut self, item: &Item, fields: serde_json::Map<String, Value>) -> Result<(), Self::Error> {
        self.inner.annotate(item, fields)
    }
    fn report(&mut self, item: &Item) -> Option<Value> { self.ask(item); self.inner.report(item) }
    fn annotation(&mut self, item: &Item) -> serde_json::Map<String, Value> { self.ask(item); self.inner.annotation(item) }
    fn discard(&mut self, step: &str, outputs: &[Item]) -> Result<(), Self::Error> { self.inner.discard(step, outputs) }
    fn record(&mut self, c: &Completed<'_>) -> Result<(), Self::Error> {
        self.completed.push((c.step.to_string(), c.inputs.to_vec(), c.outputs.to_vec()));
        self.current = None;
        self.inner.record(c)
    }
}

/// Which steps were computed and which were cached, in order.
#[derive(Default)]
pub(crate) struct Events {
    pub(crate) computed: Vec<String>,
    pub(crate) cached: Vec<String>,
    pub(crate) outputs: Vec<Item>,
}

impl Observer for Events {
    fn stage(&mut self, step: &str, _algorithm: Option<&str>, event: StageEvent<'_>) {
        match event {
            StageEvent::Started(_) => self.computed.push(step.to_string()),
            StageEvent::Cached => self.cached.push(step.to_string()),
            StageEvent::Finished(_) => {}
        }
    }

    fn output(&mut self, item: &Item, _volume: qsm_core::pipeline::VolumeRef<'_>) {
        self.outputs.push(*item);
    }
}

pub(crate) fn run_once(config: &PipelineConfig, facts: &ScanFacts, store: &mut MemoryStore) -> Events {
    let plan = crate::plan::plan(config, facts);
    let mut events = Events::default();
    run(&plan, config, facts, store, &mut events).unwrap();
    events
}

/// The steps a run took before it failed (or finished): a deep-learning inversion has no
/// weights here, so it fails at that step, after everything before it has been reused or
/// computed.
pub(crate) fn run_until_done_or_failed(config: &PipelineConfig, facts: &ScanFacts, store: &mut MemoryStore) -> Events {
    let plan = crate::plan::plan(config, facts);
    let mut events = Events::default();
    let _ = run(&plan, config, facts, store, &mut events);
    events
}

/// What a dry run says of each step, after checking that `run` then does exactly that: the
/// steps it says are cached are the ones the run reuses, and the ones it says are computed the
/// ones the run computes. The store is left as it was by the dry run.
pub(crate) fn expect_then_run(config: &PipelineConfig, facts: &ScanFacts, store: &mut MemoryStore) -> Vec<(String, ExpectedState)> {
    let plan = crate::plan::plan(config, facts);
    let recorded = serde_json::to_value(store.recorded()).unwrap();
    let expected = dry_run(&plan, config, facts, store).unwrap();
    assert_eq!(serde_json::to_value(store.recorded()).unwrap(), recorded, "a dry run changes nothing");
    assert_eq!(expected.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
               plan.steps.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), "every step, in order");
    let events = run_until_done_or_failed(config, facts, store);
    let named = |want: fn(ExpectedState) -> bool| expected.iter().filter(|e| want(e.state)).map(|e| e.id.clone()).collect::<Vec<_>>();
    assert_eq!(named(|s| s == ExpectedState::Cached), events.cached, "cached");
    assert_eq!(named(ExpectedState::computes), events.computed, "computed");
    // Each step computed is recorded under the key the dry run gave.
    for e in expected.iter().filter(|e| e.state.computes()) {
        if events.computed.contains(&e.id) && e.id != "load" {
            assert_eq!(store.recorded_keys().get(&e.id), e.key.as_ref(), "{}'s key", e.id);
        }
    }
    expected.into_iter().map(|e| (e.id, e.state)).collect()
}

/// The phantom with a MESE on its own grid (the run's): spin echoes at n·10 ms decaying with
/// R2 = 10 Hz inside the sphere.
pub(crate) fn with_mese(facts: &mut ScanFacts, store: &mut MemoryStore) {
    let tes: Vec<f64> = vec![0.01, 0.02, 0.03, 0.04];
    let mut affine = [0.0; 16];
    for i in 0..4 { affine[5 * i] = 1.0; }
    let geometry = crate::store::Geometry { dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), affine, orientation: None };
    let Some(VolumeData::Real(m)) = store.get(&Item::AcquiredMagnitude(1)).cloned() else { panic!() };
    for (e, te) in tes.iter().enumerate() {
        let echo: Vec<f64> = m.iter().map(|&v| if v > 50.0 { 1000.0 * (-10.0f64 * te).exp() } else { 1.0 }).collect();
        store.insert_on_grid(Item::MeseMagnitude(e + 1), echo, geometry.clone());
    }
    store.supply_mese(crate::store::Mese { name: "MESE".into(), dims: (N, N, N), affine, echo_times: tes.clone() });
    facts.mese_echo_times = Some(tes);
}

/// Two-pass on the phantom, its reliable-phase mask a fixed threshold.
pub(crate) fn two_pass_config() -> PipelineConfig {
    let mut config = config();
    config.masking.two_pass = true;
    config.masking.two_pass_sections = Some(vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(40.0) },
        refinements: vec![],
    }]);
    config
}

/// Another brain mask for the phantom: the usual one, eroded.
pub(crate) fn erode_the_brain_mask(config: &mut PipelineConfig) {
    config.masking.sections[0].refinements = vec![MaskOp::Erode { iterations: 1 }];
}
