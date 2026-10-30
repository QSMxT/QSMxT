//! How much memory each step of a run takes, and so the run's peak, before it starts.
//!
//! [`estimate`] gives every step of a [`Plan`] the memory it holds at its peak, on a host and
//! store described by a [`MemoryProfile`]:
//!
//! - **The step's own need** ([`step_need`]): copies of the volumes it reads, its working memory
//!   and its outputs. Measured per step on the example data (184×256×128, 5 echoes) by counting
//!   allocations, and scaled by the voxel count and, where it matters, the echo count and the
//!   threads. The deep-learning steps follow the network and its patch or tile settings.
//! - **What the store holds while the step computes** ([`StoreKind`]): nothing when the volumes
//!   live outside the memory estimated (`qsmxt run`'s files; qsmbly's page, outside the WASM
//!   heap); every volume read or written so far, in double precision, for a [`MemoryStore`] with
//!   its own volumes.
//! - **The host** ([`Host`]): natively, a margin for the allocator and the NIfTI reads and writes.
//!   In a browser, the WASM heap also holds the run's deep-learning weights from the start, it
//!   never shrinks, and it cannot grow past [`WASM32_HEAP_BYTES`].
//!
//! Byte counts are `u64`, not `usize`: a run's peak can exceed 4 GB, which a 32-bit (WebAssembly)
//! `usize` cannot hold. Querying the host's memory is the front end's job.
//!
//! [`MemoryStore`]: crate::store::MemoryStore

use serde::{Deserialize, Serialize};

use crate::config::*;
use crate::enums::*;
use crate::items::ItemKind;
use crate::masking::*;
use crate::plan::{Plan, Stage, Step};
use crate::scan::ScanFacts;
use crate::store::Item;

const F64: u64 = 8;
const GIB: u64 = 1 << 30;

/// The memory a 32-bit WebAssembly module can address: what a browser run has, all told.
pub const WASM32_HEAP_BYTES: u64 = 4 * GIB;

/// Where a run's volumes live between its steps, as far as the memory estimated is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoreKind {
    /// Outside it: `qsmxt run`'s files, or qsmbly's page (outside the WASM heap). A step holds only
    /// the volumes it reads, its working memory and what it writes.
    Disk,
    /// In it: a [`crate::store::MemoryStore`] with its own volumes, which holds every volume the
    /// run has read or written so far, in double precision, while each step computes.
    Memory,
}

/// What a run computes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Host {
    /// A native process (`qsmxt run`).
    Native,
    /// A browser's WebAssembly heap: it holds the run's deep-learning weights from the start,
    /// never shrinks, and cannot grow past [`WASM32_HEAP_BYTES`].
    Browser,
}

/// The host and store a memory estimate is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryProfile {
    pub host: Host,
    pub store: StoreKind,
    /// The threads the run computes on. Some steps hold working memory per thread.
    pub threads: usize,
}

impl MemoryProfile {
    /// `qsmxt run`: native, its volumes in files, on a typical workstation's threads.
    pub const NATIVE: Self = Self { host: Host::Native, store: StoreKind::Disk, threads: 8 };

    /// A browser run on `threads` threads, its volumes kept outside the WASM heap (qsmbly).
    pub fn browser(threads: usize) -> Self {
        Self { host: Host::Browser, store: StoreKind::Disk, threads }
    }

    /// The most a run can take on this host, when there is a hard limit: the WASM heap's.
    pub fn budget_bytes(&self) -> Option<u64> {
        match self.host {
            Host::Browser => Some(WASM32_HEAP_BYTES),
            Host::Native => None,
        }
    }
}

impl Default for MemoryProfile {
    fn default() -> Self {
        Self::NATIVE
    }
}

/// The memory each step of a plan takes at its peak, in plan order, and the run's peak.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MemoryEstimate {
    /// One figure per step of the plan, in bytes.
    pub steps: Vec<u64>,
    /// The largest of them: the run's peak.
    pub peak_bytes: u64,
    /// The index in the plan of the step that sets the peak (the first, on a tie), when the plan
    /// computes anything.
    pub peak_step: Option<usize>,
}

/// The memory each step of `plan` takes at its peak on `facts`' scan, for `profile`'s host and
/// store.
pub fn estimate(plan: &Plan, facts: &ScanFacts, profile: &MemoryProfile) -> MemoryEstimate {
    let s = Scan::of(facts, profile);
    let config = &plan.config;
    // Deep-learning weights a browser holds in its heap for the whole run.
    let weights: u64 = match profile.host {
        Host::Browser => plan.models.iter().map(|id| model_bytes(id)).sum(),
        Host::Native => 0,
    };
    // What a store in the same memory holds before the first step: the scan as given.
    let mut resident = match profile.store {
        StoreKind::Memory => acquired_bytes(facts, s.n),
        StoreKind::Disk => 0,
    };
    let mut steps = Vec::with_capacity(plan.steps.len());
    for step in &plan.steps {
        let need = step_need(step, config, &s);
        let held = match profile.host {
            // Natively a step reads its networks' weights itself.
            Host::Native => (need + step.models.iter().map(|id| model_bytes(id)).sum::<u64>()) * 13 / 10,
            Host::Browser => BROWSER_RUNTIME_BYTES + weights + need,
        };
        steps.push(held + resident);
        if profile.store == StoreKind::Memory {
            resident += step.outputs.iter().map(|i| item_bytes(i, s.n)).sum::<u64>();
        }
    }
    let peak_bytes = steps.iter().copied().max().unwrap_or(0);
    let peak_step = plan.processes().then(|| steps.iter().position(|&b| b == peak_bytes)).flatten();
    MemoryEstimate { steps, peak_bytes, peak_step }
}

/// What the WASM module, its runtime and the allocator's slack add to a step's own need in the
/// heap (measured on the example: 40–160 MB, the most for the FFT-based steps; the lighter steps
/// stay inside the heap the runtime starts with).
const BROWSER_RUNTIME_BYTES: u64 = 100_000_000;

/// The scan as the estimate sees it.
#[derive(Clone, Copy)]
struct Scan {
    dims: [usize; 3],
    /// Voxels.
    n: u64,
    echoes: u64,
    coils: u64,
    /// A MESE acquisition comes with the scan.
    mese: bool,
    host: Host,
    threads: u64,
}

impl Scan {
    fn of(facts: &ScanFacts, profile: &MemoryProfile) -> Self {
        let (x, y, z) = facts.dims;
        Scan {
            dims: [x, y, z],
            n: x as u64 * y as u64 * z as u64,
            echoes: facts.n_echoes.max(1) as u64,
            coils: facts.n_coils.max(1) as u64,
            mese: facts.mese_echo_times.is_some(),
            host: profile.host,
            threads: profile.threads.max(1) as u64,
        }
    }

    /// `per_voxel` bytes for every voxel.
    fn v(&self, per_voxel: u64) -> u64 {
        per_voxel * self.n
    }

    /// The scan as a step of a per-echo QSM sees it (one echo), or as it is.
    fn per_echo(&self, config: &PipelineConfig) -> Scan {
        let echoes = if crate::bridge::per_echo(config, self.echoes as usize) { 1 } else { self.echoes };
        Scan { echoes, ..*self }
    }
}

/// An item's bytes in memory: a mask a byte a voxel, a real volume a double, a table or report
/// next to nothing.
fn item_bytes(item: &Item, n: u64) -> u64 {
    match item.meta().kind {
        ItemKind::Mask => n,
        ItemKind::Table | ItemKind::Report => 0,
        _ => F64 * n,
    }
}

/// The scan as given, in a store in memory: every echo's phase and magnitude (each coil's, for
/// uncombined coils; the real and imaginary parts, for a scan given as those).
fn acquired_bytes(facts: &ScanFacts, n: u64) -> u64 {
    let echoes = facts.n_echoes as u64;
    let per_echo = if facts.real_imaginary || facts.has_magnitude { 2 } else { 1 };
    per_echo * echoes * facts.n_coils.max(1) as u64 * F64 * n
}

