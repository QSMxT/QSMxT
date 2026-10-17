use std::path::{Path, PathBuf};
use std::time::Instant;

use glob::glob;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use qsm_core::io::{self, NiftiData};
use serde::Serialize;

use crate::bids::derivatives::DerivativeOutputs;
use crate::bids::discovery::QsmRun;
use crate::pipeline::config::*;
use crate::pipeline::graph::{PipelineState, RunMetadata};
use qsmxt_engine::observe::{Observer, StageEvent};
use qsmxt_engine::plan::{Pass as EnginePass, Stage};
use qsmxt_engine::record::Decision;
use qsmxt_engine::store::{Completed, Contrast, Data, Header, Item, StageStore, Supplied};
use crate::pipeline::memory;
use crate::pipeline::requirements;
use crate::pipeline::stats;
use crate::nifti::write::write_volume;
use crate::error::QsmxtError;

/// Provenance record written to each workflow step directory.
#[derive(Serialize)]
struct Provenance {
    step: String,
    algorithm: Option<String>,
    parameters: serde_json::Value,
    inputs: Vec<String>,
    outputs: Vec<String>,
    duration_secs: f64,
    peak_memory_bytes: u64,
    timestamp: String,
}

/// Write one step's `provenance.json` — what BIDS-Prov is assembled from after the run (see
/// [`crate::bids::bidsprov`]). Every step that writes a file anywhere in the derivatives has to
/// call this, under a name listed in [`crate::bids::bidsprov::STEPS`] (or `GROUP_STEPS`).
pub(crate) fn write_step_provenance(
    prov_path: &Path,
    step: &str,
    algorithm: Option<&str>,
    parameters: &serde_json::Value,
    inputs: &[&Path],
    outputs: &[PathBuf],
    start: Instant,
) -> crate::Result<()> {
    let prov = Provenance {
        step: step.to_string(),
        algorithm: algorithm.map(|s| s.to_string()),
        parameters: parameters.clone(),
        inputs: inputs.iter().map(|p| p.display().to_string()).collect(),
        outputs: outputs.iter().map(|p| p.display().to_string()).collect(),
        duration_secs: start.elapsed().as_secs_f64(),
        peak_memory_bytes: memory::process_rss_bytes(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };
    if let Some(parent) = prov_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&prov)
        .map_err(|e| QsmxtError::Config(format!("Failed to serialize provenance: {}", e)))?;
    std::fs::write(prov_path, json)?;
    Ok(())
}

/// Global multi-progress for coordinating parallel progress bars.
pub static MULTI_PROGRESS: std::sync::LazyLock<MultiProgress> =
    std::sync::LazyLock::new(MultiProgress::new);

/// Create an indicatif progress bar for iterative algorithms.
fn create_progress_bar(label: &str, total: u64) -> ProgressBar {
    let pb = MULTI_PROGRESS.add(ProgressBar::new(total));
    pb.set_style(
        ProgressStyle::with_template(&format!(
            "  {{spinner:.green}} {} [{{bar:30.cyan/dim}}] {{pos}}/{{len}} ({{percent}}%) | {{elapsed_precise}} elapsed | Mem: {{msg}}",
            label
        ))
        .unwrap()
        .progress_chars("━╸─"),
    );
    pb.set_message("...");
    pb
}

/// Create a progress callback that drives an indicatif progress bar.
#[allow(clippy::type_complexity)]
fn iter_progress_bar(run_key: &str, step_name: &str) -> (Box<dyn FnMut(usize, usize)>, Option<ProgressBar>) {
    let pb: std::cell::RefCell<Option<ProgressBar>> = std::cell::RefCell::new(None);
    let name = format!("{} {}", run_key, step_name);
    let cb = Box::new(move |current: usize, total: usize| {
        let mut pb_ref = pb.borrow_mut();
        if pb_ref.is_none() && total > 0 {
            *pb_ref = Some(create_progress_bar(&name, total as u64));
        }
        let finished = current == total;
        if let Some(ref bar) = *pb_ref {
            bar.set_position(current as u64);
            // Update memory info occasionally (reading /proc is cheap but not free)
            if current == 1 || finished || current.is_multiple_of(10) {
                let rss = memory::process_rss_bytes();
                if rss > 0 {
                    bar.set_message(memory::format_bytes(rss));
                }
            }
            if finished {
                bar.finish_and_clear();
            }
        }
        // Drop the bar after finishing so MultiProgress reclaims the slot
        if finished {
            *pb_ref = None;
        }
    });
    (cb, None)
}

/// Create a byte-oriented progress bar for weight downloads, in the same visual style as
/// [`create_progress_bar`] (spinner + `━╸─` bar) but showing size + transfer rate.
#[cfg(feature = "dl")]
fn create_download_bar(label: &str, total: u64) -> ProgressBar {
    let pb = MULTI_PROGRESS.add(ProgressBar::new(total));
    pb.set_style(
        ProgressStyle::with_template(&format!(
            "  {{spinner:.green}} {} [{{bar:30.cyan/dim}}] {{bytes}}/{{total_bytes}} ({{percent}}%) | {{elapsed_precise}} elapsed | {{bytes_per_sec}}",
            label
        ))
        .unwrap()
        .progress_chars("━╸─"),
    );
    pb
}

/// Pre-fetch a deep-learning model's weights (if `model_id` names one) with a download
/// progress bar per weight file. No-op for classical algorithms (unknown id) or weights
/// already cached — after this, qsm-core's inference path finds the files in the cache.
/// `label` prefixes the bar (the pipeline passes the run key; standalone commands pass the
/// algorithm name). Shared by the pipeline runner and the standalone `invert`/`bgremove`/
/// `separate` commands.
#[cfg(feature = "dl")]
pub fn prefetch_weights(model_id: &str, label: &str) -> crate::Result<()> {
    use std::cell::RefCell;
    use std::collections::HashMap;
    let bars: RefCell<HashMap<String, ProgressBar>> = RefCell::new(HashMap::new());
    let res = {
        let mut cb = |name: &str, done: u64, total: u64| {
            let mut m = bars.borrow_mut();
            let bar = m
                .entry(name.to_string())
                .or_insert_with(|| create_download_bar(&format!("{} ↓ {}", label, name), total.max(1)));
            if total > 0 {
                bar.set_length(total);
            }
            bar.set_position(done);
            if total > 0 && done >= total {
                bar.finish_and_clear();
            }
        };
        qsm_core::models::prefetch_with_progress(model_id, &mut cb)
    };
    for (_, bar) in bars.into_inner() {
        bar.finish_and_clear();
    }
    res.map_err(|e| QsmxtError::Config(format!("weight download ({}): {}", model_id, e)))
}

/// Build without the `dl` feature (e.g. the Windows/ARM64 binary): there is no ONNX
/// inference or weight download. A classical algorithm (not a registry model) is a no-op;
/// selecting a deep-learning algorithm fails here with a clear, actionable message instead
/// of a confusing lower-level error.
#[cfg(not(feature = "dl"))]
pub fn prefetch_weights(model_id: &str, _label: &str) -> crate::Result<()> {
    if qsm_core::models::find_model(model_id).is_some() {
        return Err(QsmxtError::Config(format!(
            "'{model_id}' is a deep-learning algorithm, which this build of qsmxt does not \
             include (compiled without the 'dl' feature — e.g. the Windows/ARM64 binary). \
             Pick a classical algorithm, or install a qsmxt build with deep-learning support.",
        )));
    }
    Ok(())
}

/// Log step completion with timing.
fn log_step_done(step_name: &str, start: Instant) {
    let elapsed = start.elapsed();
    let secs = elapsed.as_secs_f64();
    let rss = memory::process_rss_bytes();
    if rss > 0 {
        log::info!(
            "{} complete ({:.1}s, Mem: {})",
            step_name, secs, memory::format_bytes(rss),
        );
    } else {
        log::info!("{} complete ({:.1}s)", step_name, secs);
    }
}

/// Helper: save a f64 volume to NIfTI using metadata from RunMetadata.
fn save_volume(path: &Path, data: &[f64], meta: &RunMetadata) -> crate::Result<()> {
    write_volume(path, data, meta.dims, meta.voxel_size, &meta.affine)
}

/// Helper: save a u8 mask as f64 NIfTI.
fn save_mask(path: &Path, mask: &[u8], meta: &RunMetadata) -> crate::Result<()> {
    let data: Vec<f64> = mask.iter().map(|&m| m as f64).collect();
    save_volume(path, &data, meta)
}

/// Helper: load a f64 volume from NIfTI.
fn load_volume(path: &Path) -> crate::Result<Vec<f64>> {
    let nifti = io::read_nifti_file(path)
        .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", path.display(), e)))?;
    Ok(nifti.data)
}

/// Helper: load a u8 mask from NIfTI.
fn load_mask(path: &Path) -> crate::Result<Vec<u8>> {
    let data = load_volume(path)?;
    Ok(data.iter().map(|&v| if v > 0.5 { 1u8 } else { 0u8 }).collect())
}

/// Execute the QSM pipeline with disk caching and auto-resume.
///
/// The engine plans the run and runs it ([`qsmxt_engine::run::run`]) against the run's files
/// ([`FileStore`]): each step saves its output to disk and drops data from memory, and on re-run,
/// completed steps with valid outputs on disk are skipped.
pub fn run_pipeline_cached(
    qsm_run: &QsmRun,
    config: &PipelineConfig,
    output: &DerivativeOutputs,
    force: bool,
    progress: &dyn Fn(&str),
) -> crate::Result<()> {
    let state_path = output.state_path(&qsm_run.key);
    let state = PipelineState::load_or_create(&state_path, config, &qsm_run.key, force);

    // What this run can produce of what was enabled, and the steps that produce it. `qsmxt run`
    // has already refused a requested output the data cannot give and warned about an implied one
    // (`requirements::check_runs`); the plan follows the same rules, so the two cannot disagree.
    let facts = requirements::scan_facts(qsm_run, config);
    let plan = qsmxt_engine::plan::plan(config, &facts);
    for note in &plan.notes {
        log::warn!("{note}");
    }
    let mut store = FileStore {
        run: qsm_run, config, output: output.clone(), meta: None, state, state_path,
        started: None, step: None, supplied_mask: None, extra_outputs: Vec::new(), metadata: None,
        working_grid_kept: false,
    };
    let mut observer = RunObserver { key: qsm_run.key.to_string(), progress, bar: None, started: None };
    let record = qsmxt_engine::run::run(&plan, config, &facts, &mut store, &mut observer)?;
    write_run_files(qsm_run, config, &facts, &plan, &record, output)
}

/// A run's `*_runrecord.json`: what it did ([`qsmxt_engine::record::RunRecord`], its fields at
/// the top level) and the scan facts it was planned from (`scan`), so the dataset's methods can
/// be made from the file alone, whichever invocation processed the run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct RunRecordFile {
    #[serde(flatten)]
    pub record: qsmxt_engine::record::RunRecord,
    pub scan: qsmxt_engine::scan::ScanFacts,
}

/// What a run did, in its workflow directory ([`qsmxt_engine::dataset::summarise_run`]): its
/// methods text, from the plan and what the run decided; the record itself
/// ([`qsmxt_engine::record::RunRecord`]); the planned pipeline graph; and the graph as it ran, only
/// when that differs from the planned one. Each graph is written as Mermaid and as SVG.
///
/// Whether the run differs from the dataset (`changes/`) is decided once every run is done, from
/// all their records (`qsmxt run`'s dataset-level steps).
fn write_run_files(
    run: &QsmRun, config: &PipelineConfig, facts: &qsmxt_engine::scan::ScanFacts, plan: &qsmxt_engine::plan::Plan,
    record: &qsmxt_engine::record::RunRecord, output: &DerivativeOutputs,
) -> crate::Result<()> {
    use crate::bids::derivatives::{remove_graph, write_graph};
    let key = &run.key;
    let methods_path = output.run_methods_path(key);
    if let Some(dir) = methods_path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let summary = qsmxt_engine::dataset::summarise_run(config, plan, facts, record, "qsmxt");
    std::fs::write(&methods_path, summary.methods.to_markdown())?;
    let file = RunRecordFile { record: record.clone(), scan: facts.clone() };
    let json = serde_json::to_string_pretty(&file).map_err(|e| QsmxtError::Config(e.to_string()))?;
    std::fs::write(output.run_record_path(key), json + "\n")?;
    // Drawn on the grid the run took: the return to the acquired grid only if it was resampled.
    write_graph(&output.run_graph_path(key, false), &summary.planned)?;
    let after_path = output.run_graph_path(key, true);
    if summary.differs {
        write_graph(&after_path, &summary.after)?;
    } else {
        remove_graph(&after_path)?;
    }
    Ok(())
}

/// `qsmxt run`'s [`StageStore`]: a run's volumes as files under the derivatives directory, named
/// by [`DerivativeOutputs`], its step cache in the run's state file ([`PipelineState`]), and a
/// provenance record for every step.
///
/// Returning the outputs to the acquired grid rewrites them in place; the working-grid originals
/// are kept aside first, and put back when a later run starts ([`restore_working_grid`]).
struct FileStore<'a> {
    run: &'a QsmRun,
    config: &'a PipelineConfig,
    /// The run's outputs, named in the space they are published in once the grid is known.
    output: DerivativeOutputs,
    /// The run's grid, once loaded.
    meta: Option<RunMetadata>,
    state: PipelineState,
    state_path: PathBuf,
    /// When the step being computed started, for its provenance.
    started: Option<Instant>,
    /// The step being computed.
    step: Option<String>,
    /// The supplied brain mask, once found.
    supplied_mask: Option<PathBuf>,
    /// Files the step being computed wrote beside its outputs (the figures beside the statistics
    /// table), recorded with them.
    extra_outputs: Vec<PathBuf>,
    /// What the step being computed keeps in the state file rather than in a file of its own
    /// (SynthSeg's per-label volumes).
    metadata: Option<serde_json::Value>,
    /// The working-grid copies of the run's published volumes are kept aside for the return to the
    /// acquired grid being computed.
    working_grid_kept: bool,
}

impl FileStore<'_> {
    /// Where `item` is kept, when it has a place.
    fn path(&self, item: &Item) -> Option<PathBuf> {
        let (o, key) = (&self.output, &self.run.key);
        let echo = |n: usize| self.run.echoes.get(n.checked_sub(1)?);
        let pass = |p: EnginePass, main: PathBuf, reliable: PathBuf| match p {
            EnginePass::Main => main,
            EnginePass::Reliable => reliable,
        };
        Some(match *item {
            Item::AcquiredPhase(n) => echo(n)?.phase_nifti.clone(),
            Item::AcquiredMagnitude(n) => echo(n)?.magnitude_nifti.clone()?,
            Item::AcquiredImaginary(n) => echo(n)?.imaginary_nifti.clone()?,
            Item::CoilPhase(c, n) => self.run.coils.as_ref()?.get(c.checked_sub(1)?)?.echoes.get(n.checked_sub(1)?)?.phase_nifti.clone(),
            Item::CoilMagnitude(c, n) =>
                self.run.coils.as_ref()?.get(c.checked_sub(1)?)?.echoes.get(n.checked_sub(1)?)?.magnitude_nifti.clone()?,
            Item::CombinedEchoPhase(n) => o.combined_phase_path(key, n),
            Item::CombinedEchoMagnitude(n) => o.combined_mag_path(key, n),
            Item::CoilCombinationMask => o.combine_mask_path(key),
            Item::Phase(n) => o.phase_scaled_path(key, n),
            Item::Magnitude(n) => o.mag_path(key, n),
            Item::CorrectedPhase(n) => o.motion_phase_path(key, n),
            Item::CorrectedMagnitude(n) => o.motion_mag_path(key, n),
            Item::CombinedMagnitude => o.magnitude_path(key),
            Item::BrainMask => o.mask_path(key),
            Item::SuppliedMask => self.supplied_mask.clone()?,
            Item::CoverageMask => o.motion_coverage_path(key),
            Item::FieldPpm => o.field_ppm_path(key),
            Item::TotalFieldHz => o.total_field_path(key),
            Item::TotalFieldMagnitude => o.total_field_magnitude_path(key),
            Item::RobustFitReport => robust_fit_report_path(&o.field_ppm_path(key)),
            Item::LocalField(p) => pass(p, o.local_field_path(key), o.reliable_local_field_path(key)),
            Item::LocalFieldMask(p) => pass(p, o.bg_mask_path(key), o.reliable_bg_mask_path(key)),
            Item::RawChi(p) => pass(p, o.chi_raw_path(key), o.reliable_chi_raw_path(key)),
            Item::ReliableMask => o.two_pass_mask_path(key),
            Item::QsmartIntermediate(p, name) => o.qsmart_intermediate_path(key, &Stage::Qsmart(p).name(), name)?,
            Item::CombinedRawChi => o.two_pass_chi_raw_path(key),
            Item::Chi => o.qsm_path(key),
            Item::ChiSupport => o.qsm_support_path(key),
            Item::SinglePassChi => o.singlepass_qsm_path(key),
            Item::Segmentation => o.dseg_path(key),
            Item::SuppliedSegmentation => self.custom(self.config.segmentation.custom_dseg_tool.as_deref(), "*_dseg.nii*", &[])?,
            Item::SegmentationLabels => o.dseg_lookup_path(key),
            // Kept in the state file, with the step's record.
            Item::SegmentationVolumes => return None,
            Item::RegionStats => o.segmentation_stats_path(key),
            Item::MotionReport => o.motion_report_path(key),
            Item::Swi => o.swi_path(key),
            Item::SwiMip => o.swi_mip_path(key),
            Item::Smwi(c) => o.smwi_path(key, c),
            Item::SmwiMip(c) => o.smwi_mip_path(key, c),
            Item::R2star => o.r2star_path(key),
            Item::T2star => o.t2star_path(key),
            Item::MeseMagnitude(n) => self.run.mese.as_ref()?.magnitude_niftis.get(n.checked_sub(1)?)?.clone(),
            Item::SuppliedR2 => self.custom(self.config.separation.custom_r2_tool.as_deref(), "*_R2map.nii*", &[])?,
            Item::R2 => o.r2_path(key),
            Item::SuppliedR2prime =>
                self.custom(self.config.separation.custom_r2prime_tool.as_deref(), "*_R2primemap.nii*", &[])?,
            Item::R2prime => o.r2prime_path(key),
            Item::R2primeCoverage => o.r2prime_coverage_path(key),
            Item::SuppliedChi => self.custom(self.config.separation.custom_qsm_tool.as_deref(), "*_Chimap.nii*", &["desc-"])?,
            Item::SeparatedChi(Contrast::Paramagnetic) => o.chi_para_path(key),
            Item::SeparatedChi(Contrast::Diamagnetic) => o.chi_dia_path(key),
            Item::SeparatedTotalChi => o.chi_sep_total_path(key),
        })
    }

    /// The derivative `tool` supplies for this run, when one is configured and found.
    fn custom(&self, tool: Option<&str>, glob: &str, exclude: &[&str]) -> Option<PathBuf> {
        find_custom_derivative(self.run, tool?, glob, exclude)
    }

    /// Before the first output is returned to the acquired grid: copy every one of the run's
    /// published volumes aside, since later steps read them back on the working grid (the mask,
    /// the combined magnitude, ...) and a re-run has to start from them rather than from what lands
    /// in `anat/`. Only this run's files: every run of the session shares `anat/`, and another
    /// run's outputs may be on its own working grid, or still being read by a reconstruction in
    /// progress.
    fn keep_working_grid_copies(&mut self) -> crate::Result<()> {
        let stash = self.output.working_grid_dir(&self.run.key);
        if stash.exists() {
            std::fs::remove_dir_all(&stash)?;
        }
        std::fs::create_dir_all(&stash)?;
        for f in self.output.run_published_niftis(&self.run.key) {
            // Not a file some other configuration left in another space.
            if f.file_name().and_then(|n| crate::bids::naming::entity_of(&n.to_string_lossy(), "space")).is_some() {
                continue;
            }
            if let Some(name) = f.file_name() {
                std::fs::copy(&f, stash.join(name))?;
            }
        }
        self.working_grid_kept = true;
        Ok(())
    }

    /// One figure per map beside the statistics table. Best-effort: the numbers are the
    /// deliverable, and a figure that fails to draw must not cost the run the statistics it
    /// already computed.
    fn draw_figures(&mut self, table: &str) {
        let anat = self.output.anat_dir(&self.run.key);
        match crate::pipeline::figure::render_all(
            &stats::parse_tsv(table),
            &self.run.key.to_string(),
            "Mean per structure; whiskers ±1 SD over the structure's voxels",
            "Volume from the SynthSeg posteriors, partial volume included",
        ) {
            Ok(figs) => for f in figs {
                match f.save_in(&anat, &self.run.key.basename()) {
                    Ok(p) => { log::info!("Figure -> {}", p.display()); self.extra_outputs.push(p); }
                    Err(e) => log::warn!("Could not write {}: {e}", f.stem),
                }
            },
            Err(e) => log::warn!("Could not draw the per-structure figures: {e}"),
        }
    }

    fn existing(&self, item: &Item) -> crate::Result<PathBuf> {
        self.path(item).ok_or_else(|| QsmxtError::Config(format!("{}: no {item:?} to read", self.run.key)))
    }

    fn meta(&self) -> &RunMetadata {
        self.meta.as_ref().expect("the run's grid is known before any output is written")
    }
}

