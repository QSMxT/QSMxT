use std::path::{Path, PathBuf};
use std::time::Instant;

use glob::glob;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use qsm_core::io::{self, NiftiData};
use serde::Serialize;

use crate::bids::derivatives::DerivativeOutputs;
use crate::bids::discovery::QsmRun;
use qsmxt_engine::*;
use crate::pipeline::graph::{PipelineState, RunMetadata};
use qsmxt_engine::observe::{Observer, StageEvent};
#[cfg(test)]
use qsmxt_engine::plan::Stage;
// What the tests reach through this module.
#[cfg(test)]
use qsmxt_engine::store::{Completed, StageStore};
#[cfg(test)]
use crate::pipeline::stats;
use qsmxt_engine::memory;
use crate::pipeline::requirements;
use crate::nifti::write::write_volume;
use crate::error::QsmxtError;

mod store;
#[cfg(test)]
use store::copy_nifti;
use store::FileStore;

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
        peak_memory_bytes: crate::pipeline::memory::process_rss_bytes(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };
    if let Some(parent) = prov_path.parent() {
        fs_err::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&prov)
        .map_err(|e| QsmxtError::Config(format!("Failed to serialize provenance: {}", e)))?;
    fs_err::write(prov_path, json)?;
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
                let rss = crate::pipeline::memory::process_rss_bytes();
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
    let rss = crate::pipeline::memory::process_rss_bytes();
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
/// completed steps with valid outputs on disk are skipped. `applied` are the recommendations
/// `--accept-recommendations` applied that this run had, which its run record keeps.
pub fn run_pipeline_applied(
    qsm_run: &QsmRun,
    config: &PipelineConfig,
    output: &DerivativeOutputs,
    force: bool,
    applied: &[qsmxt_engine::recommendations::Recommendation],
    progress: &dyn Fn(&str),
) -> crate::Result<()> {
    // A `--pipeline` chosen before the scan was read, completed on this scan.
    let (completed, changes) = requirements::run_config(qsm_run, config)?;
    for c in &changes {
        log::info!("{}: {}: {} → {} (the pipeline's on this scan)", qsm_run.key, c.label, c.from_text, c.to_text);
    }
    let config = &completed;
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
    let mut record = qsmxt_engine::run::run(&plan, config, &facts, &mut store, &mut observer)?;
    record.recommendations.extend(applied.iter().map(|r| qsmxt_engine::recommendations::Recorded {
        finding: r.clone(), outcome: qsmxt_engine::recommendations::Outcome::Applied,
    }));
    write_run_files(qsm_run, config, &facts, &plan, &record, output)
}

/// [`run_pipeline_applied`] with nothing applied.
#[cfg(any(test, feature = "testutils"))]
pub fn run_pipeline_cached(
    qsm_run: &QsmRun,
    config: &PipelineConfig,
    output: &DerivativeOutputs,
    force: bool,
    progress: &dyn Fn(&str),
) -> crate::Result<()> {
    run_pipeline_applied(qsm_run, config, output, force, &[], progress)
}

/// A run's `*_runrecord.json`: what it did ([`qsmxt_engine::record::RunRecord`], its fields at
/// the top level) and the scan facts it was planned from (`scan`), so the dataset's methods can
/// be made from the file alone, whichever invocation processed the run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunRecordFile {
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
        fs_err::create_dir_all(dir)?;
    }
    let summary = qsmxt_engine::dataset::summarise_run(config, plan, facts, record, "qsmxt");
    fs_err::write(&methods_path, summary.methods.to_markdown())?;
    let file = RunRecordFile { record: record.clone(), scan: facts.clone() };
    let json = serde_json::to_string_pretty(&file).map_err(|e| QsmxtError::Config(e.to_string()))?;
    fs_err::write(output.run_record_path(key), json + "\n")?;
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

/// Say in `run`'s record, and the files made from it, whether the fan-in combined it into
/// `group` ([`qsmxt_engine::record::RunRecord::set_orientation_group`]). A run with no record did
/// not finish, so there is nothing to say.
pub fn record_orientation_group(
    run: &QsmRun, config: &PipelineConfig, output: &DerivativeOutputs, group: Option<&str>,
) -> crate::Result<()> {
    let Ok(text) = fs_err::read_to_string(output.run_record_path(&run.key)) else { return Ok(()) };
    let mut file: RunRecordFile = serde_json::from_str(&text).map_err(|e| QsmxtError::Config(e.to_string()))?;
    if !file.record.set_orientation_group(group) {
        return Ok(());
    }
    let plan = qsmxt_engine::plan::plan(config, &file.scan);
    write_run_files(run, config, &file.scan, &plan, &file.record, output)
}

/// Say in `run`'s record, and the files made from it, whether its final maps were exported as DICOM
/// series ([`qsmxt_engine::record::RunRecord::set_dicom_exported`]). A run with no record did not
/// finish, so there is nothing to say.
pub(crate) fn record_dicom_export(
    run: &QsmRun, config: &PipelineConfig, output: &DerivativeOutputs, exported: bool,
) -> crate::Result<()> {
    let Ok(text) = fs_err::read_to_string(output.run_record_path(&run.key)) else { return Ok(()) };
    let mut file: RunRecordFile = serde_json::from_str(&text).map_err(|e| QsmxtError::Config(e.to_string()))?;
    if !file.record.set_dicom_exported(exported) {
        return Ok(());
    }
    let plan = qsmxt_engine::plan::plan(config, &file.scan);
    write_run_files(run, config, &file.scan, &plan, &file.record, output)
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
        && config.pipeline.output_space == qsmxt_engine::OutputSpace::Working)
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
    let saved: Vec<PathBuf> = fs_err::read_dir(&stash)
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
                fs_err::create_dir_all(&dir)?;
                fs_err::copy(f, dir.join(name))?;
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
        let meta = fs_err::metadata(p).ok();
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
        let mut ds: Vec<PathBuf> = fs_err::read_dir(&deriv).ok()?
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
            // A file named for this run comes first (each run of a subject can have its own,
            // `qsmxt_engine::supplied::supplied_mask_path`), then the rest in name order.
            let own = format!("{}_", run.key.basename());
            hits.sort_by_key(|p| {
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                (!name.starts_with(&own), p.clone())
            });
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
    fs_err::read_to_string(path).ok()
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
mod tests;