/// A model's weights, as QSM.rs's registry gives their size.
fn model_bytes(id: &str) -> u64 {
    qsm_core::models::find_model(id).map_or(0, |m| m.files.iter().map(|f| f.bytes).sum())
}

/// What `step` holds at its peak on its own: copies of what it reads, its working memory, and
/// its outputs.
fn step_need(step: &Step, config: &PipelineConfig, s: &Scan) -> u64 {
    let (n, e) = (s.n, s.echoes);
    let n4 = config.masking.inhomogeneity_correction && config.masking.inhomogeneity_method == InhomogeneityMethod::N4;
    let mask_recipe = |sections: &[MaskSection]| mask_need(sections, s, n4);
    match step.stage {
        Stage::Load => 0,
        // One echo at a time; uncombined coils are combined (MCPC-3D-S) first, with every coil's
        // phase and magnitude held at once.
        Stage::ScalePhase if s.coils > 1 => (2 * s.coils * e + 4 + 2 * e) * F64 * n,
        Stage::ScalePhase => s.v(8),
        // Every echo's magnitude, read and combined, and the correction's working memory: N4's
        // is less than makehomogeneous's (measured: 8 B a voxel an echo and 33 more).
        Stage::Magnitude => match n4 {
            true => s.v(8 * e + 33),
            false => s.v(8 * e + 49),
        },
        Stage::Mask(crate::plan::Pass::Main) => mask_recipe(&config.masking.sections),
        Stage::Mask(crate::plan::Pass::Reliable) => mask_recipe(&config.masking.resolved_two_pass_sections()),
        // Every echo's phase and magnitude, registered to the reference echo.
        Stage::Motion => s.v(16 * e + 113),
        // CLEAR-SWI: every echo's phase and magnitude and each echo's filtered phase; ROMEO adds
        // its weights.
        Stage::Swi => match config.swi.unwrap {
            crate::SwiUnwrap::Romeo => s.v((4 * e + 20) * F64),
            _ => s.v(24 * e + 28),
        },
        // The echoes' magnitudes and the fit's maps.
        Stage::Relaxometry => s.v(8 * e + 25),
        Stage::R2 => r2_need(step, config, s),
        Stage::FieldMap => unwrap_need(config, s),
        // A per-echo QSM runs these one echo at a time, each as a single-echo scan's.
        Stage::BackgroundRemoval(_) => bg_removal_need(config, &s.per_echo(config)),
        Stage::Inversion(_) => inversion_need(config, &s.per_echo(config)),
        // From the total field: TGV's f32 workspace over the brain's bounding box (the whole
        // volume, at worst) and its stencils.
        Stage::Tgv(_) => s.v(TGV_PER_VOXEL),
        // Two background-field removals and iLSQR inversions, the vessel filter, and the
        // intermediates it keeps.
        Stage::Qsmart(_) => s.v(QSMART_PER_VOXEL),
        Stage::TwoPassCombination => s.v(27),
        // A per-echo QSM's referencing also holds every echo's map, for their mean.
        Stage::Reference if crate::bridge::per_echo(config, s.echoes as usize) => s.v(18 + F64 * e),
        Stage::Reference | Stage::SinglePassReference => s.v(18),
        Stage::Smwi => s.v(65),
        Stage::Segmentation => match step.models.is_empty() {
            // A supplied segmentation, brought onto the run's grid.
            true => s.v(24),
            false => synthseg_bytes(s.dims, config.segmentation.patch) + s.v(16),
        },
        Stage::ChiSeparation => separation_need(config, s),
        // Each map summarised over the segmentation, one at a time.
        Stage::Analysis => s.v(32),
        // Each output brought back to the acquired grid, one at a time (nothing when the run was
        // not resampled).
        Stage::OutputSpace => s.v(24),
    }
}

/// The mask step reads every echo's phase and magnitude whatever the recipe, and its peak is the
/// same for the thresholds, BET and the morphological refinements (measured: 24 B a voxel an
/// echo and 42 more; 36 more when the echoes are corrected with N4). A network takes its own peak
/// on top of the echoes.
fn mask_need(sections: &[MaskSection], s: &Scan, n4: bool) -> u64 {
    let echoes = s.v(16 * s.echoes);
    let network = sections.iter().map(|sec| match &sec.generator {
        MaskOp::HdBet { patch, .. } => hd_bet_network_bytes(*patch) + s.v(40),
        MaskOp::Rs2Net { .. } => RS2_NET_BYTES,
        MaskOp::MindGrab { .. } => MINDGRAB_BYTES,
        _ => 0,
    }).max().unwrap_or(0);
    s.v(24 * s.echoes + if n4 { 36 } else { 42 }).max(echoes + network)
}

/// The field map (`unwrap`): unwrapping and echo combination.
fn unwrap_need(config: &PipelineConfig, s: &Scan) -> u64 {
    let (n, e) = (s.n, s.echoes);
    let fm = &config.field_mapping;
    let romeo = fm.unwrapping_algorithm == UnwrappingAlgorithm::Romeo;
    if fm.b0_estimation == B0Estimation::ComplexFit {
        // MEDI's complex fit: every echo as complex and as its magnitude besides the inputs, then
        // one volume unwrapped, by ROMEO or Laplacian alike (natively on the example, 1.74 GB on
        // 5 echoes and 1.26 GB on 3, on 1 and 8 threads). A single echo is fitted as two.
        return (40 * e.max(2) + 89) * n;
    }
    // One echo, or a per-echo QSM's fields, unwrapped one echo at a time.
    if e == 1 || crate::bridge::per_echo(config, e as usize) {
        return 85 * n;
    }
    if !romeo {
        // Laplacian: each echo unwrapped and kept, then the fit (with or without offsets). The
        // mirror solver works on the volume mirrored to twice its size along each axis, one echo
        // at a time: a complex volume eight times the size, 128 B a voxel (measured natively by
        // peak RSS on 178×182×148, 5 echoes: 0.46 GB above the DCT solver's).
        let mirror = if fm.laplacian_solver == LaplacianSolver::Mirror { 128 * n } else { 0 };
        return (24 * e + 25) * n + mirror;
    }
    if !fm.phase_offset_removal {
        // Each echo unwrapped and kept, then the linear fit.
        return (25 * e + 56) * n;
    }
    // Phase-offset removal: the echoes, the offsets and their smoothing, and ROMEO for each
    // echo, the echoes in parallel, each with its own weights and region-growing state.
    (34 * e + 62) * n + romeo_threads(s) * ROMEO_PER_ECHO_THREAD * n
}

/// ROMEO's per-echo working memory, held once per echo unwrapped at the same time.
const ROMEO_PER_ECHO_THREAD: u64 = 31;

/// Echoes' worth of ROMEO working memory held beyond the first, with phase-offset removal on
/// several threads. Natively up to one per echo (measured on the example's 5 echoes: 1.9–2.5 GB
/// on 8 threads against 1.4 GB on one, as rayon splits the work); a browser's pool keeps it to
/// about two whatever its size (measured on 4 and 16 threads).
fn romeo_threads(s: &Scan) -> u64 {
    match (s.host, s.threads) {
        (_, 1) => 0,
        (Host::Native, t) => t.min(s.echoes),
        (Host::Browser, t) => (t - 1).min(2),
    }
}

/// TGV's bytes per voxel: its f32 workspace over the brain's bounding box (the whole volume, at
/// worst), the inputs and outputs.
const TGV_PER_VOXEL: u64 = 128;
/// QSMART's bytes per voxel.
const QSMART_PER_VOXEL: u64 = 271;