impl StageStore for FileStore<'_> {
    type Error = QsmxtError;

    fn input_identity(&mut self) -> serde_json::Value {
        serde_json::Value::Array(input_identity(self.run))
    }

    fn header(&mut self) -> crate::Result<Header> {
        let first = &self.run.echoes[0];
        let first_phase = io::read_nifti_file(&first.phase_nifti)
            .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", first.phase_nifti.display(), e)))?;
        validate_run_dims(self.run, &first_phase)?;
        Ok(Header {
            dims: first_phase.dims, voxel_size: first_phase.voxel_size, affine: first_phase.affine,
            sidecar: crate::bids::sidecar::read_sidecar(&first.phase_json).ok(),
        })
    }

    fn sidecar(&mut self) -> crate::Result<Option<qsmxt_engine::scan::QsmSidecar>> {
        Ok(Some(crate::bids::sidecar::read_sidecar(&self.run.echoes[0].phase_json)?))
    }

    fn coils(&mut self) -> Vec<u32> {
        self.run.coils.iter().flatten().map(|c| c.coil_number).collect()
    }

    fn cached_scan(&mut self, key: &str) -> Option<RunMetadata> {
        // A cache from before the load step was keyed is taken as it stands.
        if let Some(Some(stored)) = self.state.completed_steps.get("load").map(|r| r.params_hash.as_ref()) {
            if stored != key {
                log::info!("Input files or geometry settings changed (obliquity threshold, algorithm, \
                            motion correction) — recomputing everything");
                self.state.completed_steps.clear();
            }
        }
        if !self.state.is_step_cached("load") {
            return None;
        }
        self.state.run_metadata.clone()
    }

    fn record_scan(&mut self, key: &str, meta: &RunMetadata, decisions: &[Decision]) -> crate::Result<()> {
        self.state.run_metadata = Some(meta.clone());
        self.state.mark_completed("load", vec![], Some(key.to_string()));
        self.state.set_decisions("load", decisions);
        self.state.save(&self.state_path)
    }

    fn decisions(&mut self, step: &str) -> Vec<Decision> {
        self.state.completed_steps.get(step).map(|r| r.decisions.clone()).unwrap_or_default()
    }

    fn begin(&mut self, meta: &RunMetadata) -> crate::Result<()> {
        self.meta = Some(meta.clone());
        restore_working_grid(self.run, &self.output, &mut self.state, &self.state_path)?;
        // A run left on the axial grid it was resampled to names its outputs in that space (#275).
        let space = published_space(self.config, meta);
        if self.state.space.as_deref() != space {
            // Every output of the last run is under the other names: nothing cached can be found,
            // and nothing under those names may stay beside the new ones.
            self.state.remove_recorded_outputs(&self.state_path);
            self.state.completed_steps.retain(|step, _| step == "load");
            self.state.space = space.map(str::to_string);
            self.state.save(&self.state_path)?;
        }
        self.output = self.output.in_space(space);
        Ok(())
    }

    fn has(&mut self, item: &Item) -> bool {
        self.path(item).is_some_and(|p| p.exists())
    }

    fn real(&mut self, item: &Item) -> crate::Result<Vec<f64>> {
        load_volume(&self.existing(item)?)
    }

    fn mask(&mut self, item: &Item) -> crate::Result<Vec<u8>> {
        load_mask(&self.existing(item)?)
    }

    fn supplied_mask(&mut self) -> Supplied {
        let tool = self.config.masking.custom_mask_tool.as_deref().unwrap_or_default();
        let Some(path) = find_custom_mask(self.run, tool) else {
            let ses = self.run.key.session.as_ref().map(|s| format!("/ses-{}", s)).unwrap_or_default();
            return Supplied::Missing { searched: format!("sub-{}{}", self.run.key.subject, ses) };
        };
        self.supplied_mask = Some(path.clone());
        let origin = path.display().to_string();
        match io::read_nifti_file(&path) {
            Ok(nd) => Supplied::Found { origin, data: nd.data, dims: nd.dims, affine: nd.affine },
            Err(error) => Supplied::Unreadable { origin, error },
        }
    }

    fn mese(&mut self) -> Option<qsmxt_engine::store::Mese> {
        self.run.mese.as_ref().map(|m| qsmxt_engine::store::Mese {
            name: m.key.to_string(), dims: m.dims, affine: m.affine, echo_times: m.echo_times.clone(),
        })
    }

    fn supplied(&mut self, item: &Item) -> Supplied {
        let Some(path) = self.path(item) else {
            return Supplied::Missing { searched: format!("the inputs of {}", self.run.key) };
        };
        let origin = path.display().to_string();
        match io::read_nifti_file(&path) {
            Ok(nd) => Supplied::Found { origin, data: nd.data, dims: nd.dims, affine: nd.affine },
            Err(error) => Supplied::Unreadable { origin, error },
        }
    }

    /// The path, which is what earlier releases keyed a step by.
    fn identity(&mut self, item: &Item) -> serde_json::Value {
        serde_json::json!(self.path(item).unwrap_or_default().to_string_lossy())
    }

    fn recorded_key(&mut self, step: &str) -> Option<String> {
        self.state.completed_steps.get(step).and_then(|r| r.params_hash.clone())
    }

    fn cached(&mut self, step: &str, key: &str) -> bool {
        self.state.is_step_cached_with_hash(step, Some(key))
    }

    fn start(&mut self, step: &str) {
        self.started = Some(Instant::now());
        self.step = Some(step.to_string());
        self.extra_outputs.clear();
        self.metadata = None;
        self.working_grid_kept = false;
    }

    fn models(&mut self, models: &[&'static str]) -> crate::Result<()> {
        for id in models {
            prefetch_weights(id, &self.run.key.to_string())?;
        }
        Ok(())
    }

    fn put(&mut self, item: Item, data: Data<'_>) -> crate::Result<()> {
        if let (Item::SegmentationVolumes, Data::Json(volumes)) = (item, data) {
            self.metadata = Some(serde_json::json!({ "volumes": volumes }));
            return Ok(());
        }
        if self.step.as_deref() == Some("output_space") && !self.working_grid_kept {
            self.keep_working_grid_copies()?;
        }
        let Some(path) = self.path(&item) else {
            match item {
                Item::QsmartIntermediate(_, name) =>
                    log::warn!("QSMART reported an intermediate QSMxT has no name for ({name}); not saved"),
                _ => log::warn!("{}: {item:?} has no place among the outputs; not saved", self.run.key),
            }
            return Ok(());
        };
        // A new output: what its sidecar recorded about the one it replaces (a reference offset,
        // where an R2′ came from) no longer holds, and the step records it afresh. Returned to the
        // acquired grid, it is the same output.
        if self.step.as_deref() != Some("output_space") {
            if let Some(sidecar) = crate::bids::entities::sidecar_path(&path).filter(|p| p.is_file()) {
                std::fs::remove_file(sidecar)?;
            }
        }
        match data {
            Data::Real(v) => save_volume(&path, v, self.meta())?,
            Data::Mask(m) => save_mask(&path, m, self.meta())?,
            Data::OnGrid(v, g) => write_volume(&path, v, g.dims, g.voxel_size, &g.affine)?,
            Data::Text(t) => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&path, t)?;
            }
            Data::Json(j) => {
                let json = serde_json::to_string_pretty(j)
                    .map_err(|e| QsmxtError::Config(format!("{}: {e}", path.display())))?;
                std::fs::write(&path, json)?;
            }
            Data::Same(source) => {
                let source = self.existing(&source)?;
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(source, &path)?;
            }
        }
        if let (Item::RegionStats, Data::Text(table)) = (item, data) {
            log::info!("Per-structure statistics -> {}", path.display());
            self.draw_figures(table);
        }
        Ok(())
    }

    /// Merged into the sidecar's other fields: BIDS-Prov adds `GeneratedBy` to the same file.
    fn annotate(&mut self, item: &Item, fields: serde_json::Map<String, serde_json::Value>) -> crate::Result<()> {
        let Some(sidecar) = self.path(item).and_then(|p| crate::bids::entities::sidecar_path(&p)) else { return Ok(()) };
        let mut obj = read_json_object(&sidecar);
        obj.extend(fields);
        if let Some(parent) = sidecar.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&sidecar, serde_json::to_string_pretty(&obj)
            .map_err(|e| QsmxtError::Config(format!("{}: {e}", sidecar.display())))?)?;
        Ok(())
    }

    fn report(&mut self, item: &Item) -> Option<serde_json::Value> {
        if *item == Item::SegmentationVolumes {
            return self.state.completed_steps.get("segmentation")?.metadata.as_ref()?.get("volumes").cloned();
        }
        let text = std::fs::read_to_string(self.path(item)?).ok()?;
        Some(serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text)))
    }

    /// The item's JSON sidecar.
    fn annotation(&mut self, item: &Item) -> serde_json::Map<String, serde_json::Value> {
        self.path(item).and_then(|p| crate::bids::entities::sidecar_path(&p))
            .map(|sidecar| read_json_object(&sidecar)).unwrap_or_default()
    }

    fn discard(&mut self, step: &str, outputs: &[Item]) -> crate::Result<()> {
        for path in outputs.iter().filter_map(|i| self.path(i)) {
            if path.exists() {
                std::fs::remove_file(&path)?;
            }
        }
        if self.state.completed_steps.contains_key(step) {
            log::info!("{step} is off now: re-running the stages that read its output");
            self.state.invalidate(step);
            self.state.save(&self.state_path)?;
        }
        Ok(())
    }

    /// Each output left on the working grid gets a `SpatialReference`: the BIDS sidecar field a
    /// `space-` label that is not a standard template needs, pointing at the run's combined
    /// magnitude in that space.
    fn keep_working_grid(&mut self) -> crate::Result<()> {
        let Some(space) = self.output.space.as_deref() else { return Ok(()) };
        let reference = crate::pipeline::multiorient::spatial_reference(&self.output, &self.run.key, space);
        let files: Vec<PathBuf> = self.output.run_published_niftis(&self.run.key).into_iter()
            .filter(|p| p.file_name().and_then(|n| crate::bids::naming::entity_of(&n.to_string_lossy(), "space"))
                .as_deref() == Some(space))
            .collect();
        for f in files {
            let Some(sidecar) = crate::bids::entities::sidecar_path(&f) else { continue };
            let mut obj = read_json_object(&sidecar);
            obj.insert("SpatialReference".into(), serde_json::Value::String(reference.clone()));
            std::fs::write(&sidecar, serde_json::to_string_pretty(&serde_json::Value::Object(obj))
                .map_err(|e| QsmxtError::Config(format!("{}: {e}", sidecar.display())))?)?;
        }
        Ok(())
    }

    fn record(&mut self, c: &Completed<'_>) -> crate::Result<()> {
        let mut inputs: Vec<PathBuf> = c.inputs.iter().filter_map(|i| self.path(i)).collect();
        let mut outputs: Vec<PathBuf> = c.outputs.iter().filter_map(|i| self.path(i)).collect();
        if c.step == "output_space" {
            if !self.working_grid_kept {
                self.keep_working_grid_copies()?;
            }
            // Read: the working-grid originals each output was returned from.
            outputs.sort();
            let stash = self.output.working_grid_dir(&self.run.key);
            inputs = outputs.iter().filter_map(|f| f.file_name().map(|n| stash.join(n))).collect();
        }
        outputs.append(&mut self.extra_outputs);
        let input_refs: Vec<&Path> = inputs.iter().map(|p| p.as_path()).collect();
        let start = self.started.take().unwrap_or_else(Instant::now);
        // What the step found in the data is recorded next to its parameters; only the parameters
        // key the cache, since the outcome is not known when the cache is checked.
        let mut recorded = c.params.clone();
        if let Some((name, value)) = &c.outcome {
            recorded[*name] = value.clone();
        }
        let prov_path = self.output.provenance_path(&self.run.key, c.step);
        write_step_provenance(&prov_path, c.step, c.algorithm, &recorded, &input_refs, &outputs, start)?;
        // Another step whose provenance names one of these files as its output no longer made
        // what is there (its record is forgotten below, or already was): it would be named their
        // maker.
        for &other in crate::bids::bidsprov::STEPS.iter().filter(|&&s| s != c.step && s != "output_space") {
            let stale = self.output.provenance_path(&self.run.key, other);
            let Ok(text) = std::fs::read_to_string(&stale) else { continue };
            let theirs: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            let theirs = theirs["outputs"].as_array().cloned().unwrap_or_default();
            if theirs.iter().filter_map(|p| p.as_str()).any(|p| outputs.iter().any(|o| o.display().to_string() == p)) {
                std::fs::remove_file(stale)?;
            }
        }

        // Whatever read this step's previous output is stale now that it has been recomputed with
        // other parameters, or for the first time (a step newly switched on, say), which the
        // downstream steps' own parameter hashes would not notice. Recomputed with the same
        // parameters — its files were missing, or cleaned — it made the same output, and what
        // read it stays cached.
        let same_as_before = self.state.completed_steps.get(c.step)
            .is_some_and(|r| r.params_hash.as_deref() == Some(c.key));
        if !same_as_before {
            for ds in crate::pipeline::graph::downstream_of(c.step) {
                self.state.completed_steps.remove(&ds);
            }
        }
        self.state.mark_completed_with_metadata(c.step, outputs, Some(c.key.to_string()), self.metadata.take());
        self.state.set_decisions(c.step, c.decisions);
        self.state.save(&self.state_path)
    }

    fn finish(&mut self) -> crate::Result<()> {
        self.state.mark_run_complete(self.config);
        self.state.save(&self.state_path)
    }
}

/// `qsmxt run`'s [`Observer`]: each step's status line through the executor's progress callback,
/// a progress bar for its iterations, and the log.
struct RunObserver<'a> {
    key: String,
    progress: &'a dyn Fn(&str),
    bar: Option<Box<dyn FnMut(usize, usize)>>,
    started: Option<Instant>,
}

impl Observer for RunObserver<'_> {
    fn stage(&mut self, step: &str, algorithm: Option<&str>, event: StageEvent<'_>) {
        match event {
            StageEvent::Started(what) => {
                (self.progress)(what);
                self.started = Some(Instant::now());
                self.bar = Some(iter_progress_bar(&self.key, algorithm.unwrap_or(step)).0);
            }
            StageEvent::Cached => {}
            StageEvent::Finished(label) => {
                self.bar = None;
                if let Some(t) = self.started.take() {
                    log_step_done(label, t);
                }
            }
        }
    }

    fn progress(&mut self, done: usize, total: usize) {
        if let Some(bar) = self.bar.as_mut() {
            bar(done, total);
        }
    }

    fn log(&mut self, level: log::Level, message: &str) {
        log::log!(level, "{message}");
    }
}

// ─── Stage functions ───

/// Check every input volume of a run against the reference matrix size.
///
/// The whole run is processed on one grid, taken from the first phase echo, and that grid is
/// used to index every other volume — so an echo or a magnitude with a different matrix size
/// only surfaces much later as an out-of-bounds panic inside qsm-core (issue #184). Reading the
/// 348-byte header of each file up front is free and lets us name the file that is at fault.
fn validate_run_dims(run: &QsmRun, reference: &NiftiData) -> crate::Result<()> {
    let phase0 = run.echoes[0].phase_nifti.as_path();
    let reference_dims = io::read_nifti_dims(phase0).map_err(QsmxtError::NiftiIo)?;
    let (rx, ry, rz) = reference_dims;

    // The grid comes from the loaded volume, so the header has to agree with what was read.
    if reference.data.len() != rx * ry * rz {
        return Err(QsmxtError::DimensionMismatch(format!(
            "{} declares {}x{}x{} ({} voxels) in its header but {} voxels were read",
            phase0.display(), rx, ry, rz, rx * ry * rz, reference.data.len(),
        )));
    }

    let coil_echoes = run.coils.iter().flatten().flat_map(|c| c.echoes.iter().enumerate());
    for (i, echo) in run.echoes.iter().enumerate().chain(coil_echoes) {
        let mut inputs: Vec<(&str, &Path)> = vec![("phase", echo.phase_nifti.as_path())];
        if let Some(ref mag) = echo.magnitude_nifti {
            inputs.push(("magnitude", mag.as_path()));
        }
        for (kind, path) in inputs {
            let dims = io::read_nifti_dims(path).map_err(QsmxtError::NiftiIo)?;
            if dims != reference_dims {
                return Err(QsmxtError::DimensionMismatch(format!(
                    concat!(
                        "echo {} {} ({}) is {}x{}x{} but the run is processed on the {}x{}x{} grid ",
                        "of {}. Every echo of a run, phase and magnitude alike, must share one ",
                        "matrix size — check the BIDS conversion, and that the magnitude and phase ",
                        "come from the same acquisition",
                    ),
                    i + 1, kind, path.display(), dims.0, dims.1, dims.2,
                    rx, ry, rz, phase0.display(),
                )));
            }
        }
    }

    Ok(())
}

/// The `space-` label of a run's published outputs: [`naming::WORKING_SPACE`] when the run was
/// resampled to an axial grid and `--output-space working` leaves its outputs there, `None` when
/// they are on the grid it was acquired on (never resampled, or returned to it).
///
/// [`naming::WORKING_SPACE`]: crate::bids::naming::WORKING_SPACE
pub fn published_space(config: &PipelineConfig, meta: &RunMetadata) -> Option<&'static str> {
    (meta.source_geometry.is_some()
        && config.pipeline.output_space == crate::pipeline::config::OutputSpace::Working)
        .then_some(crate::bids::naming::WORKING_SPACE)
}