/// What a deep-learning network needs besides its activations, and its activations measured
/// where qsm-core has no figure.
struct Net {
    /// Registry id (`qsm_core::models`), for qsm-core's per-tile cost.
    id: &'static str,
    /// Bytes per voxel of the volume (input, output, the tiles' blending), whole-volume and native.
    per_voxel: u64,
    /// Bytes held once (the parsed model, for the networks that parse it once).
    fixed: u64,
    /// Activation bytes per voxel of the volume, whole-volume.
    whole: u64,
    /// Bytes per patch voxel of one tile natively, counted by allocation (qsm-core's per-tile
    /// figure is the WASM heap's, fragmentation included, and over-counts a native run).
    native_tile: u64,
    /// Bytes per voxel of the volume of a tiled run in a browser, and bytes it holds once
    /// whatever the number of tiles (its plan and full-volume buffers), besides qsm-core's
    /// per-tile figure, which leaves them out. Fitted to the WASM harness's tiled runs.
    browser_per_voxel: u64,
    once: u64,
    /// Bytes each pool thread not running a tile of its own adds in a browser: a network whose
    /// per-patch work is itself parallel lends it to the idle threads, and each holds its own
    /// buffers. NeXtQSM (its whole VarNet unroll runs inside every patch), measured on the example
    /// with its 2 tiles in flight: 0.96 GB of heap with a 1-thread pool, 1.50 GB with 3 and
    /// 2.38 GB with 5, so about 0.44 GB for each thread beyond the tiles. The others: none seen.
    browser_per_helper: u64,
}

/// A deep-learning network's own need.
///
/// - Whole volume: `per_voxel` and `whole` bytes per voxel of the volume, plus `fixed`.
/// - Tiled (`inversion.tile_size`) in a browser: qsm-core's figure for one tile of this network
///   (`ModelSpec::tile_cost`, `inversion::tile_bytes`) times qsm-core's count of tiles in flight
///   (`inversion::wasm_tiles_in_flight`: the pool, up to what fits its tile budget), plus
///   `browser_per_voxel` over the volume, `fixed` and `once`, plus `browser_per_helper` for each
///   pool thread beyond the tiles in flight.
/// - Tiled natively: one tile per thread at `native_tile` bytes per patch voxel, plus
///   `per_voxel` and `fixed`.
///
/// Never more tiles in flight than there are tiles. The weights themselves are counted by the
/// host: read in the step natively, held for the whole run in a browser.
fn dl_need(s: &Scan, config: &PipelineConfig, net: Net) -> u64 {
    use qsm_core::inversion::{tile_bytes, wasm_tiles_in_flight, TileConfig};
    let cost = qsm_core::models::find_model(net.id).and_then(|m| m.tile_cost());
    match (config.dl_tile(), cost) {
        (Some((core, halo)), Some(cost)) => {
            let cfg = TileConfig { core, halo };
            let tiles: u64 = s.dims.iter().map(|&d| d.div_ceil(core.max(1)) as u64).product();
            match s.host {
                Host::Browser => {
                    // A browser's rayon pool has one worker more than the threads it is given
                    // (measured: xQSM and LPCNN on 4 threads hold five tiles), and qsm-core caps
                    // the tiles in flight at the pool.
                    let pool = s.threads as usize + usize::from(s.threads > 1);
                    let in_flight = (wasm_tiles_in_flight(&cfg, &cost, pool) as u64).min(tiles).max(1);
                    let helpers = (pool as u64).saturating_sub(in_flight);
                    s.v(net.browser_per_voxel) + net.fixed + net.once + in_flight * tile_bytes(&cfg, &cost)
                        + helpers * net.browser_per_helper
                }
                Host::Native => {
                    let patch = ((core + 2 * halo) as u64).saturating_pow(3).min(s.n);
                    s.v(net.per_voxel) + net.fixed + s.threads.min(tiles).max(1) * patch * net.native_tile
                }
            }
        }
        _ => s.v(net.per_voxel) + net.fixed + s.n * net.whole,
    }
}

/// Background-field removal: the total field and mask in, the local field and its mask out.
fn bg_removal_need(config: &PipelineConfig, s: &Scan) -> u64 {
    let n = s.n;
    match config.bg_removal.algorithm {
        // V-SHARP: the field's FFT and per-radius kernels, masks and filtered fields. A 1 mm
        // isotropic working grid (the default) adds the resampled copies plus V-SHARP on the
        // cropped working grid: 1.5x on the 1 mm example (0.66 -> 1.01 GB), 2.1x on UKB
        // 0.8x0.8x3 mm (0.37 -> 0.78 GB). The finest-voxel grid grows with the anisotropy ratio
        // (3.75x the voxels for UKB); budget for ratios up to ~4.
        BfAlgorithm::Vsharp => match config.bg_removal.vsharp.grid {
            crate::VsharpGrid::Native => 110 * n,
            crate::VsharpGrid::Isotropic => 210 * n,
            crate::VsharpGrid::Finest => 440 * n,
        },
        // PDF: every echo's magnitude (8 B per voxel per echo) and the noise map made from it,
        // on the acquired grid, and the solve on the mask's bounding box padded by `n_pad` voxels
        // (the whole volume for 0), taken as the volume padded (2.13 GB natively on the example's
        // 5 echoes at the default 40, 1.75 GB at 0, 2.95 GB at 80).
        BfAlgorithm::Pdf => {
            let p = config.bg_removal.pdf.n_pad;
            (217 + 8 * s.echoes) * n + 34 * padded_voxels(s, [p, p, p])
        }
        BfAlgorithm::Lbv => 37 * n,
        BfAlgorithm::Ismv => 114 * n,
        BfAlgorithm::Sharp => 59 * n,
        BfAlgorithm::Resharp => 123 * n,
        // HARPERELLA and iHARPERELLA: zero padding adds copies on the padded grid (example, 16
        // voxels per side: 0.84 -> 1.97 GB and 0.93 -> 1.96 GB).
        BfAlgorithm::Harperella => {
            let pad = config.bg_removal.harperella.pad;
            139 * n + if pad == [0; 3] { 0 } else { 114 * padded_voxels(s, pad) }
        }
        BfAlgorithm::Iharperella => {
            let pad = config.bg_removal.iharperella.pad;
            155 * n + if pad == [0; 3] { 0 } else { 103 * padded_voxels(s, pad) }
        }
        // iQFM: from every echo's phase and magnitude; on the whole volume 4.2 GB in WASM on the
        // example. BFRnet on the whole volume: 10.9 GB natively on the example. Tiles measured
        // single-threaded in WASM only (0.85 and 1.04 GB on the example at 56 + 2×4).
        BfAlgorithm::Iqfm => dl_need(s, config, Net { id: "iqfm", per_voxel: 120, fixed: 0, whole: 583, native_tile: 473, browser_per_voxel: 120, once: 0, browser_per_helper: 0 }),
        BfAlgorithm::Bfrnet => dl_need(s, config, Net { id: "bfrnet", per_voxel: 40, fixed: 0, whole: 1_760, native_tile: 3_059, browser_per_voxel: 40, once: 264_000_000, browser_per_helper: 0 }),
    }
}

/// Voxels of the acquired grid zero-padded by `pad` voxels per side on each axis.
fn padded_voxels(s: &Scan, pad: [usize; 3]) -> u64 {
    (0..3).map(|a| (s.dims[a] + 2 * pad[a]) as u64).product()
}

/// The dipole inversion of the local field (or, for iQSM/iQSM+, of the echoes' phase).
fn inversion_need(config: &PipelineConfig, s: &Scan) -> u64 {
    let n = s.n;
    // The tileable networks: activations per patch voxel, measured on tiles of 64³ (56 + 2×4) on
    // the example in WASM and natively (xQSM and QSMnet also at 48³, which separates the parts).
    let dl = |net: Net| dl_need(s, config, net);
    match config.inversion.algorithm {
        QsmAlgorithm::Tkd => 74 * n,
        QsmAlgorithm::Tsvd => 74 * n,
        QsmAlgorithm::Tikhonov => 82 * n,
        QsmAlgorithm::Ilsqr => 242 * n,
        QsmAlgorithm::Star => star_need(config, s),
        QsmAlgorithm::Lsqr => 147 * n,
        QsmAlgorithm::Heidi => 194 * n,
        QsmAlgorithm::Tv => 147 * n,
        QsmAlgorithm::Nltv => 154 * n,
        QsmAlgorithm::Rts => 227 * n,
        // MEDI and TFI hold every echo's magnitude (8 B per voxel per echo) and the noise map and
        // edge mask made from it (2.58 and 2.64 GB natively on the example's 5 echoes).
        // MEDI's zero padding enlarges its grid at the end of each axis (16 voxels on each of the
        // example's axes: 2.58 -> 3.29 GB).
        QsmAlgorithm::Medi => {
            let z = config.inversion.medi.zeropad;
            (387 + 8 * s.echoes) * (0..3).map(|a| (s.dims[a] + z[a]) as u64).product::<u64>()
        }
        QsmAlgorithm::Tfi => (398 + 8 * s.echoes) * n,
        QsmAlgorithm::Tgv => s.v(TGV_PER_VOXEL),
        QsmAlgorithm::Qsmart => s.v(QSMART_PER_VOXEL),
        QsmAlgorithm::Ndi => 123 * n,
        QsmAlgorithm::Fansi => 298 * n,
        // FANSI-TGV: 5.7 GB natively on the example, more than the WASM heap holds.
        QsmAlgorithm::FansiTgv => 938 * n,
        QsmAlgorithm::L1qsm => 370 * n,
        QsmAlgorithm::Whqsm => 290 * n,
        QsmAlgorithm::Hdqsm => 290 * n,
        QsmAlgorithm::AmpPe => 345 * n,
        // On the whole volume each of these is more than the WASM heap holds on the example.
        QsmAlgorithm::Qsmnet => dl(Net { id: "qsmnet", per_voxel: 50, fixed: 820_000_000, whole: 563, native_tile: 563, browser_per_voxel: 50, once: 0, browser_per_helper: 0 }),
        QsmAlgorithm::QsmnetPlus => dl(Net { id: "qsmnet-plus", per_voxel: 50, fixed: 820_000_000, whole: 563, native_tile: 563, browser_per_voxel: 50, once: 0, browser_per_helper: 0 }),
        QsmAlgorithm::Xqsm => dl(Net { id: "xqsm", per_voxel: 53, fixed: 0, whole: 905, native_tile: 767, browser_per_voxel: 30, once: 0, browser_per_helper: 0 }),
        QsmAlgorithm::Nextqsm => dl(Net { id: "nextqsm", per_voxel: 40, fixed: 0, whole: 1_556, native_tile: 1_556, browser_per_voxel: 40, once: 0, browser_per_helper: 440_000_000 }),
        // LPCNN measured natively on 4 threads and in WASM on one (which separates its parts);
        // iR2QSM and MoDL-QSM in WASM only, taken to divide the same way.
        QsmAlgorithm::Ir2qsm => dl(Net { id: "ir2qsm", per_voxel: 17, fixed: 0, whole: WHOLE_VOLUME_AT_LEAST, native_tile: 1_180, browser_per_voxel: 17, once: 72_000_000, browser_per_helper: 0 }),
        QsmAlgorithm::Lpcnn => dl(Net { id: "lpcnn", per_voxel: 17, fixed: 0, whole: WHOLE_VOLUME_AT_LEAST, native_tile: 837, browser_per_voxel: 2, once: 0, browser_per_helper: 0 }),
        QsmAlgorithm::ModlQsm => dl(Net { id: "modl-qsm", per_voxel: 10, fixed: 0, whole: WHOLE_VOLUME_AT_LEAST, native_tile: 800, browser_per_voxel: 2, once: 0, browser_per_helper: 0 }),
        // Patch-based networks that take the whole volume (they patch it themselves).
        QsmAlgorithm::Autoqsm => 92 * n,
        QsmAlgorithm::Qsmgan => 35 * n,
        // iQSM/iQSM+ from every echo's phase and magnitude. On the whole volume: 4.2 GB in WASM
        // for iQSM on the example, and iQSM+ more than the heap holds. Tiles measured
        // single-threaded in WASM only (0.85 and 0.99 GB on the example at 56 + 2×4).
        QsmAlgorithm::Iqsm => dl(Net { id: "iqsm", per_voxel: 120, fixed: 0, whole: 583, native_tile: 473, browser_per_voxel: 120, once: 0, browser_per_helper: 0 }),
        QsmAlgorithm::IqsmPlus => dl(Net { id: "iqsm-plus", per_voxel: 120, fixed: 0, whole: 640, native_tile: 1_026, browser_per_voxel: 120, once: 0, browser_per_helper: 0 }),
    }
}

/// STAR-QSM: its complex working arrays are on the zero-padded grid (`inversion.star.pad` voxels
/// per side, an odd extent grown by one), the field, mask and result on the acquired one.
/// Measured natively on the example at its default padding (0.76 GB) and unpadded (0.55 GB),
/// which separates the two, the same on 1 and 8 threads.
fn star_need(config: &PipelineConfig, s: &Scan) -> u64 {
    let pad = config.inversion.star.pad;
    let padded: u64 = (0..3).map(|a| {
        let m = s.dims[a] + 2 * pad[a];
        (m + m % 2) as u64
    }).product();
    26 * s.n + 66 * padded
}

/// R2 and R2′: the EPG fit to a MESE, and/or R2′ predicted by R2PRIMEnet at its patch.
fn r2_need(step: &Step, config: &PipelineConfig, s: &Scan) -> u64 {
    // The fit holds the MESE's echoes and the GRE's beside its dictionary; with no MESE, R2 is
    // supplied or not made.
    let fit = if s.mese { s.v((2 * s.echoes * F64) + 120) } else { s.v(24) };
    if step.models.is_empty() {
        return fit;
    }
    let [x, y, z] = config.separation.r2primenet.patch;
    let patch = x as u64 * y as u64 * z as u64;
    fit.max(patch * R2PRIMENET_PER_PATCH_VOXEL + R2PRIMENET_FIXED + s.v(24))
}

/// Bytes per voxel of a network on the whole volume, for iR2QSM, LPCNN and MoDL-QSM: LPCNN's,
/// 4.5 GB natively on the example (the others are known only to need more than the WASM heap).
const WHOLE_VOLUME_AT_LEAST: u64 = 720;

/// R2PRIMEnet's activations per patch voxel and its parsed model: 830 B natively (measured at
/// 128×128×64 and 96×96×64, linear up to the authors' 192×192×128) and 1.1 KB in WASM.
const R2PRIMENET_PER_PATCH_VOXEL: u64 = 1_000;
const R2PRIMENET_FIXED: u64 = 225_000_000;