/// Undo a previous run's return to the acquired grid before any step reads `anat/` again.
///
/// Cached steps hand their outputs to the steps after them on the working grid, but the return
/// to the acquired grid (the `output_space` step) rewrote them in place. Put the working-grid
/// copies back and let it run again at the end. Without the copies (a run from before they were kept), nothing in `anat/`
/// can be trusted on the working grid, so everything after loading is redone.
fn restore_working_grid(
    run: &QsmRun, output: &DerivativeOutputs, state: &mut PipelineState, state_path: &Path,
) -> crate::Result<()> {
    // Same configuration, every output in place: nothing will be recomputed, so nothing will read
    // `anat/` on the working grid, and the outputs can stay where the last run left them.
    if !state.completed_steps.contains_key("output_space") || state.config_unchanged {
        return Ok(());
    }
    let stash = output.working_grid_dir(&run.key);
    let saved: Vec<PathBuf> = std::fs::read_dir(&stash)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    if saved.is_empty() {
        log::warn!(
            "Outputs of {} were returned to the acquired grid by an earlier run that kept no \
             working-grid copies; recomputing them",
            run.key
        );
        state.completed_steps.retain(|step, _| step == "load");
    } else {
        for f in &saved {
            if let Some(name) = f.file_name() {
                let dir = output.published_dir_for(&run.key, &name.to_string_lossy());
                std::fs::create_dir_all(&dir)?;
                std::fs::copy(f, dir.join(name))?;
            }
        }
        state.completed_steps.remove("output_space");
    }
    state.save(state_path)
}

/// Every input file of a run — echoes, sidecars, uncombined coils, the MESE — by path, size and
/// modification time.
fn input_identity(run: &QsmRun) -> Vec<serde_json::Value> {
    let mut files: Vec<&Path> = Vec::new();
    let echoes = run.echoes.iter().chain(run.coils.iter().flatten().flat_map(|c| c.echoes.iter()));
    for e in echoes {
        files.extend([e.phase_nifti.as_path(), e.phase_json.as_path()]);
        files.extend(e.magnitude_nifti.as_deref());
        files.extend(e.magnitude_json.as_deref());
    }
    if let Some(mese) = &run.mese {
        files.extend(mese.magnitude_niftis.iter().map(|p| p.as_path()));
    }
    files.iter().map(|p| {
        let meta = std::fs::metadata(p).ok();
        let modified = meta.as_ref().and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos().to_string());
        serde_json::json!([p.display().to_string(), meta.map(|m| m.len()), modified])
    }).collect()
}

/// Locate a bring-your-own derivative under `<bids>/derivatives/<tool>/sub-*/[ses-*/]anat/<glob>`
/// for this run. `tool == "*"` searches every derivatives dir alphabetically; within a tool the
/// first matching file (alphabetical) wins. Any candidate whose name contains one of `exclude` is
/// passed over — `desc-` for a `Chimap` query, so it never returns a chi-separation output.
/// Returns None if nothing matches.
pub(crate) fn find_custom_derivative(
    run: &QsmRun, tool: &str, suffix_glob: &str, exclude: &[&str],
) -> Option<PathBuf> {
    // BIDS root = strip <sub-X>[/ses-Y]/anat/<file> off the first echo's phase path.
    let anat = run.echoes.first()?.phase_nifti.parent()?;   // .../sub-X[/ses-Y]/anat
    let sub_or_ses = anat.parent()?;
    let bids_root = if run.key.session.is_some() {
        sub_or_ses.parent()?.parent()?
    } else {
        sub_or_ses.parent()?
    };
    let deriv = bids_root.join("derivatives");
    if !deriv.is_dir() {
        return None;
    }
    let tool_dirs: Vec<PathBuf> = if tool == "*" {
        let mut ds: Vec<PathBuf> = std::fs::read_dir(&deriv).ok()?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        ds.sort();
        ds
    } else {
        vec![deriv.join(tool)]
    };
    let sub = format!("sub-{}", run.key.subject);
    let ses = run.key.session.as_ref().map(|s| format!("ses-{}", s));
    for td in tool_dirs {
        let anat_dir = match &ses {
            Some(s) => td.join(&sub).join(s).join("anat"),
            None => td.join(&sub).join("anat"),
        };
        let pattern = format!("{}/{}", anat_dir.display(), suffix_glob);
        if let Ok(paths) = glob(&pattern) {
            let mut hits: Vec<PathBuf> = paths.filter_map(|r| r.ok())
                .filter(|p| {
                    let name = p.to_string_lossy().to_string();
                    !exclude.iter().any(|e| name.contains(e))
                })
                .collect();
            hits.sort();
            if let Some(p) = hits.into_iter().next() {
                return Some(p);
            }
        }
    }
    None
}

/// Bring-your-own brain mask (`*_mask.nii*`).
///
/// `desc-` masks are accepted here, unlike for a `Chimap` — `desc-brain_mask` and `desc-bet_mask`
/// are how most tools name a brain mask. The one exclusion is our own reliable-phase mask: a
/// two-pass run writes `_desc-reliable_mask.nii` into the same folder, it sorts *before* the brain
/// mask, and a later run pointed at those derivatives would reconstruct inside the holey mask and
/// call it the brain — with no error anywhere to say so. The χ support mask (`_desc-qsm_mask.nii`)
/// is excluded for the same reason: it is the brain mask less an eroded rim. So is the R2′
/// coverage (`_desc-r2prime_mask.nii`), which is the brain mask cut to the MESE's field of view.
pub(crate) fn find_custom_mask(run: &QsmRun, tool: &str) -> Option<PathBuf> {
    find_custom_derivative(run, tool, "*_mask.nii*", &["_desc-reliable_", "_desc-qsm_", "_desc-r2prime_"])
}

/// The robust echo fit's per-echo report, next to the field map: `<field map>_robustfit.json`.
fn robust_fit_report_path(field_path: &Path) -> PathBuf {
    let name = field_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let stem = name.strip_suffix(".nii.gz").or_else(|| name.strip_suffix(".nii")).unwrap_or(&name);
    field_path.with_file_name(format!("{stem}_robustfit.json"))
}

/// A JSON file's top-level object, or an empty one if it is absent or unreadable.
fn read_json_object(path: &Path) -> serde_json::Map<String, serde_json::Value> {
    std::fs::read_to_string(path).ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

/// Run one of the engine's steps on a run's files, as [`run_pipeline_cached`] does once the run's
/// grid is `meta`, from a fresh state.
#[cfg(test)]
fn run_engine_step(
    run: &QsmRun, config: &PipelineConfig, output: &DerivativeOutputs, meta: &RunMetadata, stage: Stage,
) -> crate::Result<()> {
    let state_path = output.state_path(&run.key);
    let state = PipelineState::load_or_create(&state_path, config, &run.key, true);
    let facts = requirements::scan_facts(run, config);
    let plan = qsmxt_engine::plan::plan(config, &facts);
    let mut store = FileStore {
        run, config, output: output.clone(), meta: Some(meta.clone()), state, state_path,
        started: None, step: None, supplied_mask: None, extra_outputs: Vec::new(),
        metadata: None, working_grid_kept: false,
    };
    let step = qsmxt_engine::plan::Step::new(stage, vec![]);
    qsmxt_engine::run::run_step(&plan, config, &facts, meta, &step, &mut store, &mut ()).map(|_| ())
}

/// The grid and B0 direction the engine decides for a discovered run from its first phase image's
/// header ([`qsmxt_engine::scan::resolve_geometry`]).
#[cfg(test)]
fn resolve_run_geometry(run: &QsmRun, first_phase: &NiftiData, config: &PipelineConfig) -> crate::Result<RunMetadata> {
    Ok(qsmxt_engine::scan::resolve_geometry(
        first_phase.dims, first_phase.voxel_size, first_phase.affine, &requirements::scan_facts(run, config), config))
}

/// The engine's slice-gap check ([`qsmxt_engine::scan::check_slice_gap`]) for a discovered run,
/// from its first echo's sidecar.
#[cfg(test)]
fn check_slice_gap(
    run: &QsmRun, first_phase: &NiftiData, meta: &RunMetadata, config: &PipelineConfig,
) -> crate::Result<Option<(f64, usize)>> {
    let Some(sc) = crate::bids::sidecar::read_sidecar(&run.echoes[0].phase_json).ok() else { return Ok(None) };
    Ok(qsmxt_engine::scan::check_slice_gap(&sc, first_phase.dims, first_phase.voxel_size, meta, config)?.0)
}

#[cfg(test)]
mod tests {
    use qsm_core::pipeline::config::QsmReference as CoreRef;

    /// The two passes must not share a single output path. If they did, the second pass would
    /// overwrite the first's local field, eroded mask or raw map — and the combination would be
    /// one reconstruction blended with itself.
    #[test]
    fn the_two_passes_write_to_different_places() {
        use crate::bids::derivatives::DerivativeOutputs;
        use crate::pipeline::graph::PipelineState;
        use qsmxt_engine::plan::{Pass, Stage};
        use qsmxt_engine::store::Item;
        use std::path::PathBuf;

        let run = run_with_echoes(vec![(PathBuf::from("/in/sub-1/anat/p.nii"), None)]);
        let config = crate::pipeline::config::PipelineConfig::default();
        let store = super::FileStore {
            run: &run, config: &config, output: DerivativeOutputs::new(std::path::Path::new("/out")),
            meta: None, state: PipelineState::new(&config, &run.key), state_path: PathBuf::from("/out/s.json"),
            started: None, step: None, supplied_mask: None, extra_outputs: Vec::new(),
        metadata: None, working_grid_kept: false,
        };
        for (a, b) in [
            (Item::BrainMask, Item::ReliableMask),
            (Item::LocalField(Pass::Main), Item::LocalField(Pass::Reliable)),
            (Item::LocalFieldMask(Pass::Main), Item::LocalFieldMask(Pass::Reliable)),
            (Item::RawChi(Pass::Main), Item::RawChi(Pass::Reliable)),
        ] {
            assert_ne!(store.path(&a), store.path(&b), "the two passes share an output path");
        }

        // And their steps are cached separately, or one pass's cache hit would skip the other.
        for stage in [Stage::Mask, Stage::BackgroundRemoval, Stage::Inversion, Stage::Tgv, Stage::Qsmart] {
            assert_ne!(stage(Pass::Main).name(), stage(Pass::Reliable).name());
        }
        assert_eq!(Stage::Inversion(Pass::Main).name(), "invert", "the main pass keeps the unsuffixed step names");
    }

    /// The BIDS name the engine gives each item ([`qsmxt_engine::store::Item::meta`]) is the one
    /// its file is written under, and what it calls published is what lands among the run's
    /// outputs rather than in a workflow directory.
    #[test]
    fn item_metadata_names_the_files_qsmxt_writes() {
        use crate::bids::derivatives::DerivativeOutputs;
        use crate::pipeline::graph::PipelineState;
        use qsmxt_engine::store::Item;
        use std::path::PathBuf;

        let run = run_with_echoes(vec![(PathBuf::from("/in/sub-1/anat/p.nii"), None)]);
        let config = crate::pipeline::config::PipelineConfig::default();
        let store = super::FileStore {
            run: &run, config: &config, output: DerivativeOutputs::new(std::path::Path::new("/out")),
            meta: None, state: PipelineState::new(&config, &run.key), state_path: PathBuf::from("/out/s.json"),
            started: None, step: None, supplied_mask: None, extra_outputs: Vec::new(),
            metadata: None, working_grid_kept: false,
        };
        for item in Item::one_of_each() {
            let meta = item.meta();
            let Some(bids) = meta.bids_name else { continue };
            let path = store.path(&item).unwrap_or_else(|| panic!("{item:?} has a BIDS name and no file"));
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let stem = name.split('.').next().unwrap();
            assert!(stem.ends_with(&format!("_{bids}")), "{item:?}: {name} is not named {bids}");
            assert_eq!(meta.published, !path.starts_with("/out/workflow"), "{item:?}: {}", path.display());
        }
    }

    /// Single-step and end-to-end algorithms produce a map over the brain mask, with no separate
    /// background-removal stage to erode it.
    #[test]
    fn single_step_algorithms_skip_background_removal() {
        use crate::pipeline::config::{PipelineConfig, QsmAlgorithm};
        let mut config = PipelineConfig::default();

        for alg in [QsmAlgorithm::Rts, QsmAlgorithm::Tkd, QsmAlgorithm::Ilsqr] {
            config.inversion.algorithm = alg;
            assert!(!qsmxt_engine::bridge::skips_bgremove(&config), "{alg} runs its own background removal stage");
        }
        for alg in [QsmAlgorithm::Nextqsm, QsmAlgorithm::Autoqsm, QsmAlgorithm::Iqsm,
                    QsmAlgorithm::IqsmPlus, QsmAlgorithm::Tgv, QsmAlgorithm::Qsmart, QsmAlgorithm::Tfi] {
            config.inversion.algorithm = alg;
            assert!(qsmxt_engine::bridge::skips_bgremove(&config), "{alg} has no separate background removal");
        }

        // MEDI only skips it with SMV enabled.
        config.inversion.algorithm = QsmAlgorithm::Medi;
        config.inversion.medi.smv = false;
        assert!(!qsmxt_engine::bridge::skips_bgremove(&config));
        config.inversion.medi.smv = true;
        assert!(qsmxt_engine::bridge::skips_bgremove(&config));
    }

    /// A bring-your-own mask has no recipe to leave unfilled, so two-pass has nothing to build a
    /// reliable mask from. Without this gate the run would reconstruct twice from the same mask
    /// and "combine" a map with itself — twice the time, and an output indistinguishable from the
    /// single-pass one.
    #[test]
    fn a_custom_mask_turns_two_pass_off() {
        use crate::pipeline::config::PipelineConfig;
        let two_pass = |config: &PipelineConfig, run: &crate::bids::discovery::QsmRun| {
            qsmxt_engine::plan::plan(config, &crate::pipeline::requirements::scan_facts(run, config)).two_pass
        };

        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path();
        let anat = bids.join("sub-1/anat");
        std::fs::create_dir_all(&anat).unwrap();
        let phase = anat.join("sub-1_echo-1_part-phase_MEGRE.nii");
        write_vol(&phase, (2, 2, 2));
        let run = run_with_echoes(vec![(phase, None)]);

        let mut config = PipelineConfig::default();
        config.masking.two_pass = true;
        assert!(two_pass(&config, &run), "computed mask: two-pass applies");

        // Asking for a BYO mask that is actually there turns it off.
        let deriv_anat = bids.join("derivatives/bet/sub-1/anat");
        std::fs::create_dir_all(&deriv_anat).unwrap();
        write_vol(&deriv_anat.join("sub-1_MEGRE_mask.nii"), (2, 2, 2));
        config.masking.custom_mask_tool = Some("bet".to_string());
        assert!(!two_pass(&config, &run));

        // --use-custom-masks falls back to computing the mask when nothing matches, and then
        // there *is* a recipe — so two-pass applies after all.
        config.masking.custom_mask_tool = Some("nonexistent".to_string());
        assert!(two_pass(&config, &run), "a BYO mask that isn't there is not a BYO mask");

        // And it is off whenever QSM is, or whenever it was not asked for.
        config.masking.custom_mask_tool = None;
        config.pipeline.do_qsm = false;
        assert!(!two_pass(&config, &run));
        config.pipeline.do_qsm = true;
        config.masking.two_pass = false;
        assert!(!two_pass(&config, &run));
    }

    /// A bring-your-own mask must be the brain mask, not a two-pass reliable mask that happens
    /// to sit next to it. `desc-reliable` sorts before the plain `_mask` file, so the first
    /// alphabetical match is the wrong one — and reconstructing inside a holey mask while calling
    /// it the brain produces a plausible-looking map with no error anywhere.
    #[test]
    fn a_custom_mask_lookup_ignores_the_reliable_mask() {
        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path();
        let anat = bids.join("sub-1/anat");
        std::fs::create_dir_all(&anat).unwrap();
        let phase = anat.join("sub-1_echo-1_part-phase_MEGRE.nii");
        write_vol(&phase, (2, 2, 2));

        let deriv_anat = bids.join("derivatives/qsmxt/sub-1/anat");
        std::fs::create_dir_all(&deriv_anat).unwrap();
        let brain = deriv_anat.join("sub-1_MEGRE_mask.nii");
        let reliable = deriv_anat.join("sub-1_MEGRE_desc-reliable_mask.nii");
        write_vol(&brain, (2, 2, 2));
        write_vol(&reliable, (2, 2, 2));
        assert!(reliable < brain, "the reliable mask sorts first — that is the trap");

        let run = run_with_echoes(vec![(phase, None)]);
        assert_eq!(super::find_custom_mask(&run, "qsmxt"), Some(brain.clone()));
        assert_eq!(super::find_custom_mask(&run, "*"), Some(brain.clone()));

        // Only *our* reliable mask is excluded: `desc-brain`/`desc-bet` is how most tools name a
        // brain mask, and those must still be found — including when they are the only candidate.
        std::fs::remove_file(&brain).unwrap();
        let desc_brain = deriv_anat.join("sub-1_MEGRE_desc-brain_mask.nii");
        write_vol(&desc_brain, (2, 2, 2));
        assert_eq!(super::find_custom_mask(&run, "qsmxt"), Some(desc_brain));
    }

    #[test]
    fn test_prefetch_weights_noop_and_download_bar() {
        // Classical algorithm ids aren't registry models → prefetch is a no-op (in both the
        // `dl` and non-`dl` builds), no network.
        assert!(super::prefetch_weights("rts", "test").is_ok());
        assert!(super::prefetch_weights("vsharp", "test").is_ok());
        // The download bar renders without touching the network (only exists in `dl` builds).
        #[cfg(feature = "dl")]
        {
            let pb = super::create_download_bar("test ↓ x.onnx", 1000);
            pb.set_position(500);
            pb.finish_and_clear();
        }
    }

    #[cfg(not(feature = "dl"))]
    #[test]
    fn test_prefetch_weights_errors_for_dl_without_dl_feature() {
        // Selecting a DL model in a non-DL build must fail with a clear message.
        let err = super::prefetch_weights("qsmnet", "test").unwrap_err();
        assert!(format!("{}", err).contains("deep-learning"), "got: {}", err);
    }

    #[cfg(not(feature = "dl"))]
    #[test]
    fn test_hd_bet_mask_needs_dl_feature() {
        // The mask stage prefetches every DL op's weights; HD-BET must hit the same clear error.
        let sections = crate::pipeline::config::to_mask_sections(&crate::pipeline::config::hd_bet_mask_sections(), &Default::default());
        let ids: Vec<&str> = sections.iter().flat_map(|s| s.all_ops()).filter_map(|op| op.dl_model_id()).collect();
        assert_eq!(ids, ["hd-bet"]);
        let err = super::prefetch_weights(ids[0], "test").unwrap_err();
        assert!(format!("{}", err).contains("deep-learning"), "got: {}", err);
    }

    #[cfg(not(feature = "dl"))]
    #[test]
    fn test_rs2_net_mask_needs_dl_feature() {
        // RS2-Net is a DL generator like HD-BET, so it must reach the same prefetch error
        // rather than failing later with something about ONNX.
        let sections = crate::pipeline::config::to_mask_sections(&crate::pipeline::config::rs2_net_mask_sections(), &Default::default());
        let ids: Vec<&str> = sections.iter().flat_map(|s| s.all_ops()).filter_map(|op| op.dl_model_id()).collect();
        assert_eq!(ids, ["rs2-net"]);
        let err = super::prefetch_weights(ids[0], "test").unwrap_err();
        assert!(format!("{}", err).contains("deep-learning"), "got: {}", err);
    }

    #[cfg(not(feature = "dl"))]
    #[test]
    fn test_mindgrab_mask_needs_dl_feature() {
        let sections = crate::pipeline::config::to_mask_sections(&crate::pipeline::config::mindgrab_mask_sections(), &Default::default());
        let ids: Vec<&str> = sections.iter().flat_map(|s| s.all_ops()).filter_map(|op| op.dl_model_id()).collect();
        assert_eq!(ids, ["mindgrab"]);
        let err = super::prefetch_weights(ids[0], "test").unwrap_err();
        assert!(format!("{}", err).contains("deep-learning"), "got: {}", err);
    }

    #[test]
    fn test_find_custom_mask() {
        use crate::bids::discovery::{EchoFiles, QsmRun};
        use crate::bids::entities::AcquisitionKey;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let anat = root.join("sub-1").join("anat");
        std::fs::create_dir_all(&anat).unwrap();
        let phase = anat.join("sub-1_part-phase_MEGRE.nii");
        std::fs::write(&phase, b"x").unwrap();
        let bet_anat = root.join("derivatives").join("bet").join("sub-1").join("anat");
        std::fs::create_dir_all(&bet_anat).unwrap();
        let mask = bet_anat.join("sub-1_desc-bet_mask.nii");
        std::fs::write(&mask, b"x").unwrap();

        let run = QsmRun {
            key: AcquisitionKey {
                subject: "1".into(), session: None, acquisition: None,
                reconstruction: None, inversion: None, run: None, suffix: "MEGRE".into(),
            },
            coils: None,
            echoes: vec![EchoFiles {
                echo_number: 1, phase_nifti: phase.clone(), phase_json: phase.clone(),
                magnitude_nifti: None, magnitude_json: None, imaginary_nifti: None,
            }],
            magnetic_field_strength: 3.0, echo_times: vec![0.004], b0_dir: None,
            dims: (2, 2, 2), has_magnitude: false,
            mese: None,
        };
        assert_eq!(super::find_custom_mask(&run, "bet"), Some(mask.clone()));
        assert_eq!(super::find_custom_mask(&run, "*"), Some(mask.clone())); // first tool alphabetically
        assert_eq!(super::find_custom_mask(&run, "nonexistent"), None);     // falls back (None)
    }

    #[test]
    fn test_apply_reference_mean_all_masked() {
        let chi = vec![1.0, 2.0, 3.0];
        let mask = vec![1u8, 1, 1];
        let result = qsm_core::pipeline::apply_reference(&chi, &mask, CoreRef::Mean);
        assert!((result[0] - (-1.0)).abs() < 1e-10);
        assert!((result[1] - 0.0).abs() < 1e-10);
        assert!((result[2] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_apply_reference_mean_partial_mask() {
        let chi = vec![1.0, 2.0, 3.0, 4.0];
        let mask = vec![1u8, 0, 1, 0];
        let result = qsm_core::pipeline::apply_reference(&chi, &mask, CoreRef::Mean);
        assert!((result[0] - (-1.0)).abs() < 1e-10);
        assert!((result[1] - 0.0).abs() < 1e-10);
        assert!((result[2] - 1.0).abs() < 1e-10);
        assert!((result[3] - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_apply_reference_mean_empty_mask() {
        let chi = vec![1.0, 2.0, 3.0];
        let mask = vec![0u8, 0, 0];
        let result = qsm_core::pipeline::apply_reference(&chi, &mask, CoreRef::Mean);
        assert_eq!(result, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_apply_reference_none() {
        let chi = vec![1.0, 2.0, 3.0];
        let mask = vec![1u8, 0, 1];
        let result = qsm_core::pipeline::apply_reference(&chi, &mask, CoreRef::None);
        assert!((result[0] - 1.0).abs() < 1e-10);
        assert!((result[1] - 0.0).abs() < 1e-10);
        assert!((result[2] - 3.0).abs() < 1e-10);
    }

    /// Build a `QsmRun` whose echoes point at the given (phase, magnitude) file pairs.
    pub(super) fn run_with_echoes(echoes: Vec<(std::path::PathBuf, Option<std::path::PathBuf>)>) -> crate::bids::discovery::QsmRun {
        use crate::bids::discovery::{EchoFiles, QsmRun};
        use crate::bids::entities::AcquisitionKey;
        let n = echoes.len();
        QsmRun {
            key: AcquisitionKey {
                subject: "1".into(), session: None, acquisition: None,
                reconstruction: None, inversion: None, run: None, suffix: "MEGRE".into(),
            },
            coils: None,
            echoes: echoes.into_iter().enumerate().map(|(i, (phase, mag))| EchoFiles {
                echo_number: i as u32 + 1, phase_json: phase.clone(), phase_nifti: phase,
                magnitude_json: None, magnitude_nifti: mag, imaginary_nifti: None,
            }).collect(),
            magnetic_field_strength: 3.0,
            echo_times: (0..n).map(|i| 0.004 + i as f64 * 0.004).collect(),
            b0_dir: None,
            dims: (4, 4, 4), has_magnitude: true, mese: None,
        }
    }

    /// Write a NIfTI of the given dimensions filled with 1.0.
    fn write_vol(path: &std::path::Path, dims: (usize, usize, usize)) {
        let data = vec![1.0f64; dims.0 * dims.1 * dims.2];
        qsm_core::io::save_nifti_to_file(path, &data, dims, (1.0, 1.0, 1.0), &[
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]).unwrap();
    }

    #[test]
    fn test_validate_run_dims_accepts_matching_inputs() {
        let dir = tempfile::tempdir().unwrap();
        let mut echoes = Vec::new();
        for e in 1..=2 {
            let phase = dir.path().join(format!("echo{}_phase.nii", e));
            let mag = dir.path().join(format!("echo{}_mag.nii", e));
            write_vol(&phase, (4, 4, 4));
            write_vol(&mag, (4, 4, 4));
            echoes.push((phase, Some(mag)));
        }
        let run = run_with_echoes(echoes);
        let reference = qsm_core::io::read_nifti_file(&run.echoes[0].phase_nifti).unwrap();
        super::validate_run_dims(&run, &reference).unwrap();
    }

    #[test]
    fn test_validate_run_dims_rejects_mismatched_magnitude() {
        // A magnitude smaller than the phase used to reach qsm-core and panic with an
        // out-of-bounds index during inhomogeneity correction (issue #184).
        let dir = tempfile::tempdir().unwrap();
        let phase = dir.path().join("echo1_phase.nii");
        let mag = dir.path().join("echo1_mag.nii");
        write_vol(&phase, (4, 4, 4));
        write_vol(&mag, (4, 4, 3));
        let run = run_with_echoes(vec![(phase, Some(mag.clone()))]);
        let reference = qsm_core::io::read_nifti_file(&run.echoes[0].phase_nifti).unwrap();
        let err = super::validate_run_dims(&run, &reference).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("Dimension mismatch"), "got: {}", msg);
        assert!(msg.contains("magnitude"), "got: {}", msg);
        assert!(msg.contains(&mag.display().to_string()), "got: {}", msg);
        assert!(msg.contains("4x4x3"), "got: {}", msg);
        assert!(msg.contains("4x4x4"), "got: {}", msg);
    }

    #[test]
    fn test_validate_run_dims_rejects_mismatched_later_echo() {
        let dir = tempfile::tempdir().unwrap();
        let phase1 = dir.path().join("echo1_phase.nii");
        let phase2 = dir.path().join("echo2_phase.nii");
        write_vol(&phase1, (4, 4, 4));
        write_vol(&phase2, (5, 4, 4));
        let run = run_with_echoes(vec![(phase1, None), (phase2.clone(), None)]);
        let reference = qsm_core::io::read_nifti_file(&run.echoes[0].phase_nifti).unwrap();
        let err = super::validate_run_dims(&run, &reference).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("echo 2 phase"), "got: {}", msg);
        assert!(msg.contains(&phase2.display().to_string()), "got: {}", msg);
    }

    // --- resolve_geometry: how an oblique acquisition is handled ---

    /// A real UK Biobank SWI affine: 0.8 x 0.8 x 3 mm, header "Tra>Cor(-22.9)".
    fn oblique_affine() -> [f64; 16] {
        [
            0.7976, -0.0273, -0.1075, -101.6,
            0.0141, 0.7358, -1.1647, -87.4,
            0.0370, 0.3092, 2.7626, -60.2,
            0.0, 0.0, 0.0, 1.0,
        ]
    }

    fn nifti_with(affine: [f64; 16], dims: (usize, usize, usize)) -> qsm_core::io::NiftiData {
        qsm_core::io::NiftiData {
            data: vec![0.0; dims.0 * dims.1 * dims.2],
            dims,
            voxel_size: qsm_core::geometry::voxel_sizes_from_affine(&affine),
            affine,
            scl_slope: 1.0,
            scl_inter: 0.0,
        }
    }

    fn run_for_geometry(b0_dir: Option<(f64, f64, f64)>) -> crate::bids::discovery::QsmRun {
        let mut run = run_with_echoes(vec![(std::path::PathBuf::from("p.nii"), None)]);
        run.b0_dir = b0_dir;
        run.echo_times = vec![0.004];
        run
    }

    #[test]
    fn a_gapped_2d_acquisition_is_refused_unless_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let json = dir.path().join("p.json");
        let mut run = run_for_geometry(None);
        run.echoes[0].phase_json = json.clone();
        let affine = [0.8, 0.0, 0.0, 0.0, 0.0, 0.8, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let phase = nifti_with(affine, (4, 4, 4));
        let mut cfg = crate::pipeline::config::PipelineConfig::default();
        let meta = super::resolve_run_geometry(&run, &phase, &cfg).unwrap();
        let write = |t: f64| std::fs::write(&json, format!(
            r#"{{"EchoTime": 0.004, "MagneticFieldStrength": 3, "SliceThickness": {t}, "MRAcquisitionType": "2D"}}"#)).unwrap();

        write(3.0); // contiguous: passed on to qsm-core
        assert_eq!(super::check_slice_gap(&run, &phase, &meta, &cfg).unwrap(), Some((3.0, 2)));
        write(2.0); // 1 mm gap
        let err = super::check_slice_gap(&run, &phase, &meta, &cfg).unwrap_err().to_string();
        assert!(err.contains("--ignore-slice-gap"), "{err}");
        cfg.pipeline.ignore_slice_gap = true;
        assert_eq!(super::check_slice_gap(&run, &phase, &meta, &cfg).unwrap(), None);
        cfg.pipeline.ignore_slice_gap = false;
        cfg.pipeline.do_qsm = false; // nothing that needs a contiguous grid
        assert_eq!(super::check_slice_gap(&run, &phase, &meta, &cfg).unwrap(), None);
        // And qsm-core refuses it too, given the geometry.
        let mut gapped = meta.clone();
        gapped.slice_geometry = Some((2.0, 2));
        assert!(qsmxt_engine::scan::scan_metadata(&gapped).require_contiguous_slices().is_err());
    }

    #[test]
    fn geometry_axial_run_is_untouched() {
        let mut cfg = crate::pipeline::config::PipelineConfig::default();
        cfg.pipeline.axial_resampling = true;
        cfg.pipeline.obliquity_threshold = 10.0;
        let affine = [
            0.8, 0.0, 0.0, 0.0, 0.0, 0.8, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let meta = super::resolve_run_geometry(&run_for_geometry(None), &nifti_with(affine, (4, 4, 4)), &cfg).unwrap();
        assert!(meta.source_geometry.is_none(), "an axial run must not be resampled");
        assert_eq!(meta.dims, (4, 4, 4));
        assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
    }

    #[test]
    fn geometry_oblique_without_threshold_rotates_the_kernel() {
        let cfg = crate::pipeline::config::PipelineConfig::default(); // threshold -1 (disabled)
        let meta = super::resolve_run_geometry(&run_for_geometry(None), &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
        assert!(meta.source_geometry.is_none(), "resampling is off by default");
        assert_eq!(meta.dims, (16, 16, 8), "grid is left alone");
        // B0 comes from the affine rather than being assumed along z.
        let (bx, by, bz) = meta.b0_direction;
        assert!((bx - 0.0463).abs() < 1e-3 && (by - 0.3871).abs() < 1e-3 && (bz - 0.9209).abs() < 1e-3,
                "B0 should follow the affine, got ({bx}, {by}, {bz})");
    }

    #[test]
    fn geometry_oblique_over_threshold_resamples_and_b0_becomes_z() {
        let mut cfg = crate::pipeline::config::PipelineConfig::default();
        cfg.pipeline.axial_resampling = true;
        cfg.pipeline.obliquity_threshold = 10.0; // obliquity here is ~32.5 degrees
        let meta = super::resolve_run_geometry(&run_for_geometry(None), &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
        let (src_dims, _) = meta.source_geometry.expect("should resample");
        assert_eq!(src_dims, (16, 16, 8), "source dims are recorded for scale_phase");
        assert!(meta.dims.0 >= 16 && meta.dims.1 >= 16 && meta.dims.2 >= 8,
                "the cardinal box is at least as large, got {:?}", meta.dims);
        assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0), "resampled grid puts B0 along z");
        assert!(qsm_core::geometry::obliquity_from_affine(&meta.affine) < 1e-9);
        // The stages (masking among them) run on the resampled grid, so that is the affine
        // qsm-core is given — not the oblique one the files were acquired on.
        assert_eq!(qsmxt_engine::scan::scan_metadata(&meta).affine, Some(meta.affine));
        assert_ne!(meta.affine, oblique_affine());
    }

    /// A crop of the working grid keeps its world position: voxel (0,0,0) of the box is voxel
    /// `origin` of the grid, so the box's affine maps it to the same point.
    #[test]
    fn scan_metadata_in_a_box_moves_the_affine_to_the_box() {
        let cfg = crate::pipeline::config::PipelineConfig::default();
        let meta = super::resolve_run_geometry(&run_for_geometry(None), &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
        let cb = qsm_core::crop::CropBox { origin: (2, -1, 3), dims: (8, 10, 4), full_dims: meta.dims };
        let m = qsmxt_engine::scan::scan_metadata_in(&meta, &cb);
        assert_eq!(m.dims, (8, 10, 4));
        let (a, b) = (meta.affine, m.affine.unwrap());
        let world = |aff: &[f64; 16], v: [f64; 3]| -> [f64; 3] {
            std::array::from_fn(|r| aff[4 * r] * v[0] + aff[4 * r + 1] * v[1] + aff[4 * r + 2] * v[2] + aff[4 * r + 3])
        };
        for (vb, vg) in [([0.0, 0.0, 0.0], [2.0, -1.0, 3.0]), ([1.0, 2.0, 3.0], [3.0, 1.0, 6.0])] {
            let (pb, pg) = (world(&b, vb), world(&a, vg));
            assert!(pb.iter().zip(pg).all(|(x, y)| (x - y).abs() < 1e-9), "{pb:?} vs {pg:?}");
        }
        assert_eq!(qsmxt_engine::scan::scan_metadata(&meta).affine, Some(meta.affine));
    }

    #[test]
    fn geometry_threshold_above_obliquity_does_not_resample() {
        let mut cfg = crate::pipeline::config::PipelineConfig::default();
        cfg.pipeline.axial_resampling = true;
        cfg.pipeline.obliquity_threshold = 45.0; // above this run's ~32.5 degrees
        let meta = super::resolve_run_geometry(&run_for_geometry(None), &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
        assert!(meta.source_geometry.is_none());
        assert!(meta.b0_direction.2 < 0.99, "still uses the affine direction");
    }

    #[test]
    fn geometry_sidecar_b0_wins_over_resampling() {
        let mut cfg = crate::pipeline::config::PipelineConfig::default();
        cfg.pipeline.axial_resampling = true;
        cfg.pipeline.obliquity_threshold = 10.0;
        let run = run_for_geometry(Some((0.0, 0.5, 0.866)));
        let meta = super::resolve_run_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
        assert!(meta.source_geometry.is_none(), "an explicit B0_dir means the grid is left alone");
        assert_eq!(meta.b0_direction, (0.0, 0.5, 0.866));
    }

    // --- reconciling a MESE's geometry with the grid the run reconstructs on ---











    #[test]
    fn robust_fit_report_sits_next_to_the_field_map() {
        let p = std::path::Path::new("/w/unwrap/sub-1_field-ppm.nii");
        assert_eq!(super::robust_fit_report_path(p), std::path::PathBuf::from("/w/unwrap/sub-1_field-ppm_robustfit.json"));
        let gz = std::path::Path::new("/w/unwrap/sub-1_field-ppm.nii.gz");
        assert_eq!(super::robust_fit_report_path(gz), std::path::PathBuf::from("/w/unwrap/sub-1_field-ppm_robustfit.json"));
    }



    /// A MESE attached to a run, on whatever grid is given.
    fn mese_on(dims: (usize, usize, usize), affine: [f64; 16]) -> crate::bids::discovery::MeseRun {
        crate::bids::discovery::MeseRun {
            key: crate::bids::entities::AcquisitionKey {
                subject: "1".to_string(), session: None, acquisition: None,
                reconstruction: None, inversion: None, run: None,
                suffix: "MESE".to_string(),
            },
            magnitude_niftis: vec![std::path::PathBuf::from("mese.nii")],
            echo_times: vec![0.01],
            dims, affine,
        }
    }

    /// Used to assert the opposite: a MESE made the oblique run keep its grid, because resampling
    /// the GRE alone would have stranded the MESE. The MESE is now resampled onto whatever grid
    /// this picks, so it no longer has a vote.
    #[test]
    fn geometry_mese_run_is_resampled_like_any_other() {
        let mut cfg = crate::pipeline::config::PipelineConfig::default();
        cfg.pipeline.axial_resampling = true;
        cfg.pipeline.obliquity_threshold = 10.0;
        let mut run = run_for_geometry(None);
        run.mese = Some(mese_on((16, 16, 8), oblique_affine()));
        let meta = super::resolve_run_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
        assert!(meta.source_geometry.is_some(),
                "a MESE no longer suppresses a resample the threshold asked for");
        assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
    }

    // --- an algorithm that only understands axial data outranks everything else ---

    /// A deep-learning inversion: no B0 direction to set, an axial prior baked into the weights.
    fn cfg_with_axial_only_inversion() -> crate::pipeline::config::PipelineConfig {
        let mut cfg = crate::pipeline::config::PipelineConfig::default();
        cfg.inversion.algorithm = crate::pipeline::config::QsmAlgorithm::Qsmnet;
        cfg
    }

    #[test]
    fn geometry_axial_only_algorithm_resamples_without_a_threshold() {
        let cfg = cfg_with_axial_only_inversion(); // threshold stays -1 (disabled)
        let meta = super::resolve_run_geometry(&run_for_geometry(None), &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
        assert!(meta.source_geometry.is_some(),
                "a network that assumes B0 is +z must be given axial data, threshold or not");
        assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
    }

    #[test]
    fn geometry_axial_only_algorithm_outranks_a_sidecar_b0() {
        let cfg = cfg_with_axial_only_inversion();
        let run = run_for_geometry(Some((0.0, 0.5, 0.866)));
        let meta = super::resolve_run_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
        assert!(meta.source_geometry.is_some(),
                "the sidecar describes the acquisition, but the network cannot be told about it");
        assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
    }

    #[test]
    fn geometry_axial_only_algorithm_leaves_axial_data_alone() {
        let cfg = cfg_with_axial_only_inversion();
        let affine = [
            0.8, 0.0, 0.0, 0.0, 0.0, 0.8, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let meta = super::resolve_run_geometry(&run_for_geometry(None), &nifti_with(affine, (4, 4, 4)), &cfg).unwrap();
        assert!(meta.source_geometry.is_none(), "already axial: nothing to fix");
        assert_eq!(meta.dims, (4, 4, 4));
    }

    /// Used to be refused outright: resampling would have stranded the MESE, not resampling would
    /// have fed the network oblique data. The third option it skipped — resample both — is what
    /// happens now, so the refusal is gone and the network still gets axial data.
    #[test]
    fn geometry_axial_only_algorithm_with_a_mese_resamples_both() {
        let cfg = cfg_with_axial_only_inversion();
        let mut run = run_for_geometry(None);
        run.mese = Some(mese_on((16, 16, 8), oblique_affine()));
        let meta = super::resolve_run_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg)
            .expect("resampling both is defensible, so this no longer refuses");
        assert!(meta.source_geometry.is_some(), "the network must still be given axial data");
        assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
        assert!(qsm_core::geometry::obliquity_from_affine(&meta.affine) < 1e-9);
    }

    /// The MESE being on a grid of its own — the usual case for a 2D spin-echo companion to a 3D
    /// GRE — changes nothing about the decision here. It used to mean the MESE was dropped
    /// entirely at load time, so R2' was never computed.
    #[test]
    fn geometry_mese_on_its_own_grid_does_not_change_the_decision() {
        let cfg = cfg_with_axial_only_inversion();
        let mut run = run_for_geometry(None);
        // Half the matrix, four times the slice thickness, and an axial affine of its own.
        run.mese = Some(mese_on((8, 8, 2), [
            1.6, 0.0, 0.0, 0.0, 0.0, 1.6, 0.0, 0.0, 0.0, 0.0, 12.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]));
        let meta = super::resolve_run_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg)
            .expect("the MESE's own grid is reconciled at load time, not here");
        assert!(meta.source_geometry.is_some());
        assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
    }

    /// #243: R2PRIMEnet was outside the axial-only gate, so an oblique run whose other stages all
    /// take B0 as a parameter was reconstructed on the acquired grid and the network was handed it.
    #[test]
    fn geometry_r2primenet_on_an_oblique_grid_forces_a_resample() {
        let mut cfg = crate::pipeline::config::PipelineConfig::default();
        cfg.pipeline.do_chi_separation = true;
        // χ-sep-iLSQR takes B0 as a parameter, so nothing else in this run wants axial data.
        cfg.separation.algorithm = crate::pipeline::config::SeparationAlgorithm::ChiSepIlsqr;
        qsmxt_engine::config::enforce_separation_dependencies(&mut cfg);
        // No MESE, so `--r2prime-strategy auto` predicts R2′ with R2PRIMEnet.
        let run = run_for_geometry(None);
        let meta = super::resolve_run_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
        assert!(meta.source_geometry.is_some(),
                "R2PRIMEnet assumes B0 is +z and takes no direction, so the data must move");
        assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
    }

    /// The same config with a MESE attached: `auto` measures R2′ = R2* − R2 instead, no network
    /// runs, and the run keeps its grid. A resample here would cost an interpolation for nothing.
    #[test]
    fn geometry_a_mese_removes_the_r2primenet_reason_to_resample() {
        let mut cfg = crate::pipeline::config::PipelineConfig::default();
        cfg.pipeline.do_chi_separation = true;
        cfg.separation.algorithm = crate::pipeline::config::SeparationAlgorithm::ChiSepIlsqr;
        qsmxt_engine::config::enforce_separation_dependencies(&mut cfg);
        let mut run = run_for_geometry(None);
        run.mese = Some(mese_on((16, 16, 8), oblique_affine()));
        let meta = super::resolve_run_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
        assert!(meta.source_geometry.is_none(),
                "nothing in this run assumes B0 is +z, so the kernel is rotated instead");
        assert!(meta.b0_direction.2 < 0.99, "B0 comes from the affine");
    }

    /// iQFM is iQSM's local-field head, and `map_bf_alg` has no iQFM to map to — it answers for a
    /// placeholder. The gate asks about iQSM instead, so an oblique run is resampled.
    #[test]
    fn geometry_iqfm_on_an_oblique_grid_forces_a_resample() {
        let mut cfg = crate::pipeline::config::PipelineConfig::default();
        cfg.bg_removal.algorithm = crate::pipeline::config::BfAlgorithm::Iqfm;
        let meta = super::resolve_run_geometry(&run_for_geometry(None), &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
        assert!(meta.source_geometry.is_some(),
                "iQFM learned the same axial prior as iQSM and takes no B0 direction");
        assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
    }

    #[test]
    fn geometry_classical_algorithm_needs_no_resampling() {
        let cfg = crate::pipeline::config::PipelineConfig::default(); // iLSQR by default
        assert!(qsmxt_engine::bridge::axial_only_algorithms(&cfg, Default::default()).is_empty(),
                "the default pipeline takes B0 as a parameter throughout");
    }
}

#[cfg(test)]
mod swi_export_tests {
    use super::*;

    const IDENTITY: [f64; 16] = [
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];

    fn key() -> crate::bids::entities::AcquisitionKey {
        crate::bids::entities::AcquisitionKey {
            subject: "01".into(), session: None, acquisition: None,
            reconstruction: None, inversion: None, run: None, suffix: "MEGRE".into(),
        }
    }

    fn run_for(key: crate::bids::entities::AcquisitionKey) -> crate::bids::discovery::QsmRun {
        crate::bids::discovery::QsmRun {
            key,
            coils: None,
            echoes: vec![crate::bids::discovery::EchoFiles {
                echo_number: 1,
                phase_json: PathBuf::from("p.json"),
                phase_nifti: PathBuf::from("p.nii"),
                magnitude_json: None,
                imaginary_nifti: None,
                magnitude_nifti: None,
            }],
            magnetic_field_strength: 3.0,
            echo_times: vec![0.004],
            b0_dir: None,
            dims: (0, 0, 0),
            has_magnitude: true,
            mese: None,
        }
    }

    fn meta_for(dims: (usize, usize, usize), affine: [f64; 16]) -> RunMetadata {
        RunMetadata {
            dims,
            voxel_size: qsm_core::geometry::voxel_sizes_from_affine(&affine),
            affine,
            n_echoes: 1,
            echo_times: vec![0.004],
            b0_direction: (0.0, 0.0, 1.0),
            field_strength: 3.0,
            has_magnitude: true,
            source_geometry: None, slice_geometry: None,
        }
    }

    /// Two echoes where the head moved 2 mm along x between them: the stage recovers the shift,
    /// puts echo 2 back over echo 1, and the later stages read the corrected echoes and the
    /// shrunk mask. Off, or without magnitude, it does what it says.
    #[test]
    fn motion_correction_stage() {
        let dims = (32, 32, 24);
        let (nx, ny, nz) = dims;
        let blob = |shift: f64| -> Vec<f64> {
            let mut v = vec![0.0; nx * ny * nz];
            for k in 0..nz { for j in 0..ny { for i in 0..nx {
                let (x, y, z) = (i as f64 - 15.5 - shift, j as f64 - 15.5, k as f64 - 11.5);
                v[i + j * nx + k * nx * ny] = 100.0 * (-(x * x / 40.0 + y * y / 25.0 + z * z / 15.0)).exp()
                    + 30.0 * (-((x - 4.0).powi(2) + (y + 3.0).powi(2) + z * z) / 6.0).exp();
            }}}
            v
        };
        let dir = tempfile::tempdir().unwrap();
        let k = key();
        let mut run = run_for(k.clone());
        run.echoes.push(run.echoes[0].clone());
        run.echo_times = vec![0.004, 0.008];
        let output = DerivativeOutputs::new(dir.path());
        let mut meta = meta_for(dims, IDENTITY);
        meta.n_echoes = 2;
        meta.echo_times = vec![0.004, 0.008];
        let mask_path = output.mask_path(&k);
        let vs = meta.voxel_size;
        for (e, shift) in [(1, 0.0), (2, 2.0)] {
            write_volume(&output.mag_path(&k, e), &blob(shift), dims, vs, &IDENTITY).unwrap();
            write_volume(&output.phase_scaled_path(&k, e), &vec![0.3 * e as f64; nx * ny * nz], dims, vs, &IDENTITY).unwrap();
        }
        let brain: Vec<f64> = blob(0.0).iter().map(|&v| (v > 5.0) as u8 as f64).collect();
        write_volume(&mask_path, &brain, dims, vs, &IDENTITY).unwrap();

        let mut config = PipelineConfig::default();
        run_engine_step(&run, &config, &output, &meta, Stage::Motion).unwrap();
        assert!(!output.motion_coverage_path(&k).exists(), "off: no coverage mask");

        config.motion_correction.enabled = true;
        run_engine_step(&run, &config, &output, &meta, Stage::Motion).unwrap();
        let shrunk = output.motion_coverage_path(&k);
        let report: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(output.motion_report_path(&k)).unwrap()).unwrap();
        let t = report["echoes"][1]["translation_mm"].as_f64().unwrap();
        assert!((t - 2.0).abs() < 0.2, "recovered translation {t} mm, report {report}");
        assert_eq!(report["echoes"][0]["translation_mm"].as_f64().unwrap(), 0.0);
        // Echo 2 now sits over echo 1 inside the brain.
        let (m1, m2) = (load_volume(&output.motion_mag_path(&k, 1)).unwrap(), load_volume(&output.motion_mag_path(&k, 2)).unwrap());
        let mask = load_mask(&shrunk).unwrap();
        let err = m1.iter().zip(&m2).zip(&mask).filter(|(_, &m)| m != 0).map(|((a, b), _)| (a - b).abs()).fold(0.0, f64::max);
        assert!(err < 5.0, "max magnitude difference after correction {err}");

        // Off again: the coverage mask it left is not this run's.
        config.motion_correction.enabled = false;
        run_engine_step(&run, &config, &output, &meta, Stage::Motion).unwrap();
        assert!(!shrunk.exists(), "a stale coverage mask stays behind");

        config.motion_correction.enabled = true;
        let mut no_mag = meta.clone();
        no_mag.has_magnitude = false;
        let e = run_engine_step(&run, &config, &output, &no_mag, Stage::Motion).unwrap_err().to_string();
        assert!(e.contains("magnitude"), "{e}");
    }

    /// Something with structure in every direction, so a projection is not trivially constant.
    fn textured(dims: (usize, usize, usize)) -> Vec<f64> {
        let (nx, ny, nz) = dims;
        let mut data = vec![0.0; nx * ny * nz];
        for k in 0..nz {
            for j in 0..ny {
                for i in 0..nx {
                    let (x, y, z) = (i as f64, j as f64, k as f64);
                    data[i + j * nx + k * nx * ny] =
                        100.0 + 20.0 * (x * 0.7).sin() + 15.0 * (y * 0.4).cos() + 10.0 * (z * 0.9).sin();
                }
            }
        }
        data
    }

    /// Run the SWI step end to end over synthetic inputs on the given grid.
    fn run_swi_stage(
        dir: &Path,
        dims: (usize, usize, usize),
        affine: [f64; 16],
        window: usize,
    ) -> (crate::Result<()>, DerivativeOutputs, crate::bids::entities::AcquisitionKey) {
        let k = key();
        let run = run_for(k.clone());
        let output = DerivativeOutputs::new(dir);
        // Two echoes: CLEAR-SWI reads every one.
        let mut meta = meta_for(dims, affine);
        meta.n_echoes = 2;
        meta.echo_times = vec![0.004, 0.008];

        let mut config = PipelineConfig::default();
        config.swi.mip_window = window;

        let phase: Vec<f64> = textured(dims).iter().map(|v| (v * 0.01).sin()).collect();
        for e in 1..=meta.n_echoes {
            write_volume(&output.phase_scaled_path(&k, e), &phase, dims, meta.voxel_size, &affine).unwrap();
            write_volume(&output.mag_path(&k, e), &textured(dims), dims, meta.voxel_size, &affine).unwrap();
        }

        let result = run_engine_step(&run, &config, &output, &meta, Stage::Swi);
        (result, output, k)
    }

    /// The projection a viewer should see: a sliding minimum over `window` slices.
    fn expected_mip(swi: &[f64], dims: (usize, usize, usize), window: usize) -> Vec<f64> {
        let (nx, ny, nz) = dims;
        let nxy = nx * ny;
        let mut out = Vec::with_capacity(nxy * (nz - window + 1));
        for k in 0..=(nz - window) {
            for v in 0..nxy {
                out.push((0..window).map(|w| swi[v + (k + w) * nxy]).fold(f64::INFINITY, f64::min));
            }
        }
        out
    }

    /// The whole export path, not just `create_mip`: a header-only load hid this (issue #211).
    #[test]
    fn minip_header_matches_its_payload() {
        let dir = tempfile::tempdir().unwrap();
        let dims = (12, 10, 32);
        let (result, output, k) = run_swi_stage(dir.path(), dims, IDENTITY, 7);
        result.unwrap();

        let mip = qsm_core::io::read_nifti_file(&output.swi_mip_path(&k)).unwrap();
        assert_eq!(mip.dims, (12, 10, 26));
        // Every voxel the header promises is really there.
        assert_eq!(mip.data.len(), 12 * 10 * 26);

        // The file on disk holds exactly the payload, not the full-volume length.
        let bytes = std::fs::metadata(output.swi_mip_path(&k)).unwrap().len();
        assert_eq!(bytes, 352 + (12 * 10 * 26 * 4) as u64);

        // And it is the projection of the SWI that was saved beside it, zeros included.
        let swi = qsm_core::io::read_nifti_file(&output.swi_path(&k)).unwrap();
        assert_eq!(swi.dims, dims);
        let expected = expected_mip(&swi.data, dims, 7);
        for (i, (got, want)) in mip.data.iter().zip(expected.iter()).enumerate() {
            assert!((got - want).abs() < 1e-3, "voxel {i}: {got} != {want}");
        }
    }

    #[test]
    fn minip_origin_sits_at_the_slab_centre() {
        let dir = tempfile::tempdir().unwrap();
        let mut affine = IDENTITY;
        affine[10] = 3.0; // 3 mm slices
        affine[11] = -60.0;
        let (result, output, k) = run_swi_stage(dir.path(), (8, 8, 24), affine, 7);
        result.unwrap();

        let mip = qsm_core::io::read_nifti_file(&output.swi_mip_path(&k)).unwrap();
        assert_eq!(mip.dims, (8, 8, 18));
        // Three 3 mm slices along k from the SWI origin.
        assert!((mip.affine[11] - (-51.0)).abs() < 1e-3, "{:?}", mip.affine);
        // The SWI itself keeps the acquisition geometry.
        let swi = qsm_core::io::read_nifti_file(&output.swi_path(&k)).unwrap();
        assert!((swi.affine[11] - (-60.0)).abs() < 1e-3, "{:?}", swi.affine);
    }

    #[test]
    fn a_window_of_one_keeps_the_full_depth() {
        let dir = tempfile::tempdir().unwrap();
        let (result, output, k) = run_swi_stage(dir.path(), (8, 8, 10), IDENTITY, 1);
        result.unwrap();
        let mip = qsm_core::io::read_nifti_file(&output.swi_mip_path(&k)).unwrap();
        let swi = qsm_core::io::read_nifti_file(&output.swi_path(&k)).unwrap();
        assert_eq!(mip.dims, (8, 8, 10));
        assert_eq!(mip.data, swi.data);
    }

    #[test]
    fn a_window_of_the_full_depth_leaves_one_slice() {
        let dir = tempfile::tempdir().unwrap();
        let (result, output, k) = run_swi_stage(dir.path(), (8, 8, 10), IDENTITY, 10);
        result.unwrap();
        let mip = qsm_core::io::read_nifti_file(&output.swi_mip_path(&k)).unwrap();
        assert_eq!(mip.dims, (8, 8, 1));
        assert_eq!(mip.data.len(), 64);
    }

    #[test]
    fn a_window_deeper_than_the_volume_is_refused_before_any_work() {
        let dir = tempfile::tempdir().unwrap();
        let (result, output, k) = run_swi_stage(dir.path(), (8, 8, 10), IDENTITY, 11);
        let err = result.unwrap_err().to_string();
        assert!(err.contains("deeper than the 10-slice volume"), "{err}");
        // create_mip would have returned an empty vector here; nothing must reach disk.
        assert!(!output.swi_mip_path(&k).exists());
        assert!(!output.swi_path(&k).exists());
    }

    #[test]
    fn a_zero_window_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let (result, _, _) = run_swi_stage(dir.path(), (8, 8, 10), IDENTITY, 0);
        let err = result.unwrap_err().to_string();
        assert!(err.contains("at least 1 slice"), "{err}");
    }

    /// An SWI cached when the stage used the first echo only must be recomputed with every echo.
    #[test]
    fn a_first_echo_swi_cache_is_rebuilt() {
        let dir = tempfile::tempdir().unwrap();
        let (result, output, k) = run_swi_stage(dir.path(), (8, 8, 16), IDENTITY, 7);
        result.unwrap();
        let d = PipelineConfig::default().swi;
        let first_echo_key = serde_json::json!({
            "scaling": d.scaling, "strength": d.strength, "hp_sigma": d.hp_sigma,
            "mip_window": 7, "mip_dims": [8, 8, 10],
        });
        let first_echo_hash = crate::pipeline::graph::step_params_hash(Some("clear-swi"), &first_echo_key);
        let state: PipelineState = serde_json::from_str(
            &std::fs::read_to_string(output.state_path(&k)).unwrap()).unwrap();
        assert_ne!(state.completed_steps.get("swi").unwrap().params_hash.as_deref(), Some(first_echo_hash.as_str()));
    }

    /// A minIP written with full-volume dimensions must not survive as a cache hit.
    #[test]
    fn a_stale_minip_cache_is_rebuilt() {
        let dir = tempfile::tempdir().unwrap();
        let dims = (8, 8, 16);
        let (result, output, k) = run_swi_stage(dir.path(), dims, IDENTITY, 7);
        result.unwrap();

        // Replay the pre-fix parameter set: same window, no minIP dimensions recorded.
        let stale = serde_json::json!({
            "scaling": PipelineConfig::default().swi.scaling,
            "strength": PipelineConfig::default().swi.strength,
            "hp_sigma": PipelineConfig::default().swi.hp_sigma,
            "mip_window": 7,
        });
        let stale_hash = crate::pipeline::graph::step_params_hash(Some("clear-swi"), &stale);
        let mut state: PipelineState =
            serde_json::from_str(&std::fs::read_to_string(output.state_path(&k)).unwrap()).unwrap();
        assert_ne!(
            state.completed_steps.get("swi").unwrap().params_hash.as_deref(),
            Some(stale_hash.as_str()),
            "the fix must change the swi cache key so old minIPs are regenerated",
        );

        // With the old hash stored, the step is not treated as cached.
        state.completed_steps.get_mut("swi").unwrap().params_hash = Some(stale_hash);
        assert!(!state.is_step_cached_with_hash(
            "swi",
            Some(&crate::pipeline::graph::step_params_hash(
                Some("clear-swi"),
                &serde_json::json!({
                    "scaling": PipelineConfig::default().swi.scaling,
                    "strength": PipelineConfig::default().swi.strength,
                    "hp_sigma": PipelineConfig::default().swi.hp_sigma,
                    "mip_window": 7,
                    "mip_dims": [8, 8, 10],
                }),
            )),
        ));
    }

}

/// End-to-end runs of an oblique acquisition resampled to axial (issue #223).
#[cfg(test)]
mod oblique_run_tests {
    use super::*;
    use crate::bids::entities::AcquisitionKey;

    const DIMS: (usize, usize, usize) = (20, 22, 18);

    fn key(acq: &str) -> AcquisitionKey {
        AcquisitionKey {
            subject: "1".into(), session: None, acquisition: Some(acq.into()),
            reconstruction: None, inversion: None, run: None, suffix: "MEGRE".into(),
        }
    }

    /// A small multi-echo GRE of a sphere, 1 mm isotropic and tilted 30 degrees about x.
    fn oblique_run(dir: &Path, acq: &str, n_echoes: usize) -> QsmRun {
        let (nx, ny, nz) = DIMS;
        let (c, s) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
        let affine = [
            1.0, 0.0, 0.0, -10.0,
            0.0, c, -s, -11.0,
            0.0, s, c, -9.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        let mut echoes = Vec::new();
        for e in 0..n_echoes {
            let (mut mag, mut phase) = (vec![0.0; nx * ny * nz], vec![0.0; nx * ny * nz]);
            for k in 0..nz {
                for j in 0..ny {
                    for i in 0..nx {
                        let v = i + j * nx + k * nx * ny;
                        let (x, y, z) = (i as f64 - 10.0, j as f64 - 11.0, k as f64 - 9.0);
                        let r = (x * x + y * y + z * z).sqrt();
                        mag[v] = if r < 7.0 { 100.0 * (-0.02 * (e + 1) as f64).exp() + x } else { 1.0 };
                        phase[v] = (0.05 * (e + 1) as f64 * (x + 0.5 * y)).sin() * 3.0;
                    }
                }
            }
            let p = dir.join(format!("acq-{acq}_echo{}_phase.nii", e + 1));
            let m = dir.join(format!("acq-{acq}_echo{}_mag.nii", e + 1));
            io::save_nifti_to_file(&p, &phase, DIMS, (1.0, 1.0, 1.0), &affine).unwrap();
            io::save_nifti_to_file(&m, &mag, DIMS, (1.0, 1.0, 1.0), &affine).unwrap();
            echoes.push((p, Some(m)));
        }
        let mut run = super::tests::run_with_echoes(echoes);
        run.key = key(acq);
        run.dims = DIMS;
        run
    }

    /// The default pipeline with RTS for the inversion. These tests are about grids, caching and
    /// outputs, not the inversion; with the default (iLSQR) they took the suite from ~2 min to
    /// ~50 min under coverage.
    fn fast_config() -> PipelineConfig {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Rts;
        config
    }

    fn resampling_config() -> PipelineConfig {
        let mut config = fast_config();
        config.pipeline.axial_resampling = true;
        config.pipeline.obliquity_threshold = 5.0;
        config
    }

    fn voxels(path: &Path) -> usize {
        io::read_nifti_file(path).unwrap().data.len()
    }

    const ACQUIRED: usize = DIMS.0 * DIMS.1 * DIMS.2;

    /// Each run's methods text, record and planned graph (Mermaid and SVG) go in its workflow
    /// directory. Once the runs are done, the dataset's methods and graph come from every current
    /// record in the output directory: `--use-custom-masks` with no mask found for any run is said
    /// as done ("where available" no longer applies), and no run differs. A run that decided
    /// otherwise than the rest gets its methods text and graph in `changes/`. Processing one run
    /// again (`--include`) keeps the others in the dataset; a run processed with other settings is
    /// not one of its runs; `changes/` goes when no run differs.
    #[test]
    fn the_datasets_methods_come_from_its_runs_and_a_run_that_differs_is_kept_in_changes() {
        use qsmxt_engine::record::{Choice, Decision, Topic};
        let dir = tempfile::tempdir().unwrap();
        let runs: Vec<QsmRun> = ["a", "b", "c", "d"].iter().map(|acq| oblique_run(dir.path(), acq, 2)).collect();
        let mut config = fast_config();
        config.masking.custom_mask_tool = Some("nothing-here".into());
        let derivatives = dir.path().join("out");
        let output = DerivativeOutputs::new(&derivatives);
        std::fs::create_dir_all(&derivatives).unwrap();
        crate::commands::run::write_dataset_files(&derivatives, dir.path(), &config, &runs[..3]).unwrap();
        assert!(output.dataset_graph_path().with_extension("svg").exists());
        for run in &runs[..3] {
            run_pipeline_cached(run, &config, &output, true, &|_| {}).unwrap();
            let key = &run.key;
            for path in [output.run_methods_path(key), output.run_record_path(key), output.run_graph_path(key, false),
                         output.run_graph_path(key, false).with_extension("svg")] {
                assert!(path.exists() && path.starts_with(derivatives.join("workflow")), "{}", path.display());
            }
        }
        let summarise = || crate::commands::run::write_dataset_summary(&derivatives, &config).unwrap();
        let read = |p: std::path::PathBuf| std::fs::read_to_string(p).unwrap();
        summarise();
        let methods = read(derivatives.join("methods.md"));
        assert!(!methods.contains("Where available") && methods.contains("A brain mask was generated"), "{methods}");
        assert!(crate::pipeline::methods::generate_methods(&config).contains("Where available"));
        assert_eq!(methods, read(output.run_methods_path(&runs[0].key)));
        assert!(!output.changes_dir().exists());
        let graph = read(output.dataset_graph_path());
        assert!(graph.contains("All 3 runs: "), "{graph}");

        // Run c decided otherwise (as if its supplied mask had been used): it alone differs.
        let c = &runs[2].key;
        let mut file: RunRecordFile = serde_json::from_str(&read(output.run_record_path(c))).unwrap();
        let mask = file.record.steps.iter_mut().find(|s| s.id == "mask").unwrap();
        mask.decisions = vec![Decision::new(Topic::MaskSource, Choice::Supplied, "Taken from the supplied mask.")];
        std::fs::write(output.run_record_path(c), serde_json::to_string(&file).unwrap()).unwrap();
        summarise();
        let methods = read(derivatives.join("methods.md"));
        assert!(methods.contains("Where available"), "{methods}");
        let changes = || -> Vec<String> {
            let mut v: Vec<String> = std::fs::read_dir(output.changes_dir()).map(|d| d.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
            v.sort();
            v
        };
        assert_eq!(changes().len(), 3, "{:?}", changes());
        let cname = c.basename();
        assert!(output.change_path(&cname, false).exists() && output.change_path(&cname, true).with_extension("svg").exists());
        assert!(read(output.change_path(&cname, false)).contains("sourced from"));
        let before = (methods.clone(), read(output.dataset_graph_path()), changes());

        // Run a alone again (`--include`): the dataset is still a, b and c, and c's files stay.
        run_pipeline_cached(&runs[0], &config, &output, false, &|_| {}).unwrap();
        summarise();
        assert_eq!((read(derivatives.join("methods.md")), read(output.dataset_graph_path()), changes()), before);

        // Run d processed with other settings, and a run whose latest attempt did not finish, are
        // not current: neither counts.
        let mut other = config.clone();
        other.masking.custom_mask_tool = None;
        run_pipeline_cached(&runs[3], &other, &output, true, &|_| {}).unwrap();
        assert!(output.run_record_path(&runs[3].key).exists());
        let b_state = crate::pipeline::graph::state_file_path(&derivatives, &runs[1].key);
        let saved = read(b_state.clone());
        std::fs::write(&b_state, saved.replace("\"complete\"", "\"running\"")).unwrap();
        summarise();
        let graph = read(output.dataset_graph_path());
        assert!(graph.contains("1 of 2: supplied (nothing-here)"), "{graph}");
        std::fs::write(&b_state, saved).unwrap();

        // Run c again: it is like the rest, and changes/ goes.
        run_pipeline_cached(&runs[2], &config, &output, false, &|_| {}).unwrap();
        summarise();
        assert!(!output.changes_dir().exists());
        assert!(read(output.dataset_graph_path()).contains("All 3 runs: "));
    }

    /// ARLO and chi-separation read the acquired-grid source magnitudes and indexed past their end.
    #[test]
    fn r2star_is_computed_on_the_working_grid() {
        let dir = tempfile::tempdir().unwrap();
        let run = oblique_run(dir.path(), "a", 3);
        let mut config = resampling_config();
        config.pipeline.do_t2starmap = true;
        config.pipeline.do_r2starmap = true;
        let output = DerivativeOutputs::new(&dir.path().join("out"));
        run_pipeline_cached(&run, &config, &output, true, &|_| {}).unwrap();
        assert_eq!(voxels(&output.r2star_path(&run.key)), ACQUIRED);
    }

    /// Returning the outputs to the acquired grid rewrote the mask and magnitude in place, so a
    /// re-run that recomputed the inversion cropped acquired-grid data against the working grid.
    #[test]
    fn a_rerun_after_returning_to_the_acquired_grid_reads_the_working_grid() {
        let dir = tempfile::tempdir().unwrap();
        let run = oblique_run(dir.path(), "a", 3);
        let output = DerivativeOutputs::new(&dir.path().join("out"));
        let mut config = resampling_config();
        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();

        config.inversion.algorithm = QsmAlgorithm::Tkd;
        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
        assert_eq!(voxels(&output.qsm_path(&run.key)), ACQUIRED);
        assert_eq!(voxels(&output.mask_path(&run.key)), ACQUIRED);

        // Asking for the working grid on a re-run brings the working-grid outputs back.
        config.pipeline.output_space = crate::pipeline::config::OutputSpace::Working;
        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
        // ...under their working-space names (#275).
        assert_ne!(voxels(&output.published(&run.key).qsm_path(&run.key)), ACQUIRED);
        assert!(!output.qsm_path(&run.key).exists(), "the acquired-grid map was left behind");
    }

    /// Runs of one session share `anat/`, so returning one run to the acquired grid must leave the
    /// others alone — they may be mid-reconstruction on their own working grid.
    #[test]
    fn returning_to_the_acquired_grid_leaves_other_runs_alone() {
        let dir = tempfile::tempdir().unwrap();
        let output = DerivativeOutputs::new(&dir.path().join("out"));
        let a = oblique_run(dir.path(), "a", 1);
        let mut working = resampling_config();
        working.pipeline.output_space = crate::pipeline::config::OutputSpace::Working;
        run_pipeline_cached(&a, &working, &output, false, &|_| {}).unwrap();
        let a_mask = output.published(&a.key).mask_path(&a.key);
        let a_working = voxels(&a_mask);
        assert_ne!(a_working, ACQUIRED);

        // `acq-a_run-2` starts with `acq-a`'s basename, so it is the harder neighbour to tell apart.
        let mut b = oblique_run(dir.path(), "a", 1);
        b.key.run = Some("2".into());
        run_pipeline_cached(&b, &resampling_config(), &output, false, &|_| {}).unwrap();
        assert_eq!(voxels(&output.mask_path(&b.key)), ACQUIRED);
        assert_eq!(voxels(&a_mask), a_working, "run a's mask was resampled by run b");
    }

    /// Every published image carries a sidecar whose `GeneratedBy` names an activity that is in
    /// `prov/`, and the provenance records QSM.rs by its commit rather than "unknown" (#278).
    /// Resampled back to the acquired grid, two-pass, with SWI/SMWI and R2*, so the steps that
    /// rewrite and combine maps are all exercised.
    #[test]
    fn every_published_image_has_resolvable_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let run = oblique_run(dir.path(), "a", 3);
        let mut config = resampling_config();
        config.pipeline.do_t2starmap = true;
        config.pipeline.do_r2starmap = true;
        config.pipeline.do_swi = true;
        config.pipeline.do_smwi = true;
        config.masking.two_pass = true;
        let deriv = dir.path().join("out");
        let output = DerivativeOutputs::new(&deriv);
        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
        crate::bids::bidsprov::write_provenance(&deriv, std::slice::from_ref(&run), &[], &output, "qsmxt run")
            .unwrap();

        let read = |p: &Path| -> serde_json::Value {
            serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
        };
        let act = read(&deriv.join("prov/prov-qsmxt_act.json"));
        let ent = read(&deriv.join("prov/prov-qsmxt_ent.json"));
        let mut images = 0;
        for entry in std::fs::read_dir(output.anat_dir(&run.key)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "nii") {
                continue;
            }
            images += 1;
            let sidecar = path.with_extension("json");
            assert!(sidecar.exists(), "{} has no sidecar", path.display());
            let meta = read(&sidecar);
            let by = meta["GeneratedBy"].as_str()
                .unwrap_or_else(|| panic!("{} has no GeneratedBy", sidecar.display()));
            assert!(act.get(by).is_some(), "{}: GeneratedBy {by} is not in prov/", sidecar.display());
            assert_eq!(meta["QSMrsVersion"], crate::bids::bidsprov::QSM_RS_VERSION);
        }
        assert!(images >= 10, "only {images} published images: the run did not exercise enough");
        // Every GeneratedBy an entity names resolves too.
        for (iri, e) in ent.as_object().unwrap() {
            if let Some(by) = e["GeneratedBy"].as_str() {
                assert!(act.get(by).is_some(), "entity {iri}: GeneratedBy {by} is not in prov/");
            }
        }
        // The resampled maps were used from their working-grid originals, which are linked to the
        // activities that made them.
        let resample = &act[format!("bids::prov/#output_space-{}", run.key.basename())];
        assert!(resample["Used"].as_array().unwrap().len() > 1, "{resample}");
        for dir_entry in std::fs::read_dir(deriv.join("prov")).unwrap() {
            let text = std::fs::read_to_string(dir_entry.unwrap().path()).unwrap();
            assert!(!text.contains("unknown"), "provenance still says unknown");
        }
        let soft = read(&deriv.join("prov/prov-qsmxt_soft.json"));
        let commit = env!("QSM_CORE_GIT_HASH");
        assert!(soft.as_object().unwrap().values().any(|a| a["Label"] == "QSM.rs"
            && a["CodeURL"].as_str().is_some_and(|u| u.ends_with(commit))), "{soft}");
    }

    /// A run where the head moved between echoes, reaching the edge of the field of view: after
    /// motion correction some brain voxels are not covered by every echo. Background removal, the
    /// inversion, both passes of a two-pass run and the published support all stay inside that
    /// coverage, which is published (#277) — they used the full brain mask, where the field was
    /// never computed. Switching motion correction off again re-runs what read it.
    #[test]
    fn reconstruction_stays_inside_the_motion_coverage() {
        let dims = (24usize, 20usize, 16usize);
        let (nx, ny, nz) = dims;
        let affine = [1.0, 0.0, 0.0, -12.0, 0.0, 1.0, 0.0, -10.0, 0.0, 0.0, 1.0, -8.0, 0.0, 0.0, 0.0, 1.0];
        let dir = tempfile::tempdir().unwrap();
        let mut echoes = Vec::new();
        for e in 0..3 {
            // The head slides 3 mm along x over the echoes, and fills x from edge to edge.
            let shift = 1.5 * e as f64;
            let (mut mag, mut phase) = (vec![0.0; nx * ny * nz], vec![0.0; nx * ny * nz]);
            for k in 0..nz { for j in 0..ny { for i in 0..nx {
                let v = i + j * nx + k * nx * ny;
                let (x, y, z) = (i as f64 - 11.5 - shift, j as f64 - 9.5, k as f64 - 7.5);
                let inside = (y * y / 64.0 + z * z / 36.0) < 1.0;
                mag[v] = if inside { 100.0 + 20.0 * (0.5 * x).sin() + 15.0 * (0.6 * y).cos() + 5.0 * z } else { 1.0 };
                phase[v] = (0.04 * (e + 1) as f64 * (x + 0.5 * y)).sin() * 2.0;
            }}}
            let p = dir.path().join(format!("echo{}_phase.nii", e + 1));
            let m = dir.path().join(format!("echo{}_mag.nii", e + 1));
            io::save_nifti_to_file(&p, &phase, dims, (1.0, 1.0, 1.0), &affine).unwrap();
            io::save_nifti_to_file(&m, &mag, dims, (1.0, 1.0, 1.0), &affine).unwrap();
            echoes.push((p, Some(m)));
        }
        let mut run = super::tests::run_with_echoes(echoes);
        run.key = key("motion");
        run.dims = dims;
        let output = DerivativeOutputs::new(&dir.path().join("out"));
        let mut config = fast_config();
        config.motion_correction.enabled = true;
        config.masking.two_pass = true;
        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();

        let k = &run.key;
        let count = |m: &[u8]| m.iter().filter(|&&v| v != 0).count();
        let brain = load_mask(&output.mask_path(k)).unwrap();
        let coverage_path = output.motion_coverage_path(k);
        assert!(coverage_path.exists(), "the coverage mask is published");
        let coverage = load_mask(&coverage_path).unwrap();
        assert!(count(&coverage) < count(&brain),
                "the fixture has to lose coverage: {} of {}", count(&coverage), count(&brain));
        let inside = |name: &str, m: &[u8]| {
            let outside = m.iter().zip(&coverage).filter(|(&v, &c)| v != 0 && c == 0).count();
            assert_eq!(outside, 0, "{name} has {outside} voxels outside the motion coverage");
        };
        inside("background-removal mask", &load_mask(&output.bg_mask_path(k)).unwrap());
        inside("reliable-pass mask", &load_mask(&output.two_pass_mask_path(k)).unwrap());
        inside("reliable-pass background-removal mask", &load_mask(&output.reliable_bg_mask_path(k)).unwrap());
        inside("published support", &load_mask(&output.qsm_support_path(k)).unwrap());
        let chi: Vec<u8> = load_volume(&output.chi_raw_path(k)).unwrap().iter().map(|&c| u8::from(c != 0.0)).collect();
        inside("raw χ", &chi);

        // Motion correction off: the coverage mask is withdrawn and reconstruction runs on the
        // brain mask again, rather than reusing what was cached against the coverage.
        config.motion_correction.enabled = false;
        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
        assert!(!coverage_path.exists(), "a stale coverage mask stays published");
        let bg = load_mask(&output.bg_mask_path(k)).unwrap();
        assert!(bg.iter().zip(&coverage).any(|(&v, &c)| v != 0 && c == 0),
                "background removal still ran on the coverage mask after motion correction was switched off");
    }

    /// Replacing an input file in place recomputes the run from `load` (#272); re-running without
    /// touching anything recomputes nothing.
    #[test]
    fn touching_an_input_recomputes_the_run() {
        let dir = tempfile::tempdir().unwrap();
        let run = oblique_run(dir.path(), "a", 2);
        let output = DerivativeOutputs::new(&dir.path().join("out"));
        let config = fast_config();
        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
        let mtime = |p: PathBuf| std::fs::metadata(p).unwrap().modified().unwrap();
        let first = mtime(output.qsm_path(&run.key));
        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
        assert_eq!(mtime(output.qsm_path(&run.key)), first, "an untouched re-run recomputed χ");

        let phase = &run.echoes[1].phase_nifti;
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(phase, std::fs::read(phase).unwrap()).unwrap();
        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
        assert_ne!(mtime(output.qsm_path(&run.key)), first, "a replaced input did not recompute χ");
    }

    /// `--clean-intermediates` treated the output-space step's files, which are the final
    /// outputs, as intermediates and deleted them.
    #[test]
    fn cleaning_intermediates_keeps_the_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let run = oblique_run(dir.path(), "a", 1);
        let output = DerivativeOutputs::new(&dir.path().join("out"));
        run_pipeline_cached(&run, &resampling_config(), &output, false, &|_| {}).unwrap();
        crate::commands::run::clean_run(&output, &run);
        assert_eq!(voxels(&output.qsm_path(&run.key)), ACQUIRED);
        assert!(!output.working_grid_dir(&run.key).exists());
        // Re-run with the same configuration: nothing is recomputed, and nothing is lost for want
        // of the working-grid copies that cleaning removed.
        let mtime = |p: PathBuf| std::fs::metadata(p).unwrap().modified().unwrap();
        let before = mtime(output.qsm_path(&run.key));
        run_pipeline_cached(&run, &resampling_config(), &output, false, &|_| {}).unwrap();
        assert_eq!(mtime(output.qsm_path(&run.key)), before, "the re-run recomputed χ");
        assert_eq!(voxels(&output.qsm_path(&run.key)), ACQUIRED);
        // A changed configuration recomputes what it needs, and lands on the acquired grid again.
        let mut tkd = resampling_config();
        tkd.inversion.algorithm = QsmAlgorithm::Tkd;
        run_pipeline_cached(&run, &tkd, &output, false, &|_| {}).unwrap();
        assert_ne!(mtime(output.qsm_path(&run.key)), before);
        assert_eq!(voxels(&output.qsm_path(&run.key)), ACQUIRED);
    }

    /// QSMART's intermediates land in each pass's own step directory, are recorded as that step's
    /// outputs, and go with the other intermediates when they are cleaned.
    #[test]
    fn qsmart_keeps_its_intermediates_in_each_pass_step() {
        let dir = tempfile::tempdir().unwrap();
        let run = oblique_run(dir.path(), "a", 3);
        let output = DerivativeOutputs::new(&dir.path().join("out"));
        let mut config = fast_config();
        config.inversion.algorithm = QsmAlgorithm::Qsmart;
        config.masking.two_pass = true;
        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();

        let names = [
            qsm_core::pipeline::qsmart::outputs::VESSEL_MASK,
            qsm_core::pipeline::qsmart::outputs::LOCAL_FIELD_STAGE1,
            qsm_core::pipeline::qsmart::outputs::CHI_STAGE1,
            qsm_core::pipeline::qsmart::outputs::LOCAL_FIELD_STAGE2,
            qsm_core::pipeline::qsmart::outputs::CHI_STAGE2,
        ];
        let state = PipelineState::load_or_create(&output.state_path(&run.key), &config, &run.key, false);
        let mut written = Vec::new();
        for step in ["qsmart", "qsmart-reliable"] {
            let recorded = &state.completed_steps[step].outputs;
            for name in names {
                let path = output.qsmart_intermediate_path(&run.key, step, name).unwrap();
                assert!(path.exists(), "{step}: {} not written", path.display());
                assert!(recorded.contains(&path), "{step}: {} not recorded", path.display());
                assert_eq!(voxels(&path), voxels(&output.mask_path(&run.key)), "on the working grid");
                written.push(path);
            }
        }
        crate::commands::run::clean_run(&output, &run);
        assert!(written.iter().all(|p| !p.exists()), "cleaning keeps QSMART's intermediates");
    }

    /// Each inversion algorithm's step writes the same raw map. Switching to another and back in
    /// the same directory (#373) recomputes the first, and leaves what a fresh run in a directory
    /// of its own does: the same maps, the same sidecar, and no record of the other step.
    #[test]
    fn switching_the_inversion_and_back_gives_what_a_fresh_run_does() {
        let dir = tempfile::tempdir().unwrap();
        let run = oblique_run(dir.path(), "a", 2);
        for two_pass in [false, true] {
            let mut config = fast_config();
            config.masking.two_pass = two_pass;
            config.inversion.qsmart.ilsqr_max_iter = 5;
            let fresh = DerivativeOutputs::new(&dir.path().join(format!("fresh-{two_pass}")));
            run_pipeline_cached(&run, &config, &fresh, false, &|_| {}).unwrap();
            let output = DerivativeOutputs::new(&dir.path().join(format!("out-{two_pass}")));
            run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
            config.inversion.algorithm = QsmAlgorithm::Qsmart;
            run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
            config.inversion.algorithm = QsmAlgorithm::Rts;
            run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();

            let k = &run.key;
            let mut maps = vec![(output.qsm_path(k), fresh.qsm_path(k)), (output.qsm_support_path(k), fresh.qsm_support_path(k)),
                                (output.chi_raw_path(k), fresh.chi_raw_path(k))];
            if two_pass {
                maps.push((output.reliable_chi_raw_path(k), fresh.reliable_chi_raw_path(k)));
            }
            for (got, want) in maps {
                assert!(io::read_nifti_file(&got).unwrap().data == io::read_nifti_file(&want).unwrap().data,
                        "two-pass {two_pass}: {} is not what a fresh run makes", got.display());
            }
            let sidecar = |o: &DerivativeOutputs| read_json_object(&crate::bids::entities::sidecar_path(&o.qsm_path(k)).unwrap());
            assert_eq!(sidecar(&output), sidecar(&fresh), "two-pass {two_pass}");
            let state = PipelineState::load_or_create(&output.state_path(k), &config, k, false);
            assert!(!state.completed_steps.contains_key("qsmart"), "two-pass {two_pass}: QSMART's record outlived its map");
            assert!(!output.provenance_path(k, "qsmart").exists(), "two-pass {two_pass}: QSMART's provenance outlived its map");
        }
    }

    /// Switched from each inversion to each other one and back, the only provenance naming a raw
    /// map as its output is that of the step that wrote it last. QSMART runs on its own mask, as
    /// `qsmxt run` gives it by default, so switching to or from it remakes the mask first and the
    /// state has already forgotten the other inversion by the time QSMART is recorded.
    #[test]
    fn only_the_last_inversion_is_named_the_raw_maps_maker() {
        let dir = tempfile::tempdir().unwrap();
        let run = oblique_run(dir.path(), "a", 2);
        let k = &run.key;
        let algorithms = [(QsmAlgorithm::Rts, "invert"), (QsmAlgorithm::Tgv, "tgv"), (QsmAlgorithm::Qsmart, "qsmart")];
        for (a, step) in algorithms {
            for (b, _) in algorithms.into_iter().filter(|&(b, _)| b != a) {
                for two_pass in [false, true] {
                    let mut config = fast_config();
                    config.masking.two_pass = two_pass;
                    config.inversion.qsmart.ilsqr_max_iter = 5;
                    let output = DerivativeOutputs::new(&dir.path().join(format!("{a:?}-{b:?}-{two_pass}")));
                    for algorithm in [a, b, a] {
                        config.inversion.algorithm = algorithm;
                        config.masking.sections = if algorithm == QsmAlgorithm::Qsmart {
                            qsmxt_engine::masking::qsmart_default_mask_sections()
                        } else {
                            fast_config().masking.sections
                        };
                        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
                    }
                    let mut raws = vec![(output.chi_raw_path(k), step.to_string())];
                    if two_pass {
                        raws.push((output.reliable_chi_raw_path(k), format!("{step}-reliable")));
                    }
                    for (raw, step) in raws {
                        let makers: Vec<&str> = crate::bids::bidsprov::STEPS.iter().copied().filter(|&s| {
                            let Ok(text) = std::fs::read_to_string(output.provenance_path(k, s)) else { return false };
                            let prov: serde_json::Value = serde_json::from_str(&text).unwrap();
                            prov["outputs"].as_array().unwrap().iter().any(|o| o.as_str() == Some(&raw.display().to_string()))
                        }).collect();
                        assert_eq!(makers, [step.as_str()], "{a:?} → {b:?} → {a:?}, two-pass {two_pass}");
                    }
                }
            }
        }
    }

    /// A recomputed map's sidecar says what was done to it, not to the map it replaced: re-run
    /// without a reference, the map records no offset from the mean reference it had before.
    #[test]
    fn a_recomputed_map_keeps_nothing_from_the_sidecar_it_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let run = oblique_run(dir.path(), "a", 2);
        let output = DerivativeOutputs::new(&dir.path().join("out"));
        let mut config = fast_config();
        let sidecar = || read_json_object(&crate::bids::entities::sidecar_path(&output.qsm_path(&run.key)).unwrap());
        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
        assert!(sidecar().contains_key("QsmReferenceOffsetPpm"));
        config.qsm.reference = crate::pipeline::config::QsmReference::None;
        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
        let fields = sidecar();
        assert_eq!(fields["QsmReference"], "none");
        assert!(!fields.contains_key("QsmReferenceOffsetPpm") && !fields.contains_key("QsmReferenceVoxels"), "{fields:?}");
    }

    /// End to end with V-SHARP: the published χ is 0 on the rim background removal eroded, its
    /// mean over the published support is the reference zero, and the support is smaller than
    /// the brain mask (or the test would prove nothing).
    #[test]
    fn the_published_map_is_zero_outside_its_published_support() {
        let dir = tempfile::tempdir().unwrap();
        let run = oblique_run(dir.path(), "a", 3);
        let output = DerivativeOutputs::new(&dir.path().join("out"));
        run_pipeline_cached(&run, &fast_config(), &output, false, &|_| {}).unwrap();

        let chi = load_volume(&output.qsm_path(&run.key)).unwrap();
        let brain = load_mask(&output.mask_path(&run.key)).unwrap();
        let support = load_mask(&output.qsm_support_path(&run.key)).unwrap();
        let count = |m: &[u8]| m.iter().filter(|&&v| v != 0).count();
        assert!(count(&support) > 0 && count(&support) < count(&brain),
                "support {} vs brain {}", count(&support), count(&brain));
        assert!(support.iter().zip(&brain).all(|(&s, &b)| s <= b), "the support lies in the brain");
        for (i, (&c, &s)) in chi.iter().zip(&support).enumerate() {
            if s == 0 {
                assert_eq!(c, 0.0, "voxel {i} outside the support");
            }
        }
        let mean: f64 = chi.iter().zip(&support).filter(|(_, &s)| s != 0).map(|(&c, _)| c).sum::<f64>()
            / count(&support) as f64;
        assert!(mean.abs() < 1e-5, "mean over the support is {mean}");
    }

    /// The grid was cached with the first run's geometry, so a new threshold was ignored.
    #[test]
    fn a_new_obliquity_threshold_is_honoured_on_a_rerun() {
        let dir = tempfile::tempdir().unwrap();
        let run = oblique_run(dir.path(), "a", 1);
        let output = DerivativeOutputs::new(&dir.path().join("out"));
        let resampled = |output: &DerivativeOutputs| -> bool {
            let state: PipelineState = serde_json::from_str(
                &std::fs::read_to_string(output.state_path(&run.key)).unwrap()).unwrap();
            state.run_metadata.unwrap().source_geometry.is_some()
        };
        run_pipeline_cached(&run, &fast_config(), &output, false, &|_| {}).unwrap();
        assert!(!resampled(&output));
        run_pipeline_cached(&run, &resampling_config(), &output, false, &|_| {}).unwrap();
        assert!(resampled(&output));
        run_pipeline_cached(&run, &fast_config(), &output, false, &|_| {}).unwrap();
        assert!(!resampled(&output));
        assert_eq!(voxels(&output.qsm_path(&run.key)), ACQUIRED);
    }

    /// Returning a resampled run to the acquired grid moves a segmentation by majority vote, so
    /// every value is still a label id, and rebuilds the SMWI minIPs on the acquired grid (#267).
    #[test]
    fn output_space_keeps_labels_valid_and_rebuilds_the_smwi_projections() {
        let dir = tempfile::tempdir().unwrap();
        let run = oblique_run(dir.path(), "a", 2);
        let output = DerivativeOutputs::new(&dir.path().join("out"));
        let mut config = resampling_config();
        config.pipeline.do_smwi = true;
        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
        for contrast in qsmxt_engine::store::Contrast::BOTH {
            let mip = io::read_nifti_file(&output.smwi_mip_path(&run.key, contrast)).unwrap();
            let contrast = contrast.name();
            assert_eq!((mip.dims.0, mip.dims.1), (DIMS.0, DIMS.1),
                       "the {contrast} minIP is still on the working grid");
        }

        // A parcellation on the working grid, where the pipeline would have written one, and the
        // return to the acquired grid made to run again.
        let work = io::read_nifti_file(&output.working_grid_dir(&run.key)
            .join(output.mask_path(&run.key).file_name().unwrap())).unwrap();
        let ids = [0.0, 10.0, 11.0, 49.0, 50.0];
        let dseg: Vec<f64> = (0..work.data.len()).map(|v| ids[(v / 7) % ids.len()]).collect();
        save_nifti_like(&output.dseg_path(&run.key), &dseg, &work);
        let state_path = output.state_path(&run.key);
        let mut state: PipelineState = serde_json::from_str(&std::fs::read_to_string(&state_path).unwrap()).unwrap();
        state.completed_steps.remove("output_space");
        state.save(&state_path).unwrap();

        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
        let back = io::read_nifti_file(&output.dseg_path(&run.key)).unwrap();
        assert_eq!(back.data.len(), ACQUIRED);
        assert!(back.data.iter().all(|v| ids.contains(v)), "a label was interpolated");
    }

    /// `--output-space working` on a resampled run names its outputs `space-axial` with a
    /// `SpatialReference`; switching back to the acquired grid leaves no `space-axial` file behind
    /// (#275).
    #[test]
    fn working_grid_outputs_are_named_in_their_space() {
        let dir = tempfile::tempdir().unwrap();
        let run = oblique_run(dir.path(), "a", 2);
        let output = DerivativeOutputs::new(&dir.path().join("out"));
        let mut config = resampling_config();
        config.pipeline.output_space = crate::pipeline::config::OutputSpace::Working;
        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
        let axial = output.in_space(Some("axial"));
        assert!(axial.qsm_path(&run.key).exists());
        assert!(!output.qsm_path(&run.key).exists(), "an unlabelled map on the working grid");
        assert_eq!(output.published(&run.key).qsm_path(&run.key), axial.qsm_path(&run.key));
        let sidecar: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
            crate::bids::entities::sidecar_path(&axial.qsm_path(&run.key)).unwrap()).unwrap()).unwrap();
        let reference = sidecar["SpatialReference"].as_str().unwrap();
        assert!(output.output_dir.join(reference).exists(), "{reference}");
        assert!(reference.ends_with("_part-mag_space-axial_T2starw.nii"), "{reference}");

        config.pipeline.output_space = crate::pipeline::config::OutputSpace::Acquired;
        run_pipeline_cached(&run, &config, &output, false, &|_| {}).unwrap();
        assert_eq!(voxels(&output.qsm_path(&run.key)), ACQUIRED);
        let left: Vec<String> = std::fs::read_dir(output.anat_dir(&run.key)).unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("space-axial"))
            .collect();
        assert!(left.is_empty(), "{left:?}");
    }

    fn save_nifti_like(path: &Path, data: &[f64], like: &qsm_core::io::NiftiData) {
        write_volume(path, data, like.dims, like.voxel_size, &like.affine).unwrap();
    }

    /// An oblique three-orientation group under the default `--output-space acquired` (#270).
    ///
    /// The members' magnitudes are back on the acquired grid by the time the group is combined,
    /// while their local fields are on the working grid; the group used to read the two from
    /// different grids and fail. Now it reads both from the working grid, writes a referenced
    /// COSMOS map on the reference member's acquired grid, caches it, and recomputes it when a
    /// member's background removal changes.
    #[test]
    fn an_oblique_cosmos_group_lands_on_the_acquired_grid_referenced_and_cached() {
        let dir = tempfile::tempdir().unwrap();
        let output = DerivativeOutputs::new(&dir.path().join("out"));
        // No sidecar `B0_dir` (one would stop the resample to axial), so the three share one
        // affine and their directions cannot be told apart: `force` reconstructs anyway, which is
        // all this test needs — it is about grids and caching, not the directions.
        let runs: Vec<QsmRun> = ["d1", "d2", "d3"].iter()
            .map(|acq| oblique_run(dir.path(), acq, 2)).collect();
        let mut config = resampling_config();
        config.multi_orientation.group_by = "acq".into();
        config.multi_orientation.force = true;
        config.multi_orientation.lambda = 1e-2;
        for r in &runs {
            run_pipeline_cached(r, &config, &output, false, &|_| {}).unwrap();
        }
        let members: Vec<&QsmRun> = runs.iter().collect();
        let group = crate::bids::orientation::OrientationGroup { label: "sub-1".into(), members: vec![0, 1, 2] };
        let combine = |config: &PipelineConfig| crate::pipeline::multiorient::reconstruct_group(
            &group, &members, config, &output, false, &|_| {}).unwrap();

        combine(&config);
        let key = crate::pipeline::multiorient::group_key(&members);
        let cosmos = output.cosmos_path(&key);
        assert_eq!(voxels(&cosmos), ACQUIRED, "the COSMOS map is on the acquired grid");
        let sidecar: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
            output.multiorient_sidecar_path(&key, "cosmos")).unwrap()).unwrap();
        assert_eq!(sidecar["QsmReference"], "mean", "{sidecar}");
        assert!(sidecar["QsmReferenceOffsetPpm"].is_number(), "{sidecar}");
        assert_eq!(sidecar["Space"], "acquired");

        let mtime = || std::fs::metadata(&cosmos).unwrap().modified().unwrap();
        let first = mtime();
        combine(&config);
        assert_eq!(mtime(), first, "an unchanged group was recomputed");

        // A member's background removal changes: its local field is rewritten, and the group
        // is recomputed from it.
        let mut other = config.clone();
        other.bg_removal.algorithm = BfAlgorithm::Pdf;
        run_pipeline_cached(&runs[1], &other, &output, false, &|_| {}).unwrap();
        combine(&config);
        assert_ne!(mtime(), first, "a member's new local field did not recompute the group");
    }
}

/// Referencing over the map's support rather than the brain mask.
///
/// Background removal erodes the mask, and the raw map is 0 on that rim. Referencing over the
/// brain mask averaged those zeros into the offset and then wrote the rim out as a constant
/// `-offset` — a shell of about 10% of the brain at 1 mm with V-SHARP.
#[cfg(test)]
mod real_imag_input_tests {
    use super::*;
    use crate::bids::discovery::{discover_runs, DiscoveryFilter};
    use crate::testutils;

    fn run_on(bids: &Path, out: &Path) -> (QsmRun, DerivativeOutputs) {
        let run = discover_runs(bids, &DiscoveryFilter::default()).unwrap().remove(0);
        let output = DerivativeOutputs::new(out);
        run_pipeline_cached(&run, &PipelineConfig::default(), &output, true, &|_| {}).unwrap();
        (run, output)
    }

    /// Real/imaginary input, chopped along z as GE leaves it, reconstructs exactly what the same
    /// echoes given as magnitude/phase do: the chop is found and undone before anything else
    /// runs, and the provenance says so.
    #[test]
    fn chopped_real_imag_matches_mag_phase() {
        let dir = tempfile::tempdir().unwrap();
        let complex = testutils::create_real_imag_bids(&dir.path().join("ri"), true);
        let twin = testutils::create_mag_phase_twin_bids(&dir.path().join("mp"));
        let (ri_run, ri) = run_on(&complex, &dir.path().join("ri-out"));
        let (mp_run, mp) = run_on(&twin, &dir.path().join("mp-out"));

        for e in 1..=3 {
            let a = load_volume(&ri.phase_scaled_path(&ri_run.key, e)).unwrap();
            let b = load_volume(&mp.phase_scaled_path(&mp_run.key, e)).unwrap();
            let worst = a.iter().zip(&b)
                .map(|(x, y)| { let d = (x - y).rem_euclid(2.0 * std::f64::consts::PI); d.min(2.0 * std::f64::consts::PI - d) })
                .fold(0.0, f64::max);
            assert!(worst < 1e-6, "echo {e} phase differs by up to {worst} rad");
            let a = load_volume(&ri.mag_path(&ri_run.key, e)).unwrap();
            let b = load_volume(&mp.mag_path(&mp_run.key, e)).unwrap();
            assert!(a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-3 * y.abs().max(1.0)), "echo {e} magnitude differs");
        }
        let chi_ri = load_volume(&ri.qsm_path(&ri_run.key)).unwrap();
        let chi_mp = load_volume(&mp.qsm_path(&mp_run.key)).unwrap();
        let worst = chi_ri.iter().zip(&chi_mp).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max);
        assert!(worst < 1e-4, "χ differs by up to {worst} ppm");

        let prov: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(ri.provenance_path(&ri_run.key, "scale_phase")).unwrap()).unwrap();
        let params = &prov["parameters"];
        assert_eq!(params["input"], "real_imaginary");
        let conv = &params["real_imaginary_conversion"];
        assert_eq!(conv["method"], "complex_to_mag_phase");
        assert_eq!(conv["corrected"], serde_json::json!({ "x": false, "y": false, "z": true }));
        assert!(conv["coherence"]["z"].as_f64().unwrap() < -0.5, "{conv}");
        assert!(prov["inputs"].as_array().unwrap().iter().any(|p| p.as_str().unwrap().contains("_part-imag_")));
    }

    #[test]
    fn ge_real_imag_phase_is_negated() {
        // End to end: a GE sidecar gives the negated phase of the same echoes from elsewhere.
        let dir = tempfile::tempdir().unwrap();
        let ge = testutils::create_real_imag_bids(&dir.path().join("ge"), true);
        for p in glob::glob(&format!("{}/sub-1/anat/*.json", ge.display())).unwrap().flatten() {
            let mut j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
            j["Manufacturer"] = serde_json::json!("GE");
            std::fs::write(&p, j.to_string()).unwrap();
        }
        let other = testutils::create_real_imag_bids(&dir.path().join("other"), true);
        let (ge_run, ge_out) = run_on(&ge, &dir.path().join("ge-out"));
        let (other_run, other_out) = run_on(&other, &dir.path().join("other-out"));
        let a = load_volume(&ge_out.phase_scaled_path(&ge_run.key, 1)).unwrap();
        let b = load_volume(&other_out.phase_scaled_path(&other_run.key, 1)).unwrap();
        let worst = a.iter().zip(&b)
            .map(|(x, y)| { let d = (x + y).rem_euclid(2.0 * std::f64::consts::PI); d.min(2.0 * std::f64::consts::PI - d) })
            .fold(0.0, f64::max);
        assert!(worst < 1e-6, "GE phase is not the negation: up to {worst} rad off");
        let prov: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(ge_out.provenance_path(&ge_run.key, "scale_phase")).unwrap()).unwrap();
        assert_eq!(prov["parameters"]["negate_phase"], true);
    }

    /// An unchopped export converts with no correction, and a re-run is served from the cache
    /// even though the provenance carries the detection outcome next to the parameters.
    #[test]
    fn unchopped_real_imag_is_not_corrected_and_caches() {
        let dir = tempfile::tempdir().unwrap();
        let complex = testutils::create_real_imag_bids(&dir.path().join("ri"), false);
        let (run, output) = run_on(&complex, &dir.path().join("out"));
        let prov_path = output.provenance_path(&run.key, "scale_phase");
        let prov: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&prov_path).unwrap()).unwrap();
        assert_eq!(prov["parameters"]["real_imaginary_conversion"]["corrected"],
            serde_json::json!({ "x": false, "y": false, "z": false }));

        let before = std::fs::metadata(&prov_path).unwrap().modified().unwrap();
        run_pipeline_cached(&run, &PipelineConfig::default(), &output, false, &|_| {}).unwrap();
        assert_eq!(std::fs::metadata(&prov_path).unwrap().modified().unwrap(), before, "scale_phase re-ran");
    }
}