/// χ-separation: the maps it reads and the method's workspace, or a network at its patch.
fn separation_need(config: &PipelineConfig, s: &Scan) -> u64 {
    let maps = s.v(2 * s.echoes * F64 + 40);
    // χ-sep iLSQR's own QSM: an iLSQR inversion before the separation, its map held through it.
    let (qsm_solve, qsm_map) = if crate::stages::separation::reconstructs_qsm(config) {
        let mut ilsqr = config.clone();
        ilsqr.inversion.algorithm = QsmAlgorithm::Ilsqr;
        (inversion_need(&ilsqr, s), s.v(F64))
    } else {
        (0, 0)
    };
    // The χ-separation toolbox's methods. χ-sep iLSQR: at most 14.25 f64 per voxel of the padded
    // grid and 7.5 per acquired voxel, whatever the mask (QSM.rs#341's bound; measured 1.20 GB
    // heap, 1.34 GB resident, on 184×256×128 padded to 216×288×160). χ-sep MEDI: about 45 f64 per
    // padded voxel (QSM.rs's measurement: 2.7 GB on 184×256×128).
    let sep = &config.separation;
    let workspace = match sep.algorithm {
        SeparationAlgorithm::ChiSepIlsqr => F64 * (57 * padded_voxels(s, sep.chi_sep_ilsqr.pad) + 30 * s.n) / 4,
        SeparationAlgorithm::ChiSepMedi => 45 * F64 * padded_voxels(s, [0, 0, sep.chi_sep_medi.pad]),
        _ => s.v(80),
    };
    // DECOMPOSE given a QSM per echo (`qsm.per_echo`): the echoes' maps, read before they are
    // laid out voxel by voxel (counted in `maps`).
    let echo_maps = if sep.algorithm == SeparationAlgorithm::Decompose && crate::bridge::per_echo(config, s.echoes as usize) {
        s.v(s.echoes * F64)
    } else {
        0
    };
    match chi_separation_network_bytes(s.dims, &config.separation) {
        0 => maps + echo_maps + workspace.max(qsm_solve) + qsm_map,
        net => maps + net,
    }
}

/// HD-BET's peak for the network itself, in bytes, with a sliding window of `patch` (`None`: one
/// pass over the whole volume): tract's activations for one network input (~850 B per input
/// voxel, measured at 128×128×64 and 96×96×64) and the parsed model. A sliding window's input
/// is its patch. The whole volume's is the head's bounding box at 1 mm, padded to multiples of
/// 32×32×16, which does not depend on the acquired resolution (6.4–8.2 GB peak RSS measured on
/// 1 mm whole-head and 0.8×0.8×3 mm GRE, up to 160×256×256 voxels), so budget a 176×256×256
/// box. The scan's own working volumes come on top.
pub fn hd_bet_network_bytes(patch: Option<[usize; 3]>) -> u64 {
    const FULL_VOLUME_VOXELS: u64 = 256 * 256 * 176;
    let input_voxels = patch.map_or(FULL_VOLUME_VOXELS, |p| p.iter().map(|&d| d as u64).product());
    input_voxels * 850 + 130_000_000
}

/// RS2-Net runs at one fixed window, so its peak does not depend on the volume: 3.2 GB measured
/// in the browser on a mouse scan.
const RS2_NET_BYTES: u64 = 3_200_000_000;

/// MindGrab runs on the image conformed to 256³ at 1 mm whatever the acquisition, so its peak
/// does not depend on the volume: 7.3 GB measured on 256×288×48 GRE, with or without TTA (the
/// flipped pass runs after the first).
const MINDGRAB_BYTES: u64 = 7_500_000_000;

/// SynthSeg's peak in bytes on a `dims` volume (taken to be about 1 mm, the resolution SynthSeg
/// resamples to), with a sliding window of `patch` (`None`: the whole volume at once). The
/// network takes about 1 KB per voxel it sees at once, and the posteriors (one float per label
/// per voxel, blended and then cleaned up) about 200 B per padded voxel. Measured on a 184×256×128
/// scan with flip averaging: 6.8 GB on the whole volume, 3.25 GB with 128³ windows, 2.0 GB with
/// 96³.
pub fn synthseg_bytes(dims: [usize; 3], patch: Option<usize>) -> u64 {
    // qsm-core pads each axis to a multiple of 32, at least 128.
    let padded: [u64; 3] = std::array::from_fn(|a| (dims[a].div_ceil(32) * 32).max(128) as u64);
    let n_pad: u64 = padded.iter().product();
    let window: u64 = match patch {
        None => n_pad,
        Some(p) => {
            let p = (p.div_ceil(32) * 32) as u64;
            padded.iter().map(|&d| d.min(p)).product()
        }
    };
    window * 1000 + n_pad * 200
}

/// The peak of a χ-separation network on top of the maps it reads, in bytes, on a `dims`
/// volume: tract's activations for one patch, the parsed model, and the network's own padded
/// input channels and overlap accumulators. χ-sepnet runs its `patch`; SUSEP-Net runs its
/// `patch`, or with none the whole volume (padded to multiples of 8) in one pass. Other methods:
/// 0. Measured natively, single-threaded, on a 184×256×128 volume (peak RSS, the maps in and out
/// included): χ-sepnet 4.5 GB at its authors' 192×192×128 patch and 1.7 GB at 128×128×64;
/// SUSEP-Net 12.5 GB on the whole volume and 3.1 GB at 128×128×64; on 136×136×72, SUSEP-Net
/// 2.8 GB at 128×128×64 and 2.0 GB at 96×96×64.
pub fn chi_separation_network_bytes(dims: [usize; 3], config: &SeparationConfig) -> u64 {
    // (activation bytes per patch voxel, parsed model and fixed overhead, pooling multiple)
    let (patch, per_voxel, fixed, multiple) = match config.algorithm {
        SeparationAlgorithm::ChiSepNet => (Some(config.chi_sepnet.patch), CHI_SEPNET_PER_VOXEL, CHI_SEPNET_FIXED,
            qsm_core::separation::chisepnet::POOL_MULTIPLE),
        SeparationAlgorithm::SusepNet => (config.susep_net.patch, SUSEP_NET_PER_VOXEL, SUSEP_NET_FIXED,
            qsm_core::separation::susep_net::POOL_MULTIPLE),
        _ => return 0,
    };
    let round = |d: usize| d.div_ceil(multiple) * multiple;
    // The padded volume: each axis at least the patch (and, whole-volume, a multiple of 8).
    let padded: [u64; 3] = std::array::from_fn(|a| match patch {
        Some(p) => dims[a].max(p[a]) as u64,
        None => round(dims[a]) as u64,
    });
    let window: u64 = match patch {
        Some(p) => p.iter().map(|&d| d as u64).product(),
        None => padded.iter().product(),
    };
    // Three f32 input channels and three f64 accumulators over the padded volume.
    window * per_voxel + fixed + 36 * padded.iter().product::<u64>()
}

const CHI_SEPNET_PER_VOXEL: u64 = 760;
const CHI_SEPNET_FIXED: u64 = 350_000_000;
const SUSEP_NET_PER_VOXEL: u64 = 1_850;
const SUSEP_NET_FIXED: u64 = 800_000_000;

/// The settings that bring `step`'s memory down, by config path, for a front end to point at
/// when a run would not fit.
pub fn reducing_settings(step: &Step, config: &PipelineConfig) -> Vec<&'static str> {
    let nets = |sections: &[MaskSection]| sections.iter().any(|s| matches!(s.generator,
        MaskOp::HdBet { .. } | MaskOp::Rs2Net { .. } | MaskOp::MindGrab { .. }));
    match step.stage {
        Stage::Mask(crate::plan::Pass::Main) if nets(&config.masking.sections) => vec!["masking.sections"],
        Stage::Mask(crate::plan::Pass::Reliable) => vec!["masking.two_pass_sections", "masking.two_pass_combine", "masking.two_pass"],
        Stage::FieldMap => vec!["field_mapping.unwrapping_algorithm", "field_mapping.phase_offset_removal"],
        Stage::BackgroundRemoval(_) if config.bg_removal.algorithm == BfAlgorithm::Vsharp =>
            vec!["bg_removal.vsharp.grid", "bg_removal.algorithm"],
        Stage::BackgroundRemoval(_) => vec!["bg_removal.algorithm"],
        Stage::Inversion(_) if !step.models.is_empty() => vec!["inversion.tile_size", "inversion.algorithm"],
        Stage::Inversion(_) | Stage::Tgv(_) | Stage::Qsmart(_) => vec!["inversion.algorithm"],
        Stage::Segmentation => vec!["segmentation.patch"],
        Stage::R2 if !step.models.is_empty() => vec!["separation.r2primenet.patch"],
        Stage::ChiSeparation => match config.separation.algorithm {
            SeparationAlgorithm::ChiSepNet => vec!["separation.chi_sepnet.patch", "separation.algorithm"],
            SeparationAlgorithm::SusepNet => vec!["separation.susep_net.patch", "separation.algorithm"],
            SeparationAlgorithm::ChiSepIlsqr => vec!["separation.chi_sep_ilsqr.pad", "separation.algorithm"],
            SeparationAlgorithm::ChiSepMedi => vec!["separation.chi_sep_medi.pad", "separation.algorithm"],
            _ => vec!["separation.algorithm"],
        },
        _ => vec![],
    }
}

/// Estimate peak memory usage in bytes for a single `qsmxt run` of `config` on a scan of
/// `nx`×`ny`×`nz` voxels: the peak of [`estimate`] over the run's plan, natively, its volumes in
/// files ([`MemoryProfile::NATIVE`]).
///
/// * `n_coils` — Uncombined receive-coil channels (1 for combined data); MCPC-3D-S loads every
///   coil's phase and magnitude at once before the rest of the pipeline runs
/// * `has_magnitude` — Whether magnitude files are available
pub fn estimate_peak_memory_bytes(
    nx: usize,
    ny: usize,
    nz: usize,
    n_echoes: usize,
    n_coils: usize,
    has_magnitude: bool,
    config: &PipelineConfig,
) -> u64 {
    let n_echoes = n_echoes.max(1);
    let facts = ScanFacts {
        dims: (nx, ny, nz),
        voxel_size: None,
        n_echoes,
        echo_times: (1..=n_echoes).map(|e| 0.005 * e as f64).collect(),
        has_magnitude,
        n_coils: n_coils.max(1),
        real_imaginary: false,
        field_strength: 3.0,
        b0_dir: None,
        mese_echo_times: None,
        custom_mask: false, custom_dseg: false, custom_qsm: false, custom_r2: false, custom_r2prime: false,
    };
    crate::plan::plan(config, &facts).peak_memory_bytes
}