#[cfg(test)]
mod reference_support_tests {
    use super::*;

    const DIMS: (usize, usize, usize) = (8, 8, 8);
    const IDENTITY: [f64; 16] = [
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];

    pub(super) fn meta() -> RunMetadata {
        RunMetadata {
            dims: DIMS, voxel_size: (1.0, 1.0, 1.0), affine: IDENTITY, n_echoes: 1,
            echo_times: vec![0.004], b0_direction: (0.0, 0.0, 1.0), field_strength: 3.0,
            has_magnitude: true, source_geometry: None, slice_geometry: None,
        }
    }

    /// A box `lo..hi` on every axis.
    fn cube(lo: usize, hi: usize) -> Vec<u8> {
        let (nx, ny, nz) = DIMS;
        let mut m = vec![0u8; nx * ny * nz];
        for k in lo..hi {
            for j in lo..hi {
                for i in lo..hi {
                    m[i + j * nx + k * nx * ny] = 1;
                }
            }
        }
        m
    }

    pub(super) struct Fixture {
        _dir: tempfile::TempDir,
        pub(super) output: DerivativeOutputs,
        pub(super) run: QsmRun,
        pub(super) brain: Vec<u8>,
        pub(super) support: Vec<u8>,
        pub(super) chi_raw: Vec<f64>,
    }