/// Format a byte count as a human-readable string (e.g. "3.4 GB").
pub fn format_bytes(bytes: u64) -> String {
    let gb = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    if gb >= 1.0 {
        format!("{:.1} GB", gb)
    } else {
        let mb = bytes as f64 / (1024.0 * 1024.0);
        format!("{:.0} MB", mb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{plan, plan_for};

    /// The example data: 184×256×128, 5 echoes, 3 T, with magnitude.
    pub(crate) fn example() -> ScanFacts {
        ScanFacts {
            dims: (184, 256, 128),
            voxel_size: None,
            n_echoes: 5,
            echo_times: vec![0.005, 0.011, 0.017, 0.023, 0.029],
            has_magnitude: true,
            n_coils: 1,
            real_imaginary: false,
            field_strength: 3.0,
            b0_dir: None,
            mese_echo_times: None,
            custom_mask: false, custom_dseg: false, custom_qsm: false, custom_r2: false, custom_r2prime: false,
        }
    }

    fn with(edit: impl FnOnce(&mut PipelineConfig)) -> PipelineConfig {
        let mut c = PipelineConfig::default();
        edit(&mut c);
        c
    }

    fn step_bytes(p: &Plan, name: &str) -> u64 {
        p.steps.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no {name}")).memory_bytes
    }

    /// χ-sep iLSQR's step reconstructs its QSM with iLSQR, so it needs at least the iLSQR
    /// inversion's working memory; separating a supplied QSM does not.
    #[test]
    fn chi_sep_ilsqr_counts_the_ilsqr_of_its_own_qsm() {
        let mut f = example();
        f.mese_echo_times = Some(vec![0.01, 0.02, 0.03, 0.04]);
        let sep = |c: &PipelineConfig| step_bytes(&plan(c, &f), "chi_separation");
        let own = with(|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr; });
        let ilsqr = step_bytes(&plan(&with(|c| c.inversion.algorithm = QsmAlgorithm::Ilsqr), &f), "invert");
        assert!(sep(&own) >= ilsqr, "{} < {ilsqr}", sep(&own));
        let supplied = with(|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr;
                                  c.separation.custom_qsm_tool = Some("qsmxt".into()); });
        assert!(sep(&supplied) < sep(&own));
    }

    /// A per-echo QSM's background removal and inversion run one echo at a time, as a single-echo
    /// scan's; its referencing also holds every echo's map, and DECOMPOSE the echoes' maps.
    #[test]
    fn a_per_echo_qsm_is_estimated_one_echo_at_a_time() {
        let f = example();
        let medi = with(|c| { c.inversion.algorithm = QsmAlgorithm::Medi; c.bg_removal.algorithm = BfAlgorithm::Pdf; });
        let mut per_echo = medi.clone();
        per_echo.qsm.per_echo = true;
        let (a, b) = (plan(&medi, &f), plan(&per_echo, &f));
        let n = 184 * 256 * 128u64;
        for step in ["bgremove", "invert"] {
            assert_eq!(step_bytes(&a, step) - step_bytes(&b, step), (4 * 8 * n) * 13 / 10, "{step}: four echoes' magnitudes fewer");
        }
        assert!(step_bytes(&b, "unwrap") < step_bytes(&a, "unwrap"));
        assert!(step_bytes(&b, "reference") > step_bytes(&a, "reference"));
        let decompose = |c: &PipelineConfig| { let mut c = c.clone(); c.pipeline.do_chi_separation = true;
            c.separation.algorithm = SeparationAlgorithm::Decompose; step_bytes(&plan(&c, &f), "chi_separation") };
        assert!(decompose(&per_echo) > decompose(&medi));
    }

    /// χ-sep iLSQR's workspace is QSM.rs#341's bound: 14.25 f64 per padded voxel and 7.5 per
    /// acquired voxel, about 1.5 GB on the example.
    #[test]
    fn chi_sep_ilsqr_takes_its_bound() {
        let c = with(|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr;
                           c.separation.custom_qsm_tool = Some("qsmxt".into()); });
        let (n, padded) = (184 * 256 * 128u64, 216 * 288 * 160u64);
        let workspace = 8 * (57 * padded + 30 * n) / 4;
        assert!(workspace > 1_400_000_000 && workspace < 1_600_000_000, "{workspace}");
        let maps = (2 * 5 * 8 + 40) * n;
        assert_eq!(step_bytes(&plan(&c, &example()), "chi_separation"), (maps + workspace) * 13 / 10);
    }

    /// Every step that computes has an estimate, the plan's peak is the largest of them, and the
    /// plan names the step that sets it.
    #[test]
    fn every_step_has_an_estimate_and_the_plan_names_the_peak() {
        let configs = [
            PipelineConfig::default(),
            with(|c| { c.masking.two_pass = true; c.inversion.algorithm = QsmAlgorithm::Tgv; }),
            with(|c| { c.pipeline.do_swi = true; c.pipeline.do_smwi = true; c.pipeline.do_t2starmap = true;
                       c.pipeline.do_analysis = true; c.motion_correction.enabled = true; }),
            with(|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = SeparationAlgorithm::SusepNet; }),
            with(|c| { c.inversion.algorithm = QsmAlgorithm::Qsmart; c.masking.sections = crate::hd_bet_mask_sections(); }),
            with(|c| c.inversion.algorithm = QsmAlgorithm::Iqsm),
        ];
        for config in &configs {
            for profile in [MemoryProfile::NATIVE, MemoryProfile::browser(4),
                            MemoryProfile { store: StoreKind::Memory, ..MemoryProfile::NATIVE }] {
                let p = plan_for(config, &example(), &profile);
                for s in &p.steps {
                    assert!(s.stage == Stage::Load || s.memory_bytes > 0, "{} has no estimate", s.name);
                }
                let peak = p.steps.iter().map(|s| s.memory_bytes).max().unwrap();
                assert_eq!(p.peak_memory_bytes, peak);
                let named = p.peak_memory_step.as_deref().unwrap();
                assert_eq!(step_bytes(&p, named), peak);
                assert_eq!(p.memory_profile, profile);
            }
        }
        // Nothing to compute, no peak step.
        let p = plan(&with(|c| c.pipeline.do_qsm = false), &example());
        assert_eq!(p.peak_memory_step, None);
    }

    /// `qsmxt run`'s estimate is the native plan's peak, from the same per-step figures.
    #[test]
    fn the_native_estimate_is_the_native_plans_peak() {
        for config in [PipelineConfig::default(), with(|c| c.inversion.algorithm = QsmAlgorithm::Tgv)] {
            let f = example();
            let p = plan(&config, &f);
            assert_eq!(estimate_peak_memory_bytes(184, 256, 128, 5, 1, true, &config), p.peak_memory_bytes);
            assert_eq!(p.memory_profile, MemoryProfile::NATIVE);
            assert_eq!((p.memory_budget_bytes, &p.over_budget), (None, &None), "no hard limit natively");
        }
    }

    /// With the volumes held in the same memory, each step also holds everything read or written
    /// before it; with them outside, a step holds only its own.
    #[test]
    fn a_store_in_memory_adds_what_it_holds_before_each_step() {
        let config = PipelineConfig::default();
        let disk = plan_for(&config, &example(), &MemoryProfile::NATIVE);
        let memory = plan_for(&config, &example(), &MemoryProfile { store: StoreKind::Memory, ..MemoryProfile::NATIVE });
        let n = 184 * 256 * 128u64;
        let held: Vec<u64> = disk.steps.iter().zip(&memory.steps).map(|(d, m)| m.memory_bytes - d.memory_bytes).collect();
        // Before the first step: the 5 echoes' phase and magnitude, in double precision.
        assert_eq!(held[0], 10 * 8 * n);
        // Each step adds its outputs: the combined magnitude after `magnitude`, then the mask.
        let after = |name: &str| held[disk.steps.iter().position(|s| s.name == name).unwrap() + 1];
        assert_eq!(after("scale_phase") - after("load"), 10 * 8 * n, "every echo's phase and magnitude on the working grid");
        assert_eq!(after("magnitude") - after("scale_phase"), 8 * n);
        assert_eq!(after("mask") - after("magnitude"), n, "a mask is a byte a voxel");
        assert!(held.windows(2).all(|w| w[0] <= w[1]));
    }

    /// A browser's heap holds the run's weights from the start, so every step carries them.
    #[test]
    fn a_browser_heap_holds_the_weights_for_the_whole_run() {
        let config = with(|c| { c.apply_browser_profile(); c.inversion.algorithm = QsmAlgorithm::Xqsm; });
        let tkd = with(|c| { c.apply_browser_profile(); c.inversion.algorithm = QsmAlgorithm::Tkd; });
        let (a, b) = (plan_for(&config, &example(), &MemoryProfile::browser(4)),
                      plan_for(&tkd, &example(), &MemoryProfile::browser(4)));
        let xqsm_weights = model_bytes("xqsm");
        assert!(xqsm_weights > 20_000_000);
        assert_eq!(step_bytes(&a, "magnitude") - step_bytes(&b, "magnitude"), xqsm_weights);
        assert_eq!(a.memory_budget_bytes, Some(WASM32_HEAP_BYTES));
    }

    /// A browser run that would not fit is warned of before it starts: the plan names the step
    /// and the settings that bring it down, in neutral words, and the run is not refused.
    #[test]
    fn a_browser_run_over_the_heap_names_the_step_and_what_reduces_it() {
        // SUSEP-Net on the whole volume, as natively; everything else as the browser runs it.
        let config = with(|c| {
            c.apply_browser_profile();
            c.pipeline.do_chi_separation = true;
            c.separation.algorithm = SeparationAlgorithm::SusepNet;
            c.separation.susep_net.patch = None;
        });
        let p = plan_for(&config, &example(), &MemoryProfile::browser(4));
        let over = p.over_budget.as_ref().expect("over the heap");
        assert_eq!(over.step, "chi_separation");
        assert_eq!(over.budget_bytes, WASM32_HEAP_BYTES);
        assert!(over.bytes > WASM32_HEAP_BYTES);
        assert_eq!(over.settings, ["separation.susep_net.patch", "separation.algorithm"]);
        let warning = p.check.warnings.last().unwrap();
        assert!(warning.starts_with("χ-separation is estimated to need ") && warning.ends_with("more than the 4.0 GB a browser run has"),
                "{warning}");
        let problem = p.check.problems.last().unwrap();
        assert_eq!((problem.severity, problem.setting), (crate::requirements::Severity::Warning, None));
        assert_eq!((&problem.reason, &problem.settings), (warning, &over.settings));
        assert!(p.check.errors.is_empty(), "a warning, not a refusal: {:?}", p.check.errors);
        // Natively the same plan has no limit to warn of.
        let native = plan(&config, &example());
        assert!(native.over_budget.is_none());
        assert!(!native.check.warnings.iter().any(|w| w.contains("browser run")), "{:?}", native.check.warnings);
        // The browser profile's patch fits.
        let mut fitted = config.clone();
        fitted.separation.susep_net.patch = None;
        fitted.apply_browser_profile();
        assert!(plan_for(&fitted, &example(), &MemoryProfile::browser(4)).over_budget.is_none());
        // Natively the whole run is planned; R2PRIMEnet at its authors' patch is first to
        // exceed the heap when nothing is changed for the browser.
        let unprofiled = with(|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = SeparationAlgorithm::SusepNet; });
        assert_eq!(plan_for(&unprofiled, &example(), &MemoryProfile::browser(4)).over_budget.unwrap().step, "r2_r2prime");
    }

    /// With the browser profile, every step of every option fits the heap on the example data
    /// (#358): each inversion, background removal, unwrapping and mask network, SynthSeg,
    /// R2PRIMEnet and both χ-separation networks.
    #[test]
    fn the_browser_profile_fits_every_option_in_the_heap() {
        let mut configs: Vec<(String, PipelineConfig)> = Vec::new();
        // FANSI-TGV holds 5.7 GB on the example: it does not fit the heap.
        let too_big = [QsmAlgorithm::FansiTgv];
        for a in QsmAlgorithm::ALL.into_iter().filter(|a| !too_big.contains(a)) {
            configs.push((format!("{a}"), with(|c| c.inversion.algorithm = a)));
        }
        for b in [BfAlgorithm::Bfrnet, BfAlgorithm::Iqfm, BfAlgorithm::Vsharp, BfAlgorithm::Pdf, BfAlgorithm::Lbv, BfAlgorithm::Ismv, BfAlgorithm::Sharp,
                  BfAlgorithm::Resharp, BfAlgorithm::Harperella, BfAlgorithm::Iharperella] {
            configs.push((format!("{b}"), with(|c| c.bg_removal.algorithm = b)));
        }
        configs.push(("laplacian".into(), with(|c| c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian)));
        configs.push(("hd-bet".into(), with(|c| c.masking.sections = crate::hd_bet_mask_sections())));
        configs.push(("everything".into(), with(|c| {
            c.pipeline.do_swi = true; c.pipeline.do_smwi = true; c.pipeline.do_t2starmap = true;
            c.pipeline.do_analysis = true; c.motion_correction.enabled = true; c.masking.two_pass = true;
        })));
        for s in [SeparationAlgorithm::ChiSepNet, SeparationAlgorithm::SusepNet, SeparationAlgorithm::ChiSepIlsqr] {
            configs.push((format!("{s}"), with(|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = s; })));
        }
        let mut over = Vec::new();
        for (name, mut config) in configs {
            config.apply_browser_profile();
            // qsmbly runs a plan with networks on its deep-learning bundle, on 4 threads; the
            // classical bundle on as many as the device has.
            let threads: &[usize] = if plan(&config, &example()).models.is_empty() { &[1, 4, 16] } else { &[1, 4] };
            for &threads in threads {
                let p = plan_for(&config, &example(), &MemoryProfile::browser(threads));
                if let Some(o) = &p.over_budget {
                    over.push(format!("{name} on {threads} threads: {} {}", o.step, format_bytes(o.bytes)));
                }
            }
        }
        assert!(over.is_empty(), "{over:#?}");
    }

    /// The figures follow the settings that set them: patches and tiles bound the networks, more
    /// echoes and threads take more, and a lighter algorithm takes less.
    #[test]
    fn the_settings_move_the_figures_the_right_way() {
        let f = example();
        let at = |c: &PipelineConfig, step: &str, threads: usize| step_bytes(&plan_for(c, &f, &MemoryProfile { threads, ..MemoryProfile::NATIVE }), step);

        // A tile bounds a network's activations; tiles run one per thread.
        let xqsm = with(|c| c.inversion.algorithm = QsmAlgorithm::Xqsm);
        let tiled = with(|c| { c.inversion.algorithm = QsmAlgorithm::Xqsm; c.inversion.tile_size = Some(56); c.inversion.tile_halo = Some(4); });
        let small = with(|c| { c.inversion.algorithm = QsmAlgorithm::Xqsm; c.inversion.tile_size = Some(40); c.inversion.tile_halo = Some(4); });
        assert!(at(&xqsm, "invert", 4) > at(&tiled, "invert", 4));
        assert!(at(&tiled, "invert", 4) > at(&small, "invert", 4));
        assert!(at(&tiled, "invert", 4) > at(&tiled, "invert", 1));

        // STAR-QSM's padding enlarges its working grid.
        let star = |pad| with(|c| { c.inversion.algorithm = QsmAlgorithm::Star; c.inversion.star.pad = pad; });
        assert!(at(&star([8, 8, 20]), "invert", 1) > at(&star([0, 0, 0]), "invert", 1));

        // HD-BET: the whole volume, the native window, the low-memory one.
        let hd = |patch| with(|c| { c.masking.sections = crate::hd_bet_mask_sections();
            c.masking.sections[0].generator = MaskOp::HdBet { patch, tta: false, tile_step: hd_bet_default_tile_step() }; });
        let (whole, native, low) = (at(&hd(None), "mask", 1), at(&hd(Some(MaskOp::hd_bet_default_patch())), "mask", 1),
                                    at(&hd(Some([128, 128, 64])), "mask", 1));
        assert!(whole > native && native > low, "{whole} {native} {low}");
        assert!(at(&with(|c| c.masking.sections = crate::mindgrab_mask_sections()), "mask", 1) > 7_300_000_000);

        // HARPERELLA's, iHARPERELLA's and PDF's padding enlarge their working grid.
        let harp = |pad| with(|c| { c.bg_removal.algorithm = BfAlgorithm::Harperella; c.bg_removal.harperella.pad = pad; });
        assert!(at(&harp([16; 3]), "bgremove", 1) > at(&harp([0; 3]), "bgremove", 1));
        let iharp = |pad| with(|c| { c.bg_removal.algorithm = BfAlgorithm::Iharperella; c.bg_removal.iharperella.pad = pad; });
        assert!(at(&iharp([16; 3]), "bgremove", 1) > at(&iharp([0; 3]), "bgremove", 1));
        let pdf = |n_pad| with(|c| { c.bg_removal.algorithm = BfAlgorithm::Pdf; c.bg_removal.pdf.n_pad = n_pad; });
        assert!(at(&pdf(80), "bgremove", 1) > at(&pdf(40), "bgremove", 1));

        // SynthSeg's window, the χ-separation and R2PRIMEnet patches.
        let seg = |patch| with(|c| { c.pipeline.do_segmentation = true; c.segmentation.patch = patch; });
        assert!(at(&seg(None), "segmentation", 1) > at(&seg(Some(128)), "segmentation", 1));
        assert!(at(&seg(Some(128)), "segmentation", 1) > at(&seg(Some(96)), "segmentation", 1));
        for alg in [SeparationAlgorithm::ChiSepNet, SeparationAlgorithm::SusepNet] {
            let mut c = with(|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = alg; });
            let native = at(&c, "chi_separation", 1);
            c.apply_browser_profile();
            assert!(at(&c, "chi_separation", 1) < native, "{alg}");
        }
        let r2 = |patch| with(|c| { c.pipeline.do_r2primemap = true; c.separation.r2primenet.patch = patch; });
        assert!(at(&r2([192, 192, 128]), "r2_r2prime", 1) > at(&r2([128, 128, 64]), "r2_r2prime", 1));

        // Unwrapping: more echoes and, natively, more threads; Laplacian below ROMEO.
        let d = PipelineConfig::default();
        let mut three = example();
        three.n_echoes = 3;
        three.echo_times.truncate(3);
        let unwrap = |f: &ScanFacts, c: &PipelineConfig, t| step_bytes(&plan_for(c, f, &MemoryProfile { threads: t, ..MemoryProfile::NATIVE }), "unwrap");
        assert!(unwrap(&f, &d, 1) > unwrap(&three, &d, 1));
        assert!(unwrap(&f, &d, 8) > unwrap(&f, &d, 1));
        assert!(unwrap(&f, &with(|c| c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian), 1) < unwrap(&f, &d, 1));
        let laplacian = with(|c| c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian);
        let mut mirror = laplacian.clone();
        mirror.field_mapping.laplacian_solver = LaplacianSolver::Mirror;
        assert!(unwrap(&f, &mirror, 1) > unwrap(&f, &laplacian, 1));
        // MEDI's complex fit: more echoes, not more threads; MEDI's zero padding enlarges its grid.
        let cf = with(|c| c.field_mapping.b0_estimation = B0Estimation::ComplexFit);
        assert!(unwrap(&f, &cf, 1) > unwrap(&three, &cf, 1) && unwrap(&f, &cf, 8) == unwrap(&f, &cf, 1));
        let medi = |z| with(|c| { c.inversion.algorithm = QsmAlgorithm::Medi; c.inversion.medi.zeropad = z; });
        assert!(at(&medi([16; 3]), "invert", 1) > at(&medi([0; 3]), "invert", 1));

        // Lighter algorithms take less.
        assert!(at(&with(|c| c.inversion.algorithm = QsmAlgorithm::Tkd), "invert", 1) < at(&with(|c| c.inversion.algorithm = QsmAlgorithm::Rts), "invert", 1));
        assert!(at(&with(|c| c.bg_removal.algorithm = BfAlgorithm::Lbv), "bgremove", 1) < at(&d, "bgremove", 1));

        // Uncombined coils are combined with every coil held at once.
        let mut coils = example();
        coils.n_coils = 32;
        let scale = |f: &ScanFacts| step_bytes(&plan(&d, f), "scale_phase");
        assert!(scale(&coils) > scale(&f) + 32 * 5 * 2 * 8 * 184 * 256 * 128);

        // Without magnitude there is no combined magnitude to compute.
        let mut phase_only = example();
        phase_only.has_magnitude = false;
        assert!(!plan(&d, &phase_only).has(Stage::Magnitude));
    }

    /// A small minimal run stays small, and a typical brain lands in the gigabytes.
    #[test]
    fn small_runs_stay_small_and_a_typical_brain_takes_gigabytes() {
        let minimal = with(|c| { c.inversion.algorithm = QsmAlgorithm::Tkd; c.bg_removal.algorithm = BfAlgorithm::Lbv; });
        assert!(estimate_peak_memory_bytes(64, 64, 64, 1, 1, false, &minimal) < GIB);
        let typical = estimate_peak_memory_bytes(256, 256, 176, 4, 1, true, &PipelineConfig::default());
        assert!(typical > GIB && typical < 10 * GIB, "{}", format_bytes(typical));
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024 + 512 * 1024 * 1024), "3.5 GB");
        assert_eq!(format_bytes(500 * 1024 * 1024), "500 MB");
    }
}