    /// A brain mask of 6³ voxels, of which background removal kept the inner 4³: the raw map is
    /// non-zero (and offset from zero by 0.05 ppm) on the support and exactly 0 on the eroded rim.
    pub(super) fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let output = DerivativeOutputs::new(&dir.path().join("out"));
        let mut run = super::tests::run_with_echoes(vec![(dir.path().join("p.nii"), None)]);
        run.dims = DIMS;
        let (brain, support) = (cube(1, 7), cube(2, 6));
        let chi_raw: Vec<f64> = support.iter().enumerate()
            .map(|(i, &s)| if s != 0 { 0.05 + 0.001 * (i % 7) as f64 } else { 0.0 })
            .collect();
        let m = meta();
        save_mask(&output.mask_path(&run.key), &brain, &m).unwrap();
        save_mask(&output.bg_mask_path(&run.key), &support, &m).unwrap();
        save_volume(&output.chi_raw_path(&run.key), &chi_raw, &m).unwrap();
        // Compare against what is on disk, in whatever precision the NIfTI holds it.
        let chi_raw = load_volume(&output.chi_raw_path(&run.key)).unwrap();
        Fixture { _dir: dir, output, run, brain, support, chi_raw }
    }

    fn mean_over(chi: &[f64], mask: &[u8]) -> f64 {
        let (s, n) = chi.iter().zip(mask).filter(|(_, &m)| m != 0)
            .fold((0.0, 0usize), |(s, n), (&c, _)| (s + c, n + 1));
        s / n as f64
    }

    /// Run the single-pass `reference` step over the fixture with `config`.
    pub(super) fn reference(f: &Fixture, config: &PipelineConfig) -> Vec<f64> {
        run_engine_step(&f.run, config, &f.output, &meta(), Stage::Reference).unwrap();
        load_volume(&f.output.qsm_path(&f.run.key)).unwrap()
    }

    fn sidecar_offset(f: &Fixture) -> f64 {
        let sidecar = crate::bids::entities::sidecar_path(&f.output.qsm_path(&f.run.key)).unwrap();
        read_json_object(&sidecar)["QsmReferenceOffsetPpm"].as_f64().unwrap()
    }

    fn rim(f: &Fixture) -> impl Iterator<Item = usize> + '_ {
        (0..f.brain.len()).filter(|&i| f.brain[i] != 0 && f.support[i] == 0)
    }

    #[test]
    fn mean_reference_is_measured_over_the_support_and_the_rim_stays_zero() {
        let f = fixture();
        let out = reference(&f, &PipelineConfig::default());

        let true_mean = mean_over(&f.chi_raw, &f.support);
        let brain_mean = mean_over(&f.chi_raw, &f.brain);
        // The fixture has a real rim, and the brain-mask mean really is biased by it.
        assert!(rim(&f).count() > 0);
        assert!((true_mean - brain_mean).abs() > 0.01, "{true_mean} vs {brain_mean}");

        for i in rim(&f) {
            assert_eq!(out[i], 0.0, "eroded rim voxel {i} must be 0, not -offset");
        }
        for (i, &s) in f.support.iter().enumerate() {
            if s != 0 {
                assert!((out[i] - (f.chi_raw[i] - true_mean)).abs() < 1e-6, "voxel {i}");
            }
        }
        assert!(mean_over(&out, &f.support).abs() < 1e-6, "the support's mean is the zero");
        assert!((sidecar_offset(&f) - true_mean).abs() < 1e-9, "the recorded offset is the support mean");

        // The support is published, and it is exactly where the map is defined.
        assert_eq!(load_mask(&f.output.qsm_support_path(&f.run.key)).unwrap(), f.support);
    }

    #[test]
    fn region_reference_is_measured_only_where_the_region_meets_the_support() {
        let f = fixture();
        let mut config = PipelineConfig::default();
        config.qsm.reference = QsmReference::Region;
        config.qsm.reference_region = Some("left-thalamus".into());
        let ids = qsmxt_engine::regions::resolve("left-thalamus", config.segmentation.version).unwrap();

        // The region covers the lower half of the brain: half of it lies in the eroded rim, which
        // is where a small deep structure (the accumbens) lost about half its voxels in practice.
        let nxy = DIMS.0 * DIMS.1;
        let roi: Vec<u8> = f.brain.iter().enumerate()
            .map(|(i, &b)| u8::from(b != 0 && i / nxy < 4)).collect();
        let dseg: Vec<f64> = roi.iter().map(|&r| if r != 0 { ids[0] as f64 } else { 0.0 }).collect();
        save_volume(&f.output.dseg_path(&f.run.key), &dseg, &meta()).unwrap();

        let out = reference(&f, &config);
        let in_both: Vec<u8> = roi.iter().zip(&f.support).map(|(&r, &s)| r & s).collect();
        let expected = mean_over(&f.chi_raw, &in_both);
        assert!((expected - mean_over(&f.chi_raw, &roi)).abs() > 0.01, "the rim would bias it");
        assert!((sidecar_offset(&f) - expected).abs() < 1e-9);
        for i in rim(&f) {
            assert_eq!(out[i], 0.0, "eroded rim voxel {i} must be 0");
        }
        assert!(mean_over(&out, &in_both).abs() < 1e-6);
    }

    /// `none` never left a shell — the raw rim is already 0 — and must keep not doing so.
    /// Each map's rows in the statistics table carry that map's own reference (#280): the two-pass
    /// single-pass map has its own offset, and the χ-separation maps, never referenced, have none.
    #[test]
    fn each_map_carries_its_own_reference_in_the_table() {
        let f = fixture();
        let config = PipelineConfig::default();
        let m = meta();
        reference(&f, &config);
        // The single-pass map: a different raw map, so a different offset.
        let raw2: Vec<f64> = f.chi_raw.iter().map(|&c| if c != 0.0 { c + 0.02 } else { 0.0 }).collect();
        save_volume(&f.output.chi_raw_path(&f.run.key), &raw2, &m).unwrap();
        run_engine_step(&f.run, &config, &f.output, &m, Stage::SinglePassReference).unwrap();
        // χ-separation maps, which are never referenced.
        for p in [f.output.chi_para_path(&f.run.key), f.output.chi_dia_path(&f.run.key),
                  f.output.chi_sep_total_path(&f.run.key)] {
            save_volume(&p, &f.chi_raw, &m).unwrap();
        }
        let dseg: Vec<f64> = f.brain.iter().map(|&b| if b != 0 { 10.0 } else { 0.0 }).collect();
        let dseg_path = f.output.dseg_path(&f.run.key);
        save_volume(&dseg_path, &dseg, &m).unwrap();
        run_engine_step(&f.run, &config, &f.output, &m, Stage::Analysis).unwrap();

        let offset_of = |path: PathBuf| -> f64 {
            read_json_object(&crate::bids::entities::sidecar_path(&path).unwrap())
                ["QsmReferenceOffsetPpm"].as_f64().unwrap()
        };
        let main_off = offset_of(f.output.qsm_path(&f.run.key));
        let single_off = offset_of(f.output.singlepass_qsm_path(&f.run.key));
        assert!((main_off - single_off).abs() > 0.01, "{main_off} vs {single_off}");

        let table = std::fs::read_to_string(f.output.segmentation_stats_path(&f.run.key)).unwrap();
        let rows = stats::parse_tsv(&table);
        let by_map = |name: &str| rows.iter().filter(|r| r.map == name).collect::<Vec<_>>();
        for r in by_map("Chimap") {
            assert_eq!(r.reference.as_deref(), Some("mean"));
            assert!((r.reference_offset_ppm.unwrap() - main_off).abs() < 1e-6);
        }
        let single = by_map("desc-singlepass_Chimap");
        assert!(!single.is_empty());
        for r in single {
            assert_eq!(r.reference.as_deref(), Some("mean"));
            assert!((r.reference_offset_ppm.unwrap() - single_off).abs() < 1e-6, "{r:?}");
        }
        for name in ["desc-paramagnetic_Chimap", "desc-diamagnetic_Chimap", "desc-total_Chimap"] {
            let rows = by_map(name);
            assert!(!rows.is_empty(), "{name} was not summarised");
            for r in rows {
                assert_eq!((r.reference.as_deref(), r.reference_offset_ppm), (None, None), "{r:?}");
            }
        }
    }

    #[test]
    fn no_reference_leaves_the_rim_at_zero_and_the_support_untouched() {
        let f = fixture();
        let mut config = PipelineConfig::default();
        config.qsm.reference = QsmReference::None;
        let out = reference(&f, &config);
        for (i, (&o, (&s, &c))) in out.iter().zip(f.support.iter().zip(&f.chi_raw)).enumerate() {
            let expected = if s != 0 { c } else { 0.0 };
            assert!((o - expected).abs() < 1e-6, "voxel {i}");
        }
    }
}

/// χ-separation over the support χ is defined on, not the brain mask.
///
/// Referencing writes χ as 0 on the rim background removal eroded. Fitting χ-separation over the
/// brain mask took that rim as measured χ = 0 and published para/dia maps there.
#[cfg(test)]
mod chisep_support_tests {
    use super::*;
    use qsmxt_engine::stages::separation::zero_outside;
    use super::reference_support_tests::{fixture, meta, Fixture};

    fn sep_config(alg: SeparationAlgorithm) -> PipelineConfig {
        let mut config = PipelineConfig::default();
        config.pipeline.do_chi_separation = true;
        config.separation.algorithm = alg;
        config
    }

    /// Reference the fixture, add the relaxation, magnitude and local-field inputs (defined over
    /// the whole brain mask, or the field over the support as background removal leaves it), and
    /// run the `chi_separation` step. Returns the (para, dia, total) maps it wrote.
    fn separate(f: &Fixture, config: &PipelineConfig) -> [Vec<f64>; 3] {
        let m = meta();
        let k = &f.run.key;
        let over = |mask: &[u8], base: f64| -> Vec<f64> {
            mask.iter().enumerate()
                .map(|(i, &b)| if b != 0 { base + 0.1 * (i % 5) as f64 } else { 0.0 }).collect()
        };
        save_volume(&f.output.r2star_path(k), &over(&f.brain, 20.0), &m).unwrap();
        save_volume(&f.output.r2prime_path(k), &over(&f.brain, 5.0), &m).unwrap();
        save_volume(&f.output.mag_path(k, 1), &over(&f.brain, 100.0), &m).unwrap();
        let field: Vec<f64> = over(&f.support, 0.0).iter().zip(&f.support)
            .map(|(&v, &s)| if s != 0 { 0.01 + 0.001 * v } else { 0.0 }).collect();
        save_volume(&f.output.local_field_path(k), &field, &m).unwrap();

        super::reference_support_tests::reference(f, config);
        run_engine_step(&f.run, config, &f.output, &m, Stage::ChiSeparation).unwrap();
        [f.output.chi_para_path(k), f.output.chi_dia_path(k), f.output.chi_sep_total_path(k)]
            .map(|p| load_volume(&p).unwrap())
    }

    /// The method run directly on `mask`, with every input zeroed outside it and the outputs
    /// zeroed outside it: what the step must produce when it fits over `mask`.
    fn direct(f: &Fixture, config: &PipelineConfig, mask: &[u8]) -> [Vec<f64>; 3] {
        let m = meta();
        let k = &f.run.key;
        let masked = |p: PathBuf| -> Vec<f64> {
            let mut v = load_volume(&p).unwrap();
            zero_outside(mask, &mut v);
            v
        };
        let qsm = masked(f.output.qsm_path(k));
        let field = masked(f.output.local_field_path(k));
        let r2prime = masked(f.output.r2prime_path(k));
        let r2star = masked(f.output.r2star_path(k));
        let mag = load_volume(&f.output.mag_path(k, 1)).unwrap();
        let inputs = qsm_core::pipeline::SeparationInputs {
            local_field_ppm: &field, qsm: &qsm, mask, r2prime: Some(&r2prime), r2star: Some(&r2star),
            magnitude_rss: Some(&mag), magnitude_multi: Some(&mag), se_magnitude_multi: None,
        };
        let metadata = crate::pipeline::config::to_scan_metadata(
            m.dims, m.voxel_size, &m.echo_times, m.field_strength, m.b0_direction, Some(m.affine));
        let r = qsm_core::pipeline::run_separation(
            inputs, &metadata, &qsmxt_engine::bridge::to_separation_config(config), &mut |_, _| {},
        ).unwrap();
        let mut out = [r.chi_pos, r.chi_neg.iter().map(|v| v.abs()).collect(), r.chi_total];
        for map in out.iter_mut() {
            zero_outside(mask, map);
        }
        out
    }

    fn assert_zero_outside(maps: &[Vec<f64>; 3], support: &[u8]) {
        for (name, map) in ["para", "dia", "total"].iter().zip(maps) {
            for (i, (&v, &s)) in map.iter().zip(support).enumerate() {
                if s == 0 {
                    assert_eq!(v, 0.0, "{name} voxel {i} lies outside the support");
                }
            }
        }
    }

    fn assert_close(a: &[Vec<f64>; 3], b: &[Vec<f64>; 3]) {
        for (name, (x, y)) in ["para", "dia", "total"].iter().zip(a.iter().zip(b)) {
            for (i, (&p, &q)) in x.iter().zip(y).enumerate() {
                assert!((p - q).abs() <= 1e-6 * (1.0 + q.abs()), "{name} voxel {i}: {p} vs {q}");
            }
        }
    }

    /// A voxel-wise method: 0 on the eroded rim, fitted everywhere on the support.
    #[test]
    fn r2star_qsm_is_zero_on_the_rim_and_fitted_over_the_support() {
        let f = fixture();
        let config = sep_config(SeparationAlgorithm::R2starQsm);
        let out = separate(&f, &config);
        assert!(f.brain.iter().zip(&f.support).any(|(&b, &s)| b != 0 && s == 0), "the fixture has a rim");
        assert_zero_outside(&out, &f.support);
        for (i, &s) in f.support.iter().enumerate() {
            if s != 0 {
                assert!(out[0][i] != 0.0 || out[1][i] != 0.0, "support voxel {i} was not fitted");
            }
        }
        assert_close(&out, &direct(&f, &config, &f.support));
    }

    /// A spatial method that fits the local field: its fit domain is the support. Run over the
    /// brain mask it gives a different answer, so matching the support run is not a coincidence.
    ///
    /// The fixture's R2' and R2* cover the whole brain mask, rim included. The classical methods
    /// mask their inputs to the mask they are given, so restricting those maps is observable only
    /// through the networks (SUSEP-Net, χ-sepnet), whose weights the tests do not download.
    #[test]
    fn chi_sep_ilsqr_fits_the_support_not_the_brain_mask() {
        let f = fixture();
        let config = sep_config(SeparationAlgorithm::ChiSepIlsqr);
        let out = separate(&f, &config);
        assert_zero_outside(&out, &f.support);
        assert_close(&out, &direct(&f, &config, &f.support));

        let over_brain = direct(&f, &config, &f.brain);
        let differs = out[2].iter().zip(&over_brain[2]).zip(&f.support)
            .any(|((&a, &b), &s)| s != 0 && (a - b).abs() > 1e-6);
        assert!(differs, "fitting over the brain mask must change the support's values");
    }


    /// A rerun of a map fitted over the brain mask by an older release is not reused.
    #[test]
    fn the_cache_key_records_the_fit_domain() {
        let f = fixture();
        let config = sep_config(SeparationAlgorithm::R2starQsm);
        separate(&f, &config);
        let prov: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
            f.output.provenance_path(&f.run.key, "chi_separation")).unwrap()).unwrap();
        assert_eq!(prov["parameters"]["domain"], "support");
        let support = f.output.qsm_support_path(&f.run.key).display().to_string();
        assert!(prov["inputs"].as_array().unwrap().iter().any(|p| p == &support), "{prov}");
    }
}
