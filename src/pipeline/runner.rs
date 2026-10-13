use std::path::{Path, PathBuf};
use std::time::Instant;

use glob::glob;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use qsm_core::io::{self, NiftiData};
use serde::Serialize;

use crate::bids::derivatives::DerivativeOutputs;
use crate::bids::entities::AcquisitionKey;
use crate::bids::discovery::QsmRun;
use crate::pipeline::config::*;
use crate::pipeline::graph::{PipelineState, RunMetadata};
use crate::pipeline::memory;
use crate::pipeline::phase;
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
    peak_memory_bytes: usize,
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

/// The configuration a step reads, serialised whole, so that every field of it is part of the step's
/// cache key — a new config field is keyed without anyone remembering to (#272). The step's own
/// `parameters` (what provenance records) stay as they are; this is added beside them.
///
/// Sections are as narrow as the step actually reads: an upstream step keyed on a section it does
/// not read would be recomputed, and invalidate everything after it, whenever that section
/// changed. Two-pass steps read what their main-pass twin reads.
pub(crate) fn consumed_config(step: &str, config: &PipelineConfig) -> serde_json::Value {
    use serde_json::json;
    let base = step.strip_suffix(crate::pipeline::graph::RELIABLE_SUFFIX).unwrap_or(step);
    let crop = json!({
        "crop_to_mask": config.pipeline.crop_to_mask,
        "fft_padding": config.pipeline.fft_padding,
        "crop_margin_mm": config.pipeline.crop_margin_mm,
    });
    match base {
        "scale_phase" => json!({ "coil_combination_sigma": config.field_mapping.coil_combination_sigma }),
        "magnitude" => json!({
            "inhomogeneity_correction": config.masking.inhomogeneity_correction,
            "homogeneity": config.homogeneity,
        }),
        // The main mask is not the two-pass one, so its settings stay out of the main mask's key.
        "mask" if step == base => {
            let mut m = serde_json::to_value(&config.masking).unwrap_or_default();
            if let Some(o) = m.as_object_mut() {
                o.remove("two_pass");
                o.remove("two_pass_sections");
            }
            json!({ "masking": m, "bet": config.bet })
        }
        "mask" => json!({ "masking": config.masking, "bet": config.bet, "homogeneity": config.homogeneity }),
        "motion" => json!({ "motion_correction": config.motion_correction, "registration": config.registration }),
        "swi" => json!({ "swi": config.swi }),
        "unwrap" => json!({ "field_mapping": config.field_mapping }),
        "bgremove" => json!({ "bg_removal": config.bg_removal, "crop": crop }),
        "invert" | "tgv" | "qsmart" => json!({ "inversion": config.inversion, "crop": crop }),
        "twopass" => json!({ "masking": config.masking }),
        "reference" | "reference-singlepass" => json!({ "qsm": config.qsm }),
        "segmentation" => json!({ "segmentation": config.segmentation }),
        "smwi" => json!({ "smwi": config.smwi }),
        "chi_separation" => json!({ "separation": config.separation }),
        "output_space" => json!({ "output_space": config.pipeline.output_space }),
        // Keyed by their own parameters alone: R2*/T2* (ARLO has no settings), R2/R2′ (its
        // separation settings are in its parameters; keying the whole section would recompute the
        // MESE fit whenever the χ-separation method changed) and the statistics.
        _ => serde_json::Value::Null,
    }
}

/// A step's cache key: its algorithm and recorded parameters, plus the configuration it reads.
pub(crate) fn step_cache_key(
    step: &str, algorithm: Option<&str>, params: &serde_json::Value, config: &PipelineConfig,
) -> String {
    let consumed = consumed_config(step, config);
    if consumed.is_null() {
        return crate::pipeline::graph::step_params_hash(algorithm, params);
    }
    crate::pipeline::graph::step_params_hash(
        algorithm, &serde_json::json!({ "params": params, "config": consumed }))
}

/// Bundles references needed by every pipeline stage.
struct StageContext<'a> {
    run: &'a QsmRun,
    config: &'a PipelineConfig,
    output: &'a DerivativeOutputs,
    meta: &'a RunMetadata,
    state: &'a mut PipelineState,
    state_path: &'a Path,
}

impl StageContext<'_> {
    fn is_cached_with_params(&mut self, step: &str, algorithm: Option<&str>, params: &serde_json::Value) -> bool {
        let hash = step_cache_key(step, algorithm, params, self.config);
        self.state.is_step_cached_with_hash(step, Some(&hash))
    }

    fn dims(&self) -> (usize, usize, usize) { self.meta.dims }

    /// Whether the echoes after masking are the motion-corrected ones.
    fn motion_corrected(&self) -> bool {
        self.config.motion_correction.enabled && self.meta.n_echoes > 1 && self.meta.has_magnitude
    }

    /// Echo `echo` (1-based) phase, in radians on the working grid, as the stages after motion
    /// correction should read it.
    fn echo_phase_path(&self, echo: usize) -> PathBuf {
        if self.motion_corrected() { self.output.motion_phase_path(&self.run.key, echo) }
        else { self.output.phase_scaled_path(&self.run.key, echo) }
    }

    /// Echo `echo` (1-based) magnitude on the working grid, as the stages after motion
    /// correction should read it.
    fn echo_mag_path(&self, echo: usize) -> PathBuf {
        if self.motion_corrected() { self.output.motion_mag_path(&self.run.key, echo) }
        else { self.output.mag_path(&self.run.key, echo) }
    }
    fn voxel_size(&self) -> (f64, f64, f64) { self.meta.voxel_size }

    /// Write provenance.json, record params hash, and save pipeline state.
    fn complete_step(
        &mut self,
        step: &str,
        algorithm: Option<&str>,
        parameters: serde_json::Value,
        inputs: &[&Path],
        output_paths: Vec<PathBuf>,
        start: Instant,
    ) -> crate::Result<()> {
        self.complete_step_with_outcome(step, algorithm, parameters, None, inputs, output_paths, start)
    }

    /// [`Self::complete_step`], with what the step found in the data (`outcome`) recorded in its
    /// provenance next to its parameters. Only `parameters` key the cache: the outcome follows
    /// from them and the inputs, so it is not known when the cache is checked.
    #[allow(clippy::too_many_arguments)]
    fn complete_step_with_outcome(
        &mut self,
        step: &str,
        algorithm: Option<&str>,
        parameters: serde_json::Value,
        outcome: Option<(&str, serde_json::Value)>,
        inputs: &[&Path],
        output_paths: Vec<PathBuf>,
        start: Instant,
    ) -> crate::Result<()> {
        let prov_path = self.output.provenance_path(&self.run.key, step);
        let mut recorded = parameters.clone();
        if let Some((name, value)) = outcome {
            recorded[name] = value;
        }
        write_step_provenance(&prov_path, step, algorithm, &recorded, inputs, &output_paths, start)?;

        // Whatever read this step's previous output is stale now that it has been recomputed with
        // other parameters, or for the first time (a step newly switched on, say), which the
        // downstream steps' own parameter hashes would not notice. Recomputed with the same
        // parameters — its files were missing, or cleaned — it made the same output, and what
        // read it stays cached.
        let hash = step_cache_key(step, algorithm, &parameters, self.config);
        let same_as_before = self.state.completed_steps.get(step)
            .is_some_and(|r| r.params_hash.as_deref() == Some(hash.as_str()));
        if !same_as_before {
            for ds in crate::pipeline::graph::downstream_of(step) {
                self.state.completed_steps.remove(&ds);
            }
        }
        // Mark step done with params hash
        self.state.mark_completed(step, output_paths, Some(hash));
        self.state.save(self.state_path)
    }

    /// [`Self::complete_step`], plus step metadata kept in the state file for a later stage.
    #[allow(clippy::too_many_arguments)]
    fn complete_step_with_metadata(
        &mut self,
        step: &str,
        algorithm: Option<&str>,
        parameters: serde_json::Value,
        inputs: &[&Path],
        output_paths: Vec<PathBuf>,
        metadata: Option<serde_json::Value>,
        start: Instant,
    ) -> crate::Result<()> {
        self.complete_step(step, algorithm, parameters.clone(), inputs, output_paths.clone(), start)?;
        let hash = step_cache_key(step, algorithm, &parameters, self.config);
        self.state.mark_completed_with_metadata(step, output_paths, Some(hash), metadata);
        self.state.save(self.state_path)
    }
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
/// Each step saves its output to disk and drops data from memory.
/// On re-run, completed steps with valid outputs on disk are skipped.
pub fn run_pipeline_cached(
    qsm_run: &QsmRun,
    config: &PipelineConfig,
    output: &DerivativeOutputs,
    force: bool,
    progress: &dyn Fn(&str),
) -> crate::Result<()> {
    let state_path = output.state_path(&qsm_run.key);
    let mut state = PipelineState::load_or_create(&state_path, config, &qsm_run.key, force);

    let meta = stage_load(qsm_run, config, &mut state, &state_path, progress)?;
    restore_working_grid(qsm_run, output, &mut state, &state_path)?;

    // A run left on the axial grid it was resampled to names its outputs in that space (#275).
    let space = published_space(config, &meta);
    if state.space.as_deref() != space {
        // Every output of the last run is under the other names: nothing cached can be found,
        // and nothing under those names may stay beside the new ones.
        state.remove_recorded_outputs(&state_path);
        state.completed_steps.retain(|step, _| step == "load");
        state.space = space.map(str::to_string);
        state.save(&state_path)?;
    }
    let output = &output.in_space(space);

    // What this run can produce of what was enabled. `qsmxt run` has already refused a requested
    // output the data cannot give and warned about an implied one (`requirements::check_runs`);
    // here the same rules decide which stages run, so the two cannot disagree.
    let facts = requirements::RunFacts::of(qsm_run, config);
    let wanted = |o: requirements::Output| requirements::wanted(&facts, config, o);
    let do_relaxometry = wanted(requirements::Output::T2star) || wanted(requirements::Output::R2star);
    let needs_mask = config.pipeline.do_qsm || config.pipeline.do_swi || config.pipeline.do_smwi
        || do_relaxometry
        || config.pipeline.do_r2map || config.pipeline.do_r2primemap
        || config.pipeline.do_chi_separation;
    let needs_phase = needs_mask || config.pipeline.do_qsm
        || config.pipeline.do_segmentation || config.pipeline.do_analysis;

    let two_pass = two_pass_enabled(config, qsm_run);

    if !needs_phase {
        log::info!("No outputs enabled — nothing to process");
        state.mark_run_complete(config);
        state.save(&state_path)?;
        return Ok(());
    }

    let mut ctx = StageContext {
        run: qsm_run, config, output, meta: &meta,
        state: &mut state, state_path: &state_path,
    };

    stage_scale_phase(&mut ctx, progress)?;
    stage_magnitude(&mut ctx, progress)?;

    let brain_mask_path = output.mask_path(&qsm_run.key);
    if needs_mask {
        stage_mask(&mut ctx, &brain_mask_path, progress)?;
    }
    // Every stage after this reads the motion-corrected echoes (when on) and the mask every echo
    // still covers; the published brain mask stays the one masking made.
    let mask_path = if needs_mask { stage_motion(&mut ctx, &brain_mask_path, progress)? } else { brain_mask_path };

    if wanted(requirements::Output::Swi) {
        stage_swi(&mut ctx, &mask_path, progress)?;
    }

    // Before reconstruction, not after: a region reference is measured on the parcellation, so
    // referencing cannot run until this has. It needs only the combined magnitude, which
    // `stage_magnitude` has already written. Running it early also means a missing-weights
    // failure lands before a reconstruction has been paid for rather than after.
    let dseg_path = output.dseg_path(&qsm_run.key);
    if wanted(requirements::Output::Segmentation) {
        stage_segmentation(&mut ctx, &dseg_path, progress)?;
    }

    if do_relaxometry {
        stage_t2star_r2star(&mut ctx, &mask_path, progress)?;
    }

    if config.pipeline.do_r2map || config.pipeline.do_r2primemap {
        stage_r2_r2prime(&mut ctx, &mask_path, progress)?;
    }

    if !config.pipeline.do_qsm {
        log::info!("QSM processing disabled — skipping reconstruction");
    }

    if config.pipeline.do_qsm {
        let field_path = output.field_ppm_path(&qsm_run.key);
        // The total (unwrapped) field is not needed when reconstruction starts from raw
        // wrapped phase: TGV single-echo, the end-to-end iQSM/iQSM+, or iQFM field-prep
        // (which produces the local field directly from phase).
        // Which field the inversion takes is QSM.rs's call (`InversionConfig::input`, #265);
        // iQFM in front of a local-field inversion makes that field from phase itself.
        let input = qsmxt_config::bridge::inversion_input(config, meta.n_echoes);
        let need_field = input.needs_field_mapping()
            && !(input.needs_bg_removal() && config.bg_removal.algorithm == BfAlgorithm::Iqfm);

        // The field map is computed once, on the brain mask, and shared by both passes. The
        // reliable mask is a subset of it, so its pass needs no field values the shared map does
        // not already carry — and unwrapping twice would be a second ROMEO run for nothing.
        if need_field {
            stage_unwrap(&mut ctx, &mask_path, &field_path, progress)?;
        }

        let main = Pass::main(output, &qsm_run.key, &mask_path);
        let skip_bgremove = skips_bgremove(config);
        reconstruct(&mut ctx, &main, &field_path, progress)?;

        if two_pass {
            let reliable = Pass::reliable(output, &qsm_run.key);
            stage_two_pass_mask(&mut ctx, &mask_path, &reliable.mask, progress)?;
            reconstruct(&mut ctx, &reliable, &field_path, progress)?;
            stage_two_pass_combine(&mut ctx, &main, &reliable, skip_bgremove, progress)?;

            // Each map is referenced over the voxels it is defined on. The single-pass map's
            // support is the main pass's; the combined map takes each voxel from one pass or the
            // other, so its support is the union of both.
            stage_reference(
                &mut ctx, &[main.support(skip_bgremove)], &main.chi_raw,
                output.singlepass_qsm_path(&qsm_run.key), None, "reference-singlepass", progress,
            )?;
            let combined = output.two_pass_chi_raw_path(&qsm_run.key);
            stage_reference(
                &mut ctx, &[main.support(skip_bgremove), reliable.support(skip_bgremove)], &combined,
                output.qsm_path(&qsm_run.key), Some(output.qsm_support_path(&qsm_run.key)),
                "reference", progress,
            )?;
        } else {
            stage_reference(
                &mut ctx, &[main.support(skip_bgremove)], &main.chi_raw, output.qsm_path(&qsm_run.key),
                Some(output.qsm_support_path(&qsm_run.key)), "reference", progress,
            )?;
        }
    }

    // After referencing: SMWI weights by the final, referenced susceptibility map, over the voxels
    // that map is defined on — outside them χ is 0 by construction, not by measurement.
    if wanted(requirements::Output::Smwi) {
        let support = output.qsm_support_path(&qsm_run.key);
        let smwi_mask = if support.exists() { support } else { mask_path.clone() };
        stage_smwi(&mut ctx, &smwi_mask, progress)?;
    }

    if config.pipeline.do_chi_separation {
        stage_chi_separation(&mut ctx, &mask_path, progress)?;
    }

    // Last of the map-producing stages: it summarises every map on disk, so it has to run after
    // all of them — chi-separation included.
    if config.pipeline.do_analysis {
        stage_analysis(&mut ctx, &dseg_path, progress)?;
    }

    stage_output_space(&mut ctx, progress)?;

    ctx.state.mark_run_complete(config);
    ctx.state.save(&state_path)?;

    Ok(())
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
/// Cached steps hand their outputs to the steps after them on the working grid, but
/// [`stage_output_space`] rewrote them in place. Put the working-grid copies back and let it run
/// again at the end. Without the copies (a run from before they were kept), nothing in `anat/`
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

/// Put the derivatives back on the grid the data was acquired on.
///
/// A run that was resampled to axial reconstructs, and by default writes, on the cardinal grid.
/// That is the wrong place for anything the caller already holds in the acquired space — a FLIRT
/// matrix is defined in a coordinate space derived from the image's dimensions and voxel sizes,
/// so applying one to a volume on a different grid gives a wrong registration rather than an
/// error. `--output-space working` keeps the resampled grid for callers who want it.
///
/// Each kind of volume travels the way it must: masks by nearest neighbour, wrapped phase with
/// its magnitude through the complex domain, everything else trilinearly. This is a second
/// interpolation on top of the first, so the result is slightly smoother than a reconstruction
/// that was never resampled.
fn stage_output_space(ctx: &mut StageContext, progress: &dyn Fn(&str)) -> crate::Result<()> {
    let Some((dst_dims, dst_affine)) = ctx.meta.source_geometry else { return Ok(()) };
    if ctx.config.pipeline.output_space != crate::pipeline::config::OutputSpace::Acquired {
        log::info!("Leaving outputs on the resampled grid (--output-space working)");
        return record_working_space(ctx);
    }
    // `resampling` versions how each kind travels, so outputs returned by an older release (a
    // trilinearly interpolated dseg, SMWI minIPs left on the working grid; #267) are redone.
    let params = serde_json::json!({
        "space": "acquired", "dims": [dst_dims.0, dst_dims.1, dst_dims.2], "resampling": "by-kind",
    });
    if ctx.is_cached_with_params("output_space", None, &params) {
        return Ok(());
    }
    let t = Instant::now();
    progress("Resampling outputs to the acquired grid");
    log::info!(
        "Returning outputs to the acquired grid: {}x{}x{} -> {}x{}x{}",
        ctx.meta.dims.0, ctx.meta.dims.1, ctx.meta.dims.2, dst_dims.0, dst_dims.1, dst_dims.2
    );

    let src_dims = ctx.meta.dims;
    let src_affine = ctx.meta.affine;
    // Only this run's files: every run of the session shares `anat/`, and another run's outputs
    // may be on its own working grid, or still being read by a reconstruction in progress.
    let files: Vec<PathBuf> = ctx.output.run_published_niftis(&ctx.run.key).into_iter()
        // Not a file some other configuration left in another space.
        .filter(|p| p.file_name().and_then(|n| crate::bids::naming::entity_of(&n.to_string_lossy(), "space")).is_none())
        .collect();

    // Keep the working-grid originals: later steps read these back (the mask, the combined
    // magnitude, ...), so a re-run has to start from them rather than from what lands in `anat/`.
    let stash = ctx.output.working_grid_dir(&ctx.run.key);
    if stash.exists() {
        std::fs::remove_dir_all(&stash)?;
    }
    std::fs::create_dir_all(&stash)?;
    for f in &files {
        if let Some(name) = f.file_name() {
            std::fs::copy(f, stash.join(name))?;
        }
    }

    // How each file travels is decided by what it is (#267), not by its name.
    use crate::bids::derivatives::OutputKind;
    let kind = |p: &Path| ctx.output.output_kind(&ctx.run.key, p);

    // Phase is only meaningful alongside its magnitude, so handle those pairs first and skip
    // them in the scalar pass.
    let mut done: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    // Projections are on their own, shorter grids, so they cannot travel with the volumes that
    // share the working grid; each is rebuilt from its resampled source below.
    for f in files.iter().filter(|p| kind(p) == OutputKind::Projection) {
        done.insert(f.clone());
    }
    for phase_path in files.iter().filter(|p| kind(p) == OutputKind::Phase) {
        let mag_path = PathBuf::from(phase_path.to_string_lossy().replace("part-phase", "part-mag"));
        if !mag_path.exists() {
            log::warn!(
                "Leaving {} on the working grid: wrapped phase can only be resampled with its \
                 magnitude, and there is none beside it", phase_path.display());
            done.insert(phase_path.clone());
            continue;
        }
        let (phase, mag) = (load_volume(phase_path)?, load_volume(&mag_path)?);
        let resampled = qsm_core::geometry::resample_complex_onto(
            &mag, &phase, src_dims, &src_affine, dst_dims, &dst_affine,
        );
        if let Some((new_mag, new_phase)) = resampled {
            write_on_grid(phase_path, &new_phase, dst_dims, &dst_affine)?;
            write_on_grid(&mag_path, &new_mag, dst_dims, &dst_affine)?;
            done.insert(phase_path.clone());
            done.insert(mag_path);
        }
    }

    let src_voxels = src_dims.0 * src_dims.1 * src_dims.2;
    for f in files.iter().filter(|f| !done.contains(*f)) {
        let data = load_volume(f)?;
        // Resampling indexes the source by the working grid, so anything on a different grid
        // would be read out of bounds. Leave it where it is rather than corrupt it.
        if data.len() != src_voxels {
            log::warn!(
                "Leaving {} on its own grid: {} voxels, not the {} of the working grid",
                f.display(), data.len(), src_voxels,
            );
            continue;
        }
        match kind(f) {
            OutputKind::Mask => {
                let m: Vec<u8> = data.iter().map(|v| if *v > 0.5 { 1u8 } else { 0u8 }).collect();
                if let Some(out) = qsm_core::geometry::resample_mask_onto(&m, src_dims, &src_affine, dst_dims, &dst_affine) {
                    let as_f64: Vec<f64> = out.iter().map(|v| *v as f64).collect();
                    write_on_grid(f, &as_f64, dst_dims, &dst_affine)?;
                }
            }
            // A label id is a name, not a quantity: interpolating between two ids invents a third.
            OutputKind::Labels => {
                let out = qsm_core::geometry::resample_labels_onto(&data, src_dims, &src_affine, dst_dims, &dst_affine)?;
                write_on_grid(f, &out, dst_dims, &dst_affine)?;
            }
            OutputKind::Continuous | OutputKind::Phase | OutputKind::Projection => {
                if let Some(out) = qsm_core::geometry::resample_onto(&data, src_dims, &src_affine, dst_dims, &dst_affine) {
                    write_on_grid(f, &out, dst_dims, &dst_affine)?;
                }
            }
        }
    }

    // A projection cannot be interpolated onto the acquired grid as if it were a volume — its
    // slices are slabs, not samples — so each is rebuilt from the volume that just landed there:
    // the SWI minIP from the SWI, each SMWI minIP from its SMWI.
    let dst_voxel_size = qsm_core::geometry::voxel_sizes_from_affine(&dst_affine);
    let dst_grid = qsm_core::Grid::new(
        dst_dims.0, dst_dims.1, dst_dims.2,
        dst_voxel_size.0, dst_voxel_size.1, dst_voxel_size.2,
    );
    let mut projections = vec![(ctx.output.swi_mip_path(&ctx.run.key), ctx.output.swi_path(&ctx.run.key),
                                ctx.config.swi.mip_window)];
    for contrast in ["paramagnetic", "diamagnetic"] {
        projections.push((ctx.output.smwi_mip_path(&ctx.run.key, contrast),
                          ctx.output.smwi_path(&ctx.run.key, contrast), ctx.config.smwi.mip_window));
    }
    for (mip_path, source, window) in projections {
        if !(files.contains(&mip_path) && source.exists()) {
            continue;
        }
        let volume = load_volume(&source)?;
        let mip = qsm_core::swi::create_mip(&volume, &dst_grid, &dst_affine, window)
            .map_err(QsmxtError::Config)?;
        write_volume(&mip_path, &mip.data, mip.grid.dims, mip.grid.voxel_size, &mip.affine)?;
    }

    // Used: the working-grid originals each output was resampled from.
    let originals: Vec<PathBuf> = files.iter()
        .filter_map(|f| f.file_name().map(|n| stash.join(n)))
        .collect();
    let original_refs: Vec<&Path> = originals.iter().map(|p| p.as_path()).collect();
    ctx.complete_step("output_space", None, params, &original_refs, files, t)?;
    log_step_done("Output space", t);
    Ok(())
}

/// Give each of the run's outputs left on the working grid a `SpatialReference`: the BIDS
/// sidecar field a `space-` label that is not a standard template needs, pointing at the run's
/// combined magnitude in that space.
fn record_working_space(ctx: &StageContext) -> crate::Result<()> {
    let Some(space) = ctx.output.space.as_deref() else { return Ok(()) };
    let reference = crate::pipeline::multiorient::spatial_reference(ctx.output, &ctx.run.key, space);
    let files: Vec<PathBuf> = ctx.output.run_published_niftis(&ctx.run.key).into_iter()
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

/// Overwrite a NIfTI with data on a different grid.
fn write_on_grid(path: &Path, data: &[f64], dims: (usize, usize, usize), affine: &[f64; 16]) -> crate::Result<()> {
    let voxel_size = qsm_core::geometry::voxel_sizes_from_affine(affine);
    write_volume(path, data, dims, voxel_size, affine)
}

/// The box the FFT-based stages reconstruct in.
///
/// Two levers on the same cost, both opt-in, for different reasons.
///
/// `--fft-padding` grows the grid to a size `rustfft` likes. It discards nothing and an awkward
/// grid can get most of a transform's time back, but it changes where the dipole kernel is
/// sampled in k-space, so the reconstruction shifts slightly. `--crop-to-mask` reconstructs only
/// around the brain, which is faster still but genuinely discards field. See `qsm_core::crop`.
fn crop_box_for(ctx: &StageContext, mask: &[u8], stage: &str) -> qsm_core::crop::CropBox {
    let cropping = ctx.config.pipeline.crop_to_mask;
    let margin = ctx.config.pipeline.crop_margin_mm;
    let b = reconstruction_box(
        cropping, ctx.config.pipeline.fft_padding,
        mask, ctx.meta.dims, ctx.meta.voxel_size, margin,
    );

    if b.dims == ctx.meta.dims {
        return b;
    }
    if cropping {
        log::info!(
            "{}: reconstructing in {}x{}x{} instead of {}x{}x{} ({:.1}x fewer voxels, {:.0} mm margin)",
            stage, b.dims.0, b.dims.1, b.dims.2,
            b.full_dims.0, b.full_dims.1, b.full_dims.2, b.reduction(), margin,
        );
    } else {
        log::info!(
            "{}: padding {}x{}x{} to {}x{}x{} for a cheaper FFT",
            stage, b.full_dims.0, b.full_dims.1, b.full_dims.2,
            b.dims.0, b.dims.1, b.dims.2,
        );
    }
    b
}

/// Which box, given the setting — split out from the logging so it can be tested directly.
///
/// Falls back to the full grid when the chosen box would not change it, since copying volumes in
/// and out only pays for itself if the grid actually moves.
fn reconstruction_box(
    crop_to_mask: bool,
    fft_padding: bool,
    mask: &[u8],
    dims: (usize, usize, usize),
    voxel_size: (f64, f64, f64),
    margin_mm: f64,
) -> qsm_core::crop::CropBox {
    let b = if crop_to_mask {
        qsm_core::crop::crop_box_for_mask(mask, dims, voxel_size, margin_mm)
    } else if fft_padding {
        qsm_core::crop::fft_pad_box(dims)
    } else {
        qsm_core::crop::CropBox::full(dims)
    };
    if b.dims == dims {
        qsm_core::crop::CropBox::full(dims)
    } else {
        b
    }
}

/// Work out the grid and B0 direction the pipeline will actually reconstruct on.
///
/// The dipole kernel is built in voxel space, so an oblique acquisition has to be handled one of
/// two ways, and doing neither is what leaves susceptibility contrast on the floor:
///
/// 1. **Resample to a cardinal-aligned grid** (`--obliquity-threshold`, as QSMxT 8.x did), after
///    which B0 is `(0,0,1)` by construction. Costs one interpolation of every echo.
/// 2. **Keep the grid and rotate the kernel**, using the true B0 direction from the affine.
///
/// A sidecar `B0_dir` always wins, since it describes the acquisition better than the affine can.
/// How far B0 may sit from the slice normal before an axial-only algorithm is given resampled
/// data. Acquisitions carry sub-degree tilts from rounding in the affine; resampling for those
/// costs an interpolation and buys nothing.
const AXIAL_ONLY_TOLERANCE_DEG: f64 = 1.0;

/// What the orientation gate cannot read from the config alone.
///
/// Only whether a measured R2 exists, which is what decides whether `--r2prime-strategy auto`
/// falls back to R2PRIMEnet. Both sources are judged optimistically — a declared
/// `--custom-r2-tool` whose derivative turns out to be missing, or a MESE that turns out to be
/// unreadable, both leave `auto` predicting R2′ after all. `stage_r2_r2prime` warns when that
/// happens on a grid that was left oblique.
fn run_orientation_facts(
    run: &QsmRun,
    config: &PipelineConfig,
) -> qsmxt_config::bridge::RunOrientationFacts {
    qsmxt_config::bridge::RunOrientationFacts {
        has_measured_r2: run.mese.is_some() || config.separation.custom_r2_tool.is_some(),
    }
}

fn resolve_geometry(
    run: &QsmRun,
    first_phase: &NiftiData,
    config: &PipelineConfig,
) -> crate::Result<RunMetadata> {
    let affine = first_phase.affine;
    let obliquity = qsm_core::geometry::obliquity_from_affine(&affine);
    let tilt = qsm_core::geometry::b0_angle_from_affine(&affine);
    let threshold = config.pipeline.obliquity_threshold;
    let axes = qsm_core::geometry::obliquity_axes_from_affine(&affine);

    let mut meta = RunMetadata {
        dims: first_phase.dims,
        voxel_size: first_phase.voxel_size,
        affine,
        n_echoes: run.echoes.len(),
        echo_times: run.echo_times.clone(),
        b0_direction: (0.0, 0.0, 1.0),
        field_strength: run.magnetic_field_strength,
        has_magnitude: run.has_magnitude,
        source_geometry: None, slice_geometry: None,
    };

    if obliquity > 0.01 {
        log::info!(
            "Obliquity {:.1}° (per-axis {:.1}/{:.1}/{:.1}°); B0 is {:.1}° from the slice normal",
            obliquity, axes[0], axes[1], axes[2], tilt
        );
    }

    // An algorithm that only understands axial data settles the question before anything else
    // does. It takes no B0 direction, so neither a sidecar nor the affine can help it: the only
    // way to reconstruct correctly is to move the data. Below the obliquity threshold the tilt
    // is small enough not to bother.
    let axial_only = qsmxt_config::bridge::axial_only_algorithms(config, run_orientation_facts(run, config));
    let forced_axial = !axial_only.is_empty() && tilt > AXIAL_ONLY_TOLERANCE_DEG;
    if forced_axial {
        log::info!(
            "Resampling to axial because {} {} no B0 direction and {} assume it is +z; B0 is \
             {:.1}° from the slice normal here",
            axial_only.join(", "),
            if axial_only.len() == 1 { "takes" } else { "take" },
            if axial_only.len() == 1 { "it assumes" } else { "they assume" },
            tilt
        );
    }

    // A sidecar B0_dir is authoritative for anything that can use a direction at all.
    if !forced_axial {
        if let Some(dir) = run.b0_dir {
            meta.b0_direction = dir;
            log::info!("B0 direction {:?} (from the JSON sidecar)", dir);
            return Ok(meta);
        }
    }

    let resample = forced_axial || (threshold >= 0.0 && obliquity > threshold);
    if resample {
        // A matched MESE used to veto this, on both branches: the GRE would move to a cardinal
        // grid and the MESE, read straight from BIDS, would be left behind. It no longer does,
        // because the MESE is no longer assumed to share the GRE's grid — `load_mese_voxel_major`
        // resamples it onto whatever grid is chosen here. That also makes a MESE acquired on its
        // own grid usable, which is the ordinary case and used to be discarded.
        let grid = qsm_core::geometry::axial_grid_for(
            first_phase.dims.0, first_phase.dims.1, first_phase.dims.2, &affine,
        );
        let why = if forced_axial {
            "required by the chosen algorithm".to_string()
        } else {
            format!("obliquity {obliquity:.1}° > threshold {threshold:.1}°")
        };
        log::info!(
            "Resampling to axial: {}x{}x{} -> {}x{}x{} ({}); B0 becomes (0, 0, 1)",
            first_phase.dims.0, first_phase.dims.1, first_phase.dims.2,
            grid.dims.0, grid.dims.1, grid.dims.2, why
        );
        if run.mese.is_some() {
            log::info!("The matched MESE acquisition will be resampled onto the same grid");
        }
        meta.source_geometry = Some((first_phase.dims, affine));
        meta.dims = grid.dims;
        meta.voxel_size = grid.voxel_size;
        meta.affine = grid.affine;
        meta.b0_direction = (0.0, 0.0, 1.0);
        return Ok(meta);
    }

    // No resampling: build the kernel on the acquired grid, with B0 where it actually points.
    meta.b0_direction = qsm_core::geometry::b0_direction_from_affine(&affine);
    if tilt > 0.5 {
        log::info!(
            "B0 direction ({:.3}, {:.3}, {:.3}) from the affine; pass --obliquity-threshold to \
             resample to axial instead",
            meta.b0_direction.0, meta.b0_direction.1, meta.b0_direction.2
        );
    }
    Ok(meta)
}

/// qsm-core's scan metadata for a stage working on `dims` (the run's grid, or a crop of it, which
/// keeps its voxel size), with the slice geometry for qsm-core's slice-gap check.
fn scan_metadata(meta: &RunMetadata, dims: (usize, usize, usize)) -> qsm_core::pipeline::ScanMetadata {
    let mut m = crate::pipeline::config::to_scan_metadata(
        dims, meta.voxel_size, &meta.echo_times, meta.field_strength, meta.b0_direction,
    );
    m.slice_geometry = meta.slice_geometry.map(|(thickness, axis)| qsm_core::grid::SliceGeometry { thickness, axis });
    m
}

/// The acquisition's slices, as `(thickness mm, voxel axis)`, when the sidecar says enough to
/// check them for a gap: `SliceThickness`, and either `MRAcquisitionType` "2D" or a thickness
/// that differs from the pitch (a 3D acquisition records its partition thickness, the pitch, so
/// there is nothing to check). The axis is `SliceEncodingDirection`'s, else the coarsest one.
fn slice_geometry_from_sidecar(
    sc: &crate::bids::sidecar::QsmSidecar, voxel_size: (f64, f64, f64),
) -> Option<(f64, usize)> {
    let thickness = sc.slice_thickness?;
    let vs = [voxel_size.0, voxel_size.1, voxel_size.2];
    let axis = match sc.slice_encoding_direction.as_deref().map(|d| d.trim_end_matches('-')) {
        Some("i") => 0,
        Some("j") => 1,
        Some("k") => 2,
        _ => (0..3).rev().max_by(|&a, &b| vs[a].total_cmp(&vs[b])).unwrap_or(2),
    };
    let is_2d = sc.mr_acquisition_type.as_deref().is_some_and(|t| t.eq_ignore_ascii_case("2D"));
    let differs = (thickness - vs[axis]).abs() > 0.01 * vs[axis];
    (is_2d || differs).then_some((thickness, axis))
}

/// Refuse a gapped acquisition before any work, unless `--ignore-slice-gap` (or nothing that
/// needs a contiguous grid was asked for). Checked on the acquired grid, so it holds whether or
/// not the run is then resampled. Returns the geometry to hand qsm-core (on the working grid).
fn check_slice_gap(
    run: &QsmRun, first_phase: &NiftiData, meta: &RunMetadata, config: &PipelineConfig,
) -> crate::Result<Option<(f64, usize)>> {
    let Some(sc) = crate::bids::sidecar::read_sidecar(&run.echoes[0].phase_json).ok() else { return Ok(None) };
    let Some((thickness, axis)) = slice_geometry_from_sidecar(&sc, first_phase.voxel_size) else { return Ok(None) };
    let (nx, ny, nz) = first_phase.dims;
    let (vx, vy, vz) = first_phase.voxel_size;
    let grid = qsm_core::Grid::new(nx, ny, nz, vx, vy, vz);
    if let Err(gap) = grid.require_contiguous_slices(thickness, axis) {
        if config.pipeline.ignore_slice_gap {
            log::warn!("{gap}; reconstructing anyway (--ignore-slice-gap): treat the result as unreliable");
            return Ok(None);
        }
        if config.pipeline.do_qsm {
            return Err(QsmxtError::Config(format!(
                "{gap} (from the sidecar's SliceThickness). QSM needs contiguous slices; pass \
                 --ignore-slice-gap to reconstruct anyway")));
        }
        log::warn!("{gap}; no QSM was asked for, so continuing");
        return Ok(None);
    }
    // Contiguous: hand the geometry on, unless resampling moved the slices off the grid's axes.
    Ok(if meta.source_geometry.is_none() { Some((thickness, axis)) } else { None })
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

fn stage_load(
    qsm_run: &QsmRun,
    config: &PipelineConfig,
    state: &mut PipelineState,
    state_path: &Path,
    progress: &dyn Fn(&str),
) -> crate::Result<RunMetadata> {
    // The grid every other step works on is decided here, so a setting that changes it leaves
    // nothing cached valid. A cache from before this was recorded is taken as it stands.
    let geometry = serde_json::json!({
        "obliquity_threshold": config.pipeline.obliquity_threshold,
        "ignore_slice_gap": config.pipeline.ignore_slice_gap,
        // Everything after masking reads the motion-corrected echoes, and no later step's cache
        // key says so; changing these recomputes the run, as a geometry change does.
        "motion_correction": config.motion_correction,
        "registration": if config.motion_correction.enabled { Some(&config.registration) } else { None },
        "axial_only": qsmxt_config::bridge::axial_only_algorithms(
            config, run_orientation_facts(qsm_run, config)),
        // Replacing an input in place (a re-export, a corrected sidecar) changes everything
        // computed from it; nothing else would notice.
        "inputs": input_identity(qsm_run),
    });
    let geometry_hash = crate::pipeline::graph::step_params_hash(None, &geometry);
    if let Some(Some(stored)) = state.completed_steps.get("load").map(|r| r.params_hash.as_ref()) {
        if *stored != geometry_hash {
            log::info!("Input files or geometry settings changed (obliquity threshold, algorithm, \
                        motion correction) — recomputing everything");
            state.completed_steps.clear();
        }
    }
    if !state.is_step_cached("load") {
        let t = Instant::now();
        progress("Loading NIfTI metadata");
        let first_phase = io::read_nifti_file(&qsm_run.echoes[0].phase_nifti)
            .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", qsm_run.echoes[0].phase_nifti.display(), e)))?;
        validate_run_dims(qsm_run, &first_phase)?;

        let mut meta = resolve_geometry(qsm_run, &first_phase, config)?;
        meta.slice_geometry = check_slice_gap(qsm_run, &first_phase, &meta, config)?;
        log::info!(
            "Volume: {}x{}x{}, {:.2}x{:.2}x{:.2}mm, {} echoes{}, B0={:.1}T, TEs={:?}s",
            first_phase.dims.0, first_phase.dims.1, first_phase.dims.2,
            first_phase.voxel_size.0, first_phase.voxel_size.1, first_phase.voxel_size.2,
            meta.n_echoes,
            qsm_run.coils.as_ref().map(|c| format!(" x {} uncombined coils", c.len())).unwrap_or_default(),
            meta.field_strength, meta.echo_times,
        );
        state.run_metadata = Some(meta.clone());
        state.mark_completed("load", vec![], Some(geometry_hash));
        state.save(state_path)?;
        log_step_done("Load", t);
        Ok(meta)
    } else {
        log::info!("Skipping load (cached)");
        state.run_metadata.clone().ok_or_else(|| {
            QsmxtError::Config("Cached state missing run metadata".to_string())
        })
    }
}

/// Retrospective motion correction: rigidly co-register every echo onto the reference echo
/// (estimated on magnitude, applied to phase through the complex signal) and shrink the mask to
/// where every echo still has data. Returns the mask the later stages should use.
fn stage_motion(ctx: &mut StageContext, mask_path: &Path, progress: &dyn Fn(&str)) -> crate::Result<PathBuf> {
    let m = &ctx.config.motion_correction;
    if !m.enabled {
        forget_motion(ctx)?;
        return Ok(mask_path.to_path_buf());
    }
    if !ctx.meta.has_magnitude {
        return Err(QsmxtError::Config(
            "--motion-correction needs magnitude images: the registration is estimated on magnitude, \
             and this run has none".into()));
    }
    if ctx.meta.n_echoes < 2 {
        log::warn!("Motion correction needs at least two echoes; this run has one, so it is skipped");
        forget_motion(ctx)?;
        return Ok(mask_path.to_path_buf());
    }
    let out_mask = ctx.output.motion_coverage_path(&ctx.run.key);
    let params = serde_json::json!({
        "motion_correction": m, "registration": ctx.config.registration, "n_echoes": ctx.meta.n_echoes,
    });
    if ctx.is_cached_with_params("motion", None, &params) && out_mask.exists() {
        log::info!("Skipping motion correction (cached)");
        return Ok(out_mask);
    }
    let t = Instant::now();
    progress("Motion correction (rigid co-registration of the echoes)");
    let n = ctx.meta.n_echoes;
    if m.reference != qsmxt_config::MotionReference::First
        && ctx.config.masking.sections.iter().any(|s| s.input == MaskingInput::MagnitudeFirst)
    {
        log::warn!("Motion correction aligns the echoes onto echo {} but the mask was made on the \
                    first echo's magnitude; if the head moved, the mask is offset from the corrected \
                    echoes", m.reference);
    }
    let mut phases = Vec::with_capacity(n);
    let mut mags = Vec::with_capacity(n);
    let mut inputs: Vec<PathBuf> = Vec::with_capacity(2 * n + 1);
    for e in 1..=n {
        let p = ctx.output.phase_scaled_path(&ctx.run.key, e);
        let q = ctx.output.mag_path(&ctx.run.key, e);
        phases.push(load_volume(&p)?);
        mags.push(load_volume(&q)?);
        inputs.push(p);
        inputs.push(q);
    }
    let mask = load_mask(mask_path)?;
    let phase_refs: Vec<&[f64]> = phases.iter().map(|v| v.as_slice()).collect();
    let mag_refs: Vec<&[f64]> = mags.iter().map(|v| v.as_slice()).collect();
    let meta = crate::pipeline::config::to_scan_metadata(
        ctx.meta.dims, ctx.meta.voxel_size, &ctx.meta.echo_times, ctx.meta.field_strength, ctx.meta.b0_direction,
    );
    let r = qsm_core::pipeline::run_motion_correction(
        &phase_refs, &mag_refs, &mask, &meta, &m.to_core(&ctx.config.registration), &mut |_, _| {},
    ).map_err(|e| QsmxtError::Config(format!("motion correction: {e}")))?;

    let mut outputs = Vec::with_capacity(2 * n + 2);
    for e in 0..n {
        let p = ctx.output.motion_phase_path(&ctx.run.key, e + 1);
        let q = ctx.output.motion_mag_path(&ctx.run.key, e + 1);
        save_volume(&p, &r.phases[e], ctx.meta)?;
        save_volume(&q, &r.magnitudes[e], ctx.meta)?;
        outputs.push(p);
        outputs.push(q);
    }
    save_mask(&out_mask, &r.mask, ctx.meta)?;
    outputs.push(out_mask.clone());
    let report = motion_report_json(&r);
    let report_path = ctx.output.motion_report_path(&ctx.run.key);
    std::fs::write(&report_path, serde_json::to_string_pretty(&report)
        .map_err(|e| QsmxtError::Config(format!("motion report: {e}")))?)?;
    outputs.push(report_path);

    log_motion_summary(&r, &mask);
    let input_refs: Vec<&Path> = inputs.iter().map(|p| p.as_path()).chain(std::iter::once(mask_path)).collect();
    ctx.complete_step("motion", None, params, &input_refs, outputs, t)?;
    log_step_done("Motion correction", t);
    Ok(out_mask)
}

/// Motion correction did not run this time. If an earlier run of this configuration did, what
/// it left is stale: its coverage mask must not be published as this run's, and every stage that
/// read the corrected echoes or that mask has to run again on the uncorrected ones.
fn forget_motion(ctx: &mut StageContext) -> crate::Result<()> {
    let coverage = ctx.output.motion_coverage_path(&ctx.run.key);
    if coverage.exists() {
        std::fs::remove_file(&coverage)?;
    }
    if ctx.state.completed_steps.contains_key("motion") {
        log::info!("Motion correction is off now: re-running the stages that read its output");
        ctx.state.invalidate("motion");
        ctx.state.save(ctx.state_path)?;
    }
    Ok(())
}

/// The motion report: per echo (1-based) rotation, translation, largest tissue displacement,
/// NCC, overlap and whether the registration is suspect, plus the series' extremes.
fn motion_report_json(r: &qsm_core::pipeline::MotionCorrectionResult) -> serde_json::Value {
    serde_json::json!({
        "reference_echo": r.motion.reference + 1,
        "max_displacement_mm": r.max_displacement_mm(),
        "rotation_spread_deg": r.rotation_spread_deg(),
        "echoes": r.motion.volumes.iter().map(|v| serde_json::json!({
            "echo": v.index + 1,
            "rotation_deg": v.rotation_deg,
            "translation_mm": v.translation_mm,
            "max_displacement_mm": v.max_displacement_mm,
            "ncc": v.ncc,
            "overlap": v.overlap,
            "suspect": v.suspect,
            "b0_direction": [v.bdir.0, v.bdir.1, v.bdir.2],
        })).collect::<Vec<_>>(),
    })
}

fn log_motion_summary(r: &qsm_core::pipeline::MotionCorrectionResult, mask_in: &[u8]) {
    for v in &r.motion.volumes {
        log::info!("  echo {}: rotation {:.2}°, translation {:.2} mm, max displacement {:.2} mm, NCC {:.3}, overlap {:.2}{}",
            v.index + 1, v.rotation_deg, v.translation_mm, v.max_displacement_mm, v.ncc, v.overlap,
            if v.suspect { "  [suspect registration]" } else { "" });
    }
    log::info!("Motion: max displacement {:.2} mm, B0 direction spread {:.2}° (field mapping uses the reference echo's)",
        r.max_displacement_mm(), r.rotation_spread_deg());
    let suspect: Vec<String> = r.motion.volumes.iter().filter(|v| v.suspect).map(|v| (v.index + 1).to_string()).collect();
    if !suspect.is_empty() {
        log::warn!("Motion correction: the registration of echo(es) {} may have failed (low NCC or overlap); \
                    check the motion report", suspect.join(", "));
    }
    let (before, after) = (count_set(mask_in), count_set(&r.mask));
    if after < before {
        log::info!("Mask restricted to voxels every echo covers: {} -> {} voxels", before, after);
    }
}

/// Whether real/imaginary input from this manufacturer has its phase negated on conversion.
///
/// GE's real/imaginary pair gives phase of the opposite sign to the Siemens/Philips convention
/// the pipeline assumes: on one subject scanned on all three, high-pass field maps from GE
/// anti-correlate with the other two, which correlate with each other. Matched as dcm2niix and
/// the DICOM tag spell it ("GE", "GE MEDICAL SYSTEMS", "General Electric").
fn negates_real_imaginary_phase(manufacturer: Option<&str>) -> bool {
    let Some(m) = manufacturer else { return false };
    let m = m.trim().to_uppercase();
    m == "GE" || m.starts_with("GE ") || m.contains("GENERAL ELECTRIC")
}

/// Real/imaginary echoes of a run as magnitude and phase on its acquired grid, via
/// `qsm_core::complex::complex_to_mag_phase` (which also detects and undoes slice chopping, and
/// negates the phase when `negate_phase`).
fn convert_real_imaginary(ctx: &StageContext, negate_phase: bool) -> crate::Result<qsm_core::complex::MagPhaseEchoes> {
    let (dims, voxel) = match ctx.meta.source_geometry {
        Some((d, a)) => (d, qsm_core::geometry::voxel_sizes_from_affine(&a)),
        None => (ctx.meta.dims, ctx.meta.voxel_size),
    };
    let n = dims.0 * dims.1 * dims.2;
    let mut real = Vec::with_capacity(ctx.run.echoes.len());
    let mut imag = Vec::with_capacity(ctx.run.echoes.len());
    for echo in &ctx.run.echoes {
        let imag_path = echo.imaginary_nifti.as_ref().ok_or_else(|| QsmxtError::Config(format!(
            "echo {}: real/imaginary run without a part-imag file", echo.echo_number)))?;
        for (path, out) in [(&echo.phase_nifti, &mut real), (imag_path, &mut imag)] {
            let v = load_volume(path)?;
            if v.len() != n {
                return Err(QsmxtError::DimensionMismatch(format!(
                    "{} has {} voxels, expected {}", path.display(), v.len(), n)));
            }
            out.push(v);
        }
    }
    let grid = qsm_core::Grid::new(dims.0, dims.1, dims.2, voxel.0, voxel.1, voxel.2);
    let real_refs: Vec<&[f64]> = real.iter().map(|v| v.as_slice()).collect();
    let imag_refs: Vec<&[f64]> = imag.iter().map(|v| v.as_slice()).collect();
    let params = qsm_core::complex::ComplexToMagPhaseParams {
        chop: qsm_core::complex::ChopCorrection::Auto,
        negate_phase,
    };
    let out = qsm_core::complex::complex_to_mag_phase(&real_refs, &imag_refs, &grid, &params);
    if negate_phase {
        log::info!("Real/imaginary input: GE phase sign convention, phase negated");
    }

    let c = &out.chopping;
    let axes: Vec<&str> = ["x", "y", "z"].into_iter().zip(c.corrected).filter(|(_, on)| *on).map(|(a, _)| a).collect();
    log::info!("Real/imaginary input: neighbour phase coherence x {:+.3}, y {:+.3}, z {:+.3}; {}",
        c.coherence[0], c.coherence[1], c.coherence[2],
        if axes.is_empty() { "no slice chopping".to_string() } else { format!("slice chopping corrected along {}", axes.join(", ")) });
    let ambiguous = c.ambiguous_axes();
    if !ambiguous.is_empty() {
        let names: Vec<&str> = ambiguous.iter().map(|&a| ["x", "y", "z"][a]).collect();
        log::warn!("Real/imaginary input: slice-chopping detection is uncertain along {} \
                    (coherence near zero); check the phase for a π jump between neighbouring slices",
            names.join(", "));
    }
    Ok(out)
}

/// Provenance record of a real/imaginary conversion.
fn slice_chopping_json(c: &qsm_core::complex::SliceChopping) -> serde_json::Value {
    serde_json::json!({
        "method": "complex_to_mag_phase",
        "detected": c.detected,
        "coherence": { "x": c.coherence[0], "y": c.coherence[1], "z": c.coherence[2] },
        "corrected": { "x": c.corrected[0], "y": c.corrected[1], "z": c.corrected[2] },
    })
}

fn stage_scale_phase(ctx: &mut StageContext, progress: &dyn Fn(&str)) -> crate::Result<()> {
    if ctx.run.coils.is_some() {
        return stage_combine_coils(ctx, progress);
    }
    let complex = ctx.run.echoes[0].imaginary_nifti.is_some();
    let mut params = serde_json::json!({
        "resampled_to_axial": ctx.meta.source_geometry.is_some(),
        "dims": [ctx.meta.dims.0, ctx.meta.dims.1, ctx.meta.dims.2],
    });
    // The sign convention follows from the sidecar, so it is a parameter (and keys the cache).
    let negate_phase = complex && negates_real_imaginary_phase(
        crate::bids::sidecar::read_sidecar(&ctx.run.echoes[0].phase_json)?.manufacturer.as_deref());
    if complex {
        params["input"] = serde_json::json!("real_imaginary");
        params["negate_phase"] = serde_json::json!(negate_phase);
    }
    if ctx.is_cached_with_params("scale_phase", None, &params) {
        log::info!("Skipping scale_phase (cached)");
        return Ok(());
    }
    let t = Instant::now();
    // Real/imaginary input becomes magnitude and phase before anything else sees it.
    let converted = if complex {
        progress("Converting real/imaginary to magnitude/phase");
        Some(convert_real_imaginary(ctx, negate_phase)?)
    } else {
        None
    };
    if ctx.meta.source_geometry.is_some() {
        progress("Resampling to axial + rescaling phase");
    } else {
        progress("Rescaling phase to radians");
    }
    let mut phase_paths = Vec::new();
    let mut mag_paths = Vec::new();

    if let Some((src_dims, src_affine)) = ctx.meta.source_geometry {
        // Oblique run: bring every echo onto the cardinal grid before anything else sees it.
        // Magnitude and phase go together through the complex domain, because interpolating
        // wrapped phase on its own turns every wrap into a band of wrong values.
        let (nx, ny, nz) = src_dims;
        let params = qsm_core::geometry::AxialResampleParams::default();
        for (i, echo) in ctx.run.echoes.iter().enumerate() {
            let (mag, phase) = match &converted {
                Some(c) => (c.magnitudes[i].clone(), c.phases[i].clone()),
                None => {
                    let mut phase_nifti = io::read_nifti_file(&echo.phase_nifti)
                        .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", echo.phase_nifti.display(), e)))?;
                    qsm_core::pipeline::scale_phase_to_pi(&mut phase_nifti.data);
                    let mag = match echo.magnitude_nifti.as_ref() {
                        Some(p) => io::read_nifti_file(p)
                            .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", p.display(), e)))?
                            .data,
                        // No magnitude: weight every voxel equally so the phase still resamples correctly.
                        None => vec![1.0; phase_nifti.data.len()],
                    };
                    (mag, phase_nifti.data)
                }
            };
            let out = qsm_core::geometry::resample_complex_to_axial(
                &mag, &phase, nx, ny, nz, &src_affine, &params,
            );
            let p_path = ctx.output.phase_scaled_path(&ctx.run.key, i + 1);
            save_volume(&p_path, &out.phase, ctx.meta)?;
            phase_paths.push(p_path);
            if ctx.run.has_magnitude {
                let m_path = ctx.output.mag_path(&ctx.run.key, i + 1);
                save_volume(&m_path, &out.magnitude, ctx.meta)?;
                mag_paths.push(m_path);
            }
        }
    } else if let Some(c) = &converted {
        for i in 0..ctx.run.echoes.len() {
            let p_path = ctx.output.phase_scaled_path(&ctx.run.key, i + 1);
            save_volume(&p_path, &c.phases[i], ctx.meta)?;
            phase_paths.push(p_path);
            let m_path = ctx.output.mag_path(&ctx.run.key, i + 1);
            save_volume(&m_path, &c.magnitudes[i], ctx.meta)?;
            mag_paths.push(m_path);
        }
    } else {
        for (i, echo) in ctx.run.echoes.iter().enumerate() {
            let mut phase_nifti = io::read_nifti_file(&echo.phase_nifti)
                .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", echo.phase_nifti.display(), e)))?;
            qsm_core::pipeline::scale_phase_to_pi(&mut phase_nifti.data);
            let out_path = ctx.output.phase_scaled_path(&ctx.run.key, i + 1);
            save_volume(&out_path, &phase_nifti.data, ctx.meta)?;
            phase_paths.push(out_path);
        }

        // Save raw (uncorrected) per-echo magnitudes as intermediates
        // (needed by MCPC-3D-S, linear fit, ROMEO)
        if ctx.run.has_magnitude {
            for (i, echo) in ctx.run.echoes.iter().enumerate() {
                if let Some(ref mag_path) = echo.magnitude_nifti {
                    let out_path = ctx.output.mag_path(&ctx.run.key, i + 1);
                    if let Some(parent) = out_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::copy(mag_path, &out_path)?;
                    mag_paths.push(out_path);
                }
            }
        }
    }

    let mut all_paths = phase_paths;
    all_paths.extend(mag_paths.clone());
    let input_paths: Vec<PathBuf> = ctx.run.echoes.iter()
        .flat_map(|e| std::iter::once(e.phase_nifti.clone()).chain(e.imaginary_nifti.clone()))
        .collect();
    let input_refs: Vec<&Path> = input_paths.iter().map(|p| p.as_path()).collect();
    let outcome = converted.as_ref().map(|c| ("real_imaginary_conversion", slice_chopping_json(&c.chopping)));
    ctx.complete_step_with_outcome("scale_phase", None, params, outcome, &input_refs, all_paths, t)?;
    log_step_done(if ctx.meta.source_geometry.is_some() { "Resample + rescale phase" } else { "Rescale phase" }, t);
    Ok(())
}

/// MCPC-3D-S coil combination for uncombined (per-coil) runs.
///
/// Replaces `scale_phase` for runs with `coils`: every coil's echoes are loaded, phase rescaled
/// to radians, combined with `qsm_core::utils::mcpc3ds_combine`, and the combined per-echo
/// phase (already in radians, coil offsets removed) and magnitude are written to the same
/// `scale_phase` intermediates every later stage reads. The combined echoes are also exported
/// to the derivatives `anat/` folder as `rec-mcpc3ds` MEGRE volumes for inspection/reuse.
fn stage_combine_coils(ctx: &mut StageContext, progress: &dyn Fn(&str)) -> crate::Result<()> {
    let coils = ctx.run.coils.as_ref().expect("stage_combine_coils needs coils");
    let n_coils = coils.len();
    let n_echoes = ctx.meta.n_echoes;
    // MCPC-3D-S estimates each coil's phase offset from the difference of two echoes; qsm-core
    // asserts on one. `qsmxt run` refuses such a run up front; this keeps a direct caller from
    // reaching the panic.
    if n_echoes < 2 {
        return Err(QsmxtError::Config(format!(
            "{}: MCPC-3D-S coil combination needs at least 2 echoes, and this run has {} echo \
             with {} uncombined coils. Combine the coils first (on the scanner, or with another \
             tool).", ctx.run.key, n_echoes, n_coils)));
    }
    let sigma = ctx.config.field_mapping.coil_combination_sigma;
    let unwrap = format!("{}", ctx.config.field_mapping.unwrapping_algorithm);
    let params = serde_json::json!({
        "coil_combination": "mcpc3ds",
        "n_coils": n_coils,
        "n_echoes": n_echoes,
        "coil_numbers": coils.iter().map(|c| c.coil_number).collect::<Vec<_>>(),
        "hip_echoes": [1, 2],
        "sigma": sigma,
        "hip_unwrapping": unwrap,
        "echo_times": ctx.meta.echo_times,
        "resampled_to_axial": ctx.meta.source_geometry.is_some(),
    });
    if ctx.is_cached_with_params("scale_phase", Some("mcpc3ds"), &params) {
        log::info!("Skipping coil combination (cached)");
        return Ok(());
    }
    let t = Instant::now();
    log::info!("Coil combination (MCPC-3D-S): {} coils x {} echoes", n_coils, n_echoes);
    progress("MCPC-3D-S coil combination");

    let (src_dims, src_voxel) = match ctx.meta.source_geometry {
        Some((d, a)) => (d, qsm_core::geometry::voxel_sizes_from_affine(&a)),
        None => (ctx.meta.dims, ctx.meta.voxel_size),
    };
    let n = src_dims.0 * src_dims.1 * src_dims.2;
    let mut phases: Vec<Vec<Vec<f64>>> = Vec::with_capacity(n_coils);
    let mut mags: Vec<Vec<Vec<f64>>> = Vec::with_capacity(n_coils);
    let mut inputs: Vec<PathBuf> = Vec::new();
    for coil in coils {
        let mut cp = Vec::with_capacity(n_echoes);
        let mut cm = Vec::with_capacity(n_echoes);
        for echo in coil.echoes.iter().take(n_echoes) {
            let mut p = load_volume(&echo.phase_nifti)?;
            if p.len() != n {
                return Err(QsmxtError::DimensionMismatch(format!(
                    "{} has {} voxels, expected {}", echo.phase_nifti.display(), p.len(), n)));
            }
            qsm_core::pipeline::scale_phase_to_pi(&mut p);
            let mag_path = echo.magnitude_nifti.as_ref().ok_or_else(|| QsmxtError::Config(format!(
                "coil {} echo {}: magnitude required for MCPC-3D-S", coil.coil_number, echo.echo_number)))?;
            let m = load_volume(mag_path)?;
            if m.len() != n {
                return Err(QsmxtError::DimensionMismatch(format!(
                    "{} has {} voxels, expected {}", mag_path.display(), m.len(), n)));
            }
            inputs.push(echo.phase_nifti.clone());
            inputs.push(mag_path.clone());
            cp.push(p);
            cm.push(m);
        }
        phases.push(cp);
        mags.push(cm);
    }

    let grid = qsm_core::Grid::new(
        src_dims.0, src_dims.1, src_dims.2, src_voxel.0, src_voxel.1, src_voxel.2,
    );
    let unwrap_method = if unwrap == "laplacian" {
        qsm_core::unwrap::UnwrapMethod::Laplacian
    } else {
        qsm_core::unwrap::UnwrapMethod::Romeo
    };
    let combined = qsm_core::utils::mcpc3ds_combine(
        &phases, &mags, &ctx.meta.echo_times, sigma, [0, 1], unwrap_method, &grid,
    )?;
    drop(phases);
    drop(mags);

    let mut outputs = Vec::new();
    for e in 0..n_echoes {
        // Combine on the acquired grid (all coils share it), then resample the single combined
        // pair rather than 32 of them.
        let (phase_e, mag_e) = match ctx.meta.source_geometry {
            Some((sd, sa)) => {
                let r = qsm_core::geometry::resample_complex_to_axial(
                    &combined.magnitudes[e], &combined.phases[e], sd.0, sd.1, sd.2, &sa,
                    &qsm_core::geometry::AxialResampleParams::default(),
                );
                (r.phase, r.magnitude)
            }
            None => (combined.phases[e].clone(), combined.magnitudes[e].clone()),
        };
        let p_path = ctx.output.phase_scaled_path(&ctx.run.key, e + 1);
        save_volume(&p_path, &phase_e, ctx.meta)?;
        let m_path = ctx.output.mag_path(&ctx.run.key, e + 1);
        save_volume(&m_path, &mag_e, ctx.meta)?;
        // Exported combined echoes (BIDS-style, rec-mcpc3ds) next to the other derivatives.
        let p_out = ctx.output.combined_phase_path(&ctx.run.key, e + 1);
        save_volume(&p_out, &phase_e, ctx.meta)?;
        let m_out = ctx.output.combined_mag_path(&ctx.run.key, e + 1);
        save_volume(&m_out, &mag_e, ctx.meta)?;
        outputs.extend([p_path, m_path, p_out, m_out]);
    }
    let mask_out = ctx.output.combine_mask_path(&ctx.run.key);
    let combine_mask = match ctx.meta.source_geometry {
        Some((sd, sa)) => qsm_core::geometry::resample_mask_to_axial(&combined.mask, sd.0, sd.1, sd.2, &sa),
        None => combined.mask.clone(),
    };
    save_mask(&mask_out, &combine_mask, ctx.meta)?;
    outputs.push(mask_out);

    let input_refs: Vec<&Path> = inputs.iter().map(|p| p.as_path()).collect();
    ctx.complete_step("scale_phase", Some("mcpc3ds"), params, &input_refs, outputs, t)?;
    log_step_done("Coil combination", t);
    Ok(())
}

/// Apply inhomogeneity (bias field) correction on the run's working grid.
///
/// qsm-core indexes the volume as `x + y*nx + z*nx*ny` over the grid dimensions, so a volume
/// with fewer voxels than the grid panics with an out-of-bounds index inside the box filter
/// rather than reporting which input is wrong (issue #184). `validate_run_dims` catches this at
/// load time for a fresh run; this guard also covers runs resumed from a cached state.
fn homogeneity_correct(
    data: &[f64],
    meta: &RunMetadata,
    homogeneity: &HomogeneityConfig,
    source: &str,
) -> crate::Result<Vec<f64>> {
    let (nx, ny, nz) = meta.dims;
    let (vsx, vsy, vsz) = meta.voxel_size;
    if data.len() != nx * ny * nz {
        return Err(QsmxtError::DimensionMismatch(format!(
            concat!(
                "inhomogeneity correction: {} has {} voxels but this run is processed on a ",
                "{}x{}x{} grid ({} voxels) taken from its first phase echo. Check that the ",
                "magnitude and phase come from the same acquisition, or pass ",
                "--no-inhomogeneity-correction to skip this step",
            ),
            source, data.len(), nx, ny, nz, nx * ny * nz,
        )));
    }
    let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
    Ok(qsm_core::utils::makehomogeneous(data, &grid, homogeneity.sigma_mm, homogeneity.nbox))
}

fn stage_magnitude(ctx: &mut StageContext, progress: &dyn Fn(&str)) -> crate::Result<()> {
    let mag_params = serde_json::json!({
        "inhomogeneity_correction": ctx.config.masking.inhomogeneity_correction,
        "homogeneity_sigma_mm": ctx.config.homogeneity.sigma_mm,
        "homogeneity_nbox": ctx.config.homogeneity.nbox,
    });
    if ctx.is_cached_with_params("magnitude", Some("rss"), &mag_params) {
        log::info!("Skipping magnitude (cached)");
        return Ok(());
    }
    if !ctx.run.has_magnitude {
        return Ok(());
    }
    let t = Instant::now();
    progress("Computing RSS-combined magnitude");

    // Load raw per-echo magnitudes
    let mut mag_slices: Vec<Vec<f64>> = Vec::new();
    for i in 0..ctx.meta.n_echoes {
        let m_path = ctx.output.mag_path(&ctx.run.key, i + 1);
        if m_path.exists() {
            mag_slices.push(load_volume(&m_path)?);
        } else if let Some(ref src) = ctx.run.echoes[i].magnitude_nifti {
            let nifti = io::read_nifti_file(src)
                .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", src.display(), e)))?;
            mag_slices.push(nifti.data);
        }
    }

    if !mag_slices.is_empty() {
        let refs: Vec<&[f64]> = mag_slices.iter().map(|v| v.as_slice()).collect();
        let mut combined = phase::rss_combine(&refs)?;

        // Apply homogeneity correction to the combined result
        if ctx.config.masking.inhomogeneity_correction {
            progress("Applying inhomogeneity correction");
            combined = homogeneity_correct(
                &combined, ctx.meta, &ctx.config.homogeneity,
                "the RSS-combined magnitude",
            )?;
        }

        let combined_path = ctx.output.magnitude_path(&ctx.run.key);
        save_volume(&combined_path, &combined, ctx.meta)?;
        let mag_inputs: Vec<PathBuf> = (0..ctx.meta.n_echoes).map(|i| ctx.output.mag_path(&ctx.run.key, i + 1)).collect();
        let input_refs: Vec<&Path> = mag_inputs.iter().map(|p| p.as_path()).collect();
        ctx.complete_step("magnitude", Some("rss"), mag_params, &input_refs, vec![combined_path], t)?;
    }
    log_step_done("RSS magnitude", t);
    Ok(())
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
fn find_custom_mask(run: &QsmRun, tool: &str) -> Option<PathBuf> {
    find_custom_derivative(run, tool, "*_mask.nii*", &["_desc-reliable_", "_desc-qsm_", "_desc-r2prime_"])
}

fn stage_mask(ctx: &mut StageContext, mask_path: &Path, progress: &dyn Fn(&str)) -> crate::Result<()> {
    let mask_params = serde_json::json!({
        "sections": ctx.config.masking.sections.iter().map(|s| format!("{}", s)).collect::<Vec<_>>(),
        "combine": format!("{}", ctx.config.masking.combine),
        "refinements": ctx.config.masking.refinements.iter().map(|o| format!("{}", o)).collect::<Vec<_>>(),
        "custom_mask_tool": ctx.config.masking.custom_mask_tool,
    });
    if ctx.is_cached_with_params("mask", None, &mask_params) {
        log::info!("Skipping mask (cached)");
        return Ok(());
    }
    let t = Instant::now();
    progress("Creating mask");

    // Prefer a bring-your-own mask from BIDS derivatives, if requested and available.
    if let Some(tool) = ctx.config.masking.custom_mask_tool.clone() {
        if let Some(cm_path) = find_custom_mask(ctx.run, &tool) {
            match io::read_nifti_file(&cm_path) {
                Ok(nd) => {
                    let raw: Vec<u8> = nd.data.iter().map(|&v| if v > 0.5 { 1u8 } else { 0u8 }).collect();
                    let mask_u8 = if nd.dims == ctx.meta.dims {
                        raw
                    } else {
                        // Nearest-neighbour resample onto the working (axial) grid.
                        phase::resample_mask_to_axial(&raw, nd.dims.0, nd.dims.1, nd.dims.2, &nd.affine)
                    };
                    let (nx, ny, nz) = ctx.meta.dims;
                    if mask_u8.len() == nx * ny * nz {
                        log::info!("Using custom mask from {}", cm_path.display());
                        save_mask(mask_path, &mask_u8, ctx.meta)?;
                        ctx.complete_step("mask", None, mask_params, &[cm_path.as_path()],
                                          vec![mask_path.to_path_buf()], t)?;
                        log_step_done("Mask (custom)", t);
                        return Ok(());
                    }
                    log::warn!("custom mask {} could not be conformed to the working grid — computing instead",
                               cm_path.display());
                }
                Err(e) => log::warn!("could not read custom mask {} ({}) — computing instead", cm_path.display(), e),
            }
        } else {
            let ses_str = ctx.run.key.session.as_ref().map(|s| format!("/ses-{}", s)).unwrap_or_default();
            log::info!("no custom mask under derivatives (tool: {}) for sub-{}{} — computing",
                       tool, ctx.run.key.subject, ses_str);
        }
    }

    log::info!("Creating mask ({} section(s), combined with {})",
               ctx.config.masking.sections.len(), ctx.config.masking.combine);

    let working_mask = build_mask_from_sections(ctx, &ctx.config.masking.sections.clone())?;
    save_mask(mask_path, &working_mask, ctx.meta)?;
    let mag_paths: Vec<PathBuf> = (1..=ctx.meta.n_echoes).map(|e| ctx.output.mag_path(&ctx.run.key, e))
        .filter(|p| p.exists()).collect();
    let mag_refs: Vec<&Path> = mag_paths.iter().map(|p| p.as_path()).collect();
    ctx.complete_step("mask", None, mask_params, &mag_refs, vec![mask_path.to_path_buf()], t)?;
    log_step_done("Mask creation", t);
    Ok(())
}

/// Build a mask from a list of sections, folded with the configured combine mode and post-combine
/// refinements. Shared by the main mask and the two-pass reliable mask, so both read the same
/// images and honour the same folding rules.
fn build_mask_from_sections(
    ctx: &mut StageContext, sections: &[MaskSection],
) -> crate::Result<Vec<u8>> {
    // Load phases (needed for PhaseQuality masking input)
    let mut phases: Vec<Vec<f64>> = Vec::new();
    for i in 0..ctx.meta.n_echoes {
        let p_path = ctx.output.phase_scaled_path(&ctx.run.key, i + 1);
        if p_path.exists() {
            phases.push(load_volume(&p_path)?);
        }
    }

    // Every echo's magnitude: each section builds the magnitude it asks for (echo 1, the RSS, the
    // last echo) from these in qsm-core, so all sections see the same, consistently corrected,
    // echoes (QSM.rs#173).
    let magnitudes = mask_echo_magnitudes(ctx)?;

    // Convert config masking sections to qsm-core types
    let core_sections = crate::pipeline::config::to_mask_sections(sections, &ctx.config.bet);
    let scan_meta = scan_metadata(ctx.meta, ctx.meta.dims);
    let phase_refs: Vec<&[f64]> = phases.iter().map(|p| p.as_slice()).collect();

    // Deep-learning mask ops (HD-BET): fetch the weights first, with a progress bar — or fail
    // clearly on a build without the `dl` feature.
    for id in core_sections.iter().flat_map(|s| s.all_ops()).filter_map(|op| op.dl_model_id()) {
        prefetch_weights(id, &ctx.run.key.to_string())?;
    }

    let core_refinements = qsmxt_config::to_mask_ops(&ctx.config.masking.refinements, &ctx.config.bet);
    let mag_refs: Vec<&[f64]> = magnitudes.iter().map(|m| m.as_slice()).collect();
    combine_mask_sections(
        &core_sections, ctx.config.masking.combine, &core_refinements,
        &phase_refs, &mag_refs, &scan_meta,
    ).map_err(|e| QsmxtError::Config(format!("masking: {}", e)))
}

/// Warn when the reliable mask leaves two-pass with nothing useful to do.
///
/// The holes are what the method works with, so a reliable mask that has (almost) none is a
/// second reconstruction spent reproducing the first, and one with no voxels at all produces a
/// combined map identical to the single-pass one. Neither is an error — the user may be
/// exploring — but neither is worth the run time in silence. The 99.9% tolerance is there because
/// a slightly-too-loose threshold leaves a handful of voxels rather than exactly zero.
fn two_pass_coverage_advice(kept: usize, brain: usize) -> Option<&'static str> {
    if brain == 0 {
        return None;
    }
    if kept == 0 {
        return Some("The reliable-phase mask is empty, so the two-pass output will be identical \
                     to the single-pass one. Loosen --two-pass-mask, or turn two-pass off.");
    }
    if kept as f64 >= 0.999 * brain as f64 {
        return Some("The reliable-phase mask covers essentially the whole brain mask, so \
                     two-pass has almost nothing to reduce — it will spend a second \
                     reconstruction reproducing the first. Tighten --two-pass-mask (and drop any \
                     hole-filling from it), or turn two-pass off.");
    }
    None
}

/// Build the reliable-phase mask of a two-pass run, restricted to the main mask.
///
/// The restriction is not optional: the reliable mask decides where its pass's values are
/// preferred, so a section reaching past the brain mask would promote a reconstruction over a
/// region the run excluded outright.
fn stage_two_pass_mask(
    ctx: &mut StageContext, main_mask: &Path, out_path: &Path, progress: &dyn Fn(&str),
) -> crate::Result<()> {
    let sections = ctx.config.masking.resolved_two_pass_sections();
    let params = serde_json::json!({
        "sections": sections.iter().map(|s| format!("{}", s)).collect::<Vec<_>>(),
        "combine": format!("{}", ctx.config.masking.combine),
        "refinements": ctx.config.masking.refinements.iter().map(|o| format!("{}", o)).collect::<Vec<_>>(),
    });
    let step = format!("mask{}", crate::pipeline::graph::RELIABLE_SUFFIX);
    if ctx.is_cached_with_params(&step, None, &params) {
        log::info!("Skipping reliable-phase mask (cached)");
        return Ok(());
    }
    let t = Instant::now();
    progress("Creating the reliable-phase mask");
    log::info!("Creating the reliable-phase mask ({} section(s))", sections.len());

    let reliable = build_mask_from_sections(ctx, &sections)?;
    let main = load_mask(main_mask)?;
    let restricted = qsm_core::pipeline::restrict_reliable_mask(&reliable, &main)
        .map_err(|e| QsmxtError::Config(format!("reliable mask: {}", e)))?;

    let (kept, brain) = (
        restricted.iter().filter(|&&v| v != 0).count(),
        main.iter().filter(|&&v| v != 0).count(),
    );
    log::info!("Reliable-phase mask: {kept} of {brain} brain voxels ({:.1}%)",
               100.0 * kept as f64 / brain.max(1) as f64);
    if let Some(advice) = two_pass_coverage_advice(kept, brain) {
        log::warn!("{advice}");
    }

    save_mask(out_path, &restricted, ctx.meta)?;
    ctx.complete_step(&step, None, params, &[main_mask], vec![out_path.to_path_buf()], t)?;
    log_step_done("Reliable-phase mask", t);
    Ok(())
}

/// Build each mask section, fold them together with `combine`, then apply `refinements` to the
/// result.
///
/// `combine` only matters with more than one section: `or` keeps a voxel any section keeps
/// (the union qsm-core's `run_masking` produces), `and` keeps only voxels every section keeps.
/// The refinements run on the combined mask, which is where an intersection's holes get filled —
/// they are checked to be non-generators before we get here, so the image they see does not
/// matter; the magnitude is passed for `signal-erode`.
fn combine_mask_sections(
    sections: &[qsm_core::pipeline::config::MaskSection],
    combine: MaskCombine,
    refinements: &[qsm_core::pipeline::config::MaskOp],
    phases: &[&[f64]],
    magnitudes: &[&[f64]],
    meta: &qsm_core::pipeline::config::ScanMetadata,
) -> std::result::Result<Vec<u8>, qsm_core::pipeline::PipelineError> {
    use qsm_core::pipeline::PipelineError;

    let mut combined: Option<Vec<u8>> = None;
    for section in sections {
        let mask = qsm_core::pipeline::build_mask_section(section, phases, magnitudes, meta)?;
        combined = Some(match combined {
            None => mask,
            Some(mut acc) => { combine.accumulate(&mut acc, &mask); acc }
        });
    }
    let combined = combined.ok_or_else(|| PipelineError::InvalidConfig("no mask sections configured".into()))?;

    if refinements.is_empty() {
        return Ok(combined);
    }
    // The refinements run on the combined mask, so they see the RSS magnitude (what
    // `signal-erode` gates on); they are not generators, so the input image does not matter.
    let magnitude = qsm_core::pipeline::section_magnitude(
        qsm_core::pipeline::config::MaskingInput::Magnitude, magnitudes, meta)?;
    qsm_core::pipeline::apply_mask_ops(
        combined, refinements, magnitude.as_deref().unwrap_or(&[]), magnitude.as_deref(), meta)
}

/// Every echo's magnitude on the working grid, for masking, homogeneity-corrected when
/// `--inhomogeneity-correction` is on. Empty for a phase-only run.
///
/// Each echo is corrected on its own, so a section on echo 1, on the RSS or on the last echo
/// sees the same correction; the masks used to mix a corrected RSS with an uncorrected echo
/// depending on the section.
fn mask_echo_magnitudes(ctx: &StageContext) -> crate::Result<Vec<Vec<f64>>> {
    let mut out = Vec::with_capacity(ctx.meta.n_echoes);
    for e in 1..=ctx.meta.n_echoes {
        let p = ctx.output.mag_path(&ctx.run.key, e);
        if !p.exists() {
            return Ok(Vec::new());
        }
        let data = load_volume(&p)?;
        out.push(if ctx.config.masking.inhomogeneity_correction {
            homogeneity_correct(&data, ctx.meta, &ctx.config.homogeneity,
                                &format!("echo {e}'s magnitude"))?
        } else {
            data
        });
    }
    Ok(out)
}

fn stage_swi(ctx: &mut StageContext, mask_path: &Path, progress: &dyn Fn(&str)) -> crate::Result<()> {
    let (nx, ny, nz) = ctx.dims();
    let (vsx, vsy, vsz) = ctx.voxel_size();
    // Checked up front so an impossible window fails before the SWI is computed, not after.
    let window = ctx.config.swi.mip_window;
    let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
    let (mip_grid, _) = qsm_core::swi::mip_geometry(&grid, &ctx.meta.affine, window)
        .map_err(QsmxtError::Config)?;
    let swi_params = serde_json::json!({
        "scaling": ctx.config.swi.scaling,
        "strength": ctx.config.swi.strength,
        "hp_sigma": ctx.config.swi.hp_sigma,
        "mip_window": window,
        // Part of the cache key so a minIP written with full-volume dimensions before this was
        // fixed (issue #211) is rebuilt rather than kept.
        "mip_dims": [mip_grid.nx(), mip_grid.ny(), mip_grid.nz()],
    });
    if ctx.is_cached_with_params("swi", Some("clear-swi"), &swi_params) {
        log::info!("Skipping swi (cached)");
        return Ok(());
    }
    let t = Instant::now();
    log::info!("Computing SWI (echo 1: Laplacian unwrap, high-pass phase mask, minIP)");
    progress("Computing SWI");
    let phase_data = load_volume(&ctx.echo_phase_path(1))?;
    let mag_data = load_volume(&ctx.output.magnitude_path(&ctx.run.key))?;
    let mask = load_mask(mask_path)?;

    let unwrapped = qsm_core::unwrap::laplacian_unwrap(&phase_data, &mask, &grid, qsm_core::unwrap::LaplacianSolver::Dct);
    let swi_params_core = qsm_core::swi::SwiParams {
        hp_sigma: ctx.config.swi.hp_sigma, scaling: ctx.config.swi.scaling.into(),
        strength: ctx.config.swi.strength, mip_window: window,
    };
    let swi = qsm_core::swi::calculate_swi(
        &unwrapped, &mag_data, &mask, &grid, &swi_params_core,
    );
    // The projection carries its own geometry: shorter than the SWI along the slice axis, and
    // centred on each slab. Writing it under the run's dimensions is issue #211.
    let mip = qsm_core::swi::create_mip(&swi, &grid, &ctx.meta.affine, window)
        .map_err(QsmxtError::Config)?;

    let swi_path = ctx.output.swi_path(&ctx.run.key);
    let mip_path = ctx.output.swi_mip_path(&ctx.run.key);
    save_volume(&swi_path, &swi, ctx.meta)?;
    write_volume(&mip_path, &mip.data, mip.grid.dims, mip.grid.voxel_size, &mip.affine)?;
    let phase_path = ctx.echo_phase_path(1);
    let mag_input = ctx.output.magnitude_path(&ctx.run.key);
    ctx.complete_step("swi", Some("clear-swi"), swi_params, &[phase_path.as_path(), mag_input.as_path(), mask_path], vec![swi_path, mip_path], t)?;
    log_step_done("SWI", t);
    Ok(())
}

/// The pipeline's own output for an input, or the bring-your-own derivative standing in for it.
///
/// A custom tool means the stage that would have written `path` never ran, so the file has to be
/// found under `<bids>/derivatives/<tool>/` instead. The pipeline's own output still wins when it
/// exists: a run that computed the map should be summarised on what it computed.
fn resolve_input(
    ctx: &StageContext, path: &Path, custom_tool: Option<&str>, glob: &str, exclude: &[&str],
) -> Option<PathBuf> {
    if path.exists() {
        return Some(path.to_path_buf());
    }
    let found = find_custom_derivative(ctx.run, custom_tool?, glob, exclude)?;
    log::info!("Summarising the supplied {}", found.display());
    Some(found)
}

/// Per-structure statistics over a segmentation.
///
/// One row per (map, structure), covering every quantitative map the run produced: χ, the
/// chi-separation maps, T2*, R2*, R2 and R2'. The maps are found on disk rather than inferred from
/// the config, so a bring-your-own derivative is summarised the same as one this run computed, and
/// a map that was enabled but could not be made (no MESE for R2, say) simply contributes no rows.
///
/// Both a median and a mean are reported because they answer different questions: the
/// distributions inside a structure are skewed and carry outliers (vessels, partial volume at a
/// boundary), so the median is the robust summary while the mean is what most of the literature
/// quotes. Min/max and the 5th/95th percentiles bracket the spread from both ends — the
/// percentiles without a single voxel setting them, the extremes saying how far that voxel went.
fn stage_analysis(ctx: &mut StageContext, dseg_path: &Path, progress: &dyn Fn(&str)) -> crate::Result<()> {
    let out_path = ctx.output.segmentation_stats_path(&ctx.run.key);

    // A supplied dseg or Chimap is read where it lies, as chi-separation reads its custom inputs:
    // `--use-custom-dseg`/`--use-custom-qsm` mean the run never computed one, so nothing was
    // written to the pipeline's own path for it.
    let dseg = resolve_input(
        ctx, dseg_path, ctx.config.segmentation.custom_dseg_tool.as_deref(), "*_dseg.nii*", &[]);
    let Some(dseg) = dseg else {
        log::warn!("Skipping analysis: no segmentation at {}", dseg_path.display());
        return Ok(());
    };
    let qsm = resolve_input(
        ctx, &ctx.output.qsm_path(&ctx.run.key),
        ctx.config.separation.custom_qsm_tool.as_deref(), "*_Chimap.nii*", &["desc-"]);

    let mut maps: Vec<stats::MapSpec> = stats::candidate_maps(ctx.output, &ctx.run.key)
        .into_iter()
        .filter(|m| m.path.exists())
        .collect();
    if let Some(qsm) = qsm {
        if !maps.iter().any(|m| m.name == stats::CHIMAP) {
            maps.insert(0, stats::MapSpec { name: stats::CHIMAP, unit: "ppm", path: qsm });
        }
    }

    // The cache key carries which maps were summarised: computing R2* on a re-run has to add its
    // rows rather than leave the table as it was.
    let params = serde_json::json!({
        "version": format!("{}", ctx.config.segmentation.version),
        "dseg": dseg.display().to_string(),
        "maps": maps.iter().map(|m| (m.name, m.path.display().to_string())).collect::<Vec<_>>(),
    });
    if ctx.is_cached_with_params("analysis", None, &params) {
        log::info!("Skipping analysis (cached)");
        return Ok(());
    }

    if maps.is_empty() {
        log::warn!("Skipping analysis: the run produced no quantitative map to summarise");
        return Ok(());
    }
    let t = Instant::now();
    progress("Summarising maps per structure");

    let seg = load_volume(&dseg)?;
    let labels = to_core_synthseg_version(ctx.config.segmentation.version).labels();
    // SynthSeg's per-label volumes, when this run produced them itself.
    let volumes: Option<Vec<f64>> = ctx.state.completed_steps.get("segmentation")
        .and_then(|r| r.metadata.as_ref())
        .and_then(|m| m.get("volumes"))
        .and_then(|v| serde_json::from_value(v.clone()).ok());

    let mut table = String::from(stats::HEADER);
    let mut summarised: Vec<&str> = Vec::new();
    for spec in &maps {
        let map = load_volume(&spec.path)?;
        if map.len() != seg.len() {
            return Err(QsmxtError::DimensionMismatch(format!(
                "segmentation has {} voxels but {} has {}",
                seg.len(), spec.path.display(), map.len())));
        }
        // What this map was referenced to, read back from its own sidecar, where the referencing
        // stage wrote it — so the table says what its zero means. A map that was never
        // referenced (the χ-separation maps) has no such record, and its columns stay empty.
        let reference = crate::bids::entities::sidecar_path(&spec.path)
            .as_deref()
            .and_then(stats::ReferenceInfo::from_sidecar);
        table.push_str(&stats::map_rows(spec, &map, &seg, labels, volumes.as_deref(),
                                        reference.as_ref()));
        summarised.push(spec.name);
    }

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&out_path, table.clone())?;
    log::info!("Per-structure statistics for {} -> {}", summarised.join(", "), out_path.display());

    // One figure per map, beside the table. Best-effort: the numbers are the deliverable, and a
    // figure that fails to draw must not cost the run the statistics it already computed.
    let mut outputs = vec![out_path.clone()];
    let anat = ctx.output.anat_dir(&ctx.run.key);
    match crate::pipeline::figure::render_all(
        &stats::parse_tsv(&table),
        &ctx.run.key.to_string(),
        "Mean per structure; whiskers ±1 SD over the structure's voxels",
        "Volume from the SynthSeg posteriors, partial volume included",
    ) {
        Ok(figs) => for f in figs {
            match f.save_in(&anat, &ctx.run.key.basename()) {
                Ok(p) => { log::info!("Figure -> {}", p.display()); outputs.push(p); }
                Err(e) => log::warn!("Could not write {}: {e}", f.stem),
            }
        },
        Err(e) => log::warn!("Could not draw the per-structure figures: {e}"),
    }
    let inputs: Vec<&Path> = std::iter::once(dseg.as_path())
        .chain(maps.iter().map(|m| m.path.as_path()))
        .collect();
    ctx.complete_step("analysis", None, params, &inputs, outputs, t)?;
    log_step_done("Per-structure analysis", t);
    Ok(())
}

/// R2PRIMEnet inference. Gated in one place so a non-`dl` build says why it cannot run rather
/// than failing to link.
///
/// The network's weights are trained at 3 T and it takes no field strength as an input, so
/// `qsm_core` refuses anything far from 3 T unless `separation.r2primenet.ignore_field_mismatch`
/// is set. Whether to get this far at all off 3 T is [`r2primenet_field_policy`]'s decision.
#[cfg(feature = "dl")]
fn run_r2primenet(
    ctx: &StageContext, r2star: &[f64], mask: &[u8], grid: &qsm_core::Grid,
) -> crate::Result<Vec<f64>> {
    let params = r2primenet_params(&ctx.config.separation.r2primenet)?;
    let weights = qsm_core::models::primary_weight("r2primenet")
        .map_err(|e| QsmxtError::Config(format!("r2primenet weights: {}", e)))?;
    let (prog, _) = iter_progress_bar(&ctx.run.key.to_string(), "r2primenet");
    qsm_core::relaxometry::r2primenet(
        r2star, mask, grid, ctx.meta.field_strength, &weights,
        &qsm_core::relaxometry::R2PrimeNetNorm::default(),
        &params,
        prog,
    )
    .map_err(|e| QsmxtError::Config(format!("r2primenet: {}", e)))
}

/// qsm-core's R2PRIMEnet parameters, or why the configured patch cannot be used.
#[cfg(feature = "dl")]
fn r2primenet_params(
    c: &crate::pipeline::config::R2primenetConfig,
) -> crate::Result<qsm_core::relaxometry::R2PrimeNetParams> {
    c.validate().map_err(QsmxtError::Config)?;
    Ok(qsm_core::relaxometry::R2PrimeNetParams {
        patch: (c.patch[0], c.patch[1], c.patch[2]),
        ignore_field_mismatch: c.ignore_field_mismatch,
    })
}

/// What to do about R2PRIMEnet's field strength before running it.
#[derive(Debug, PartialEq)]
enum R2primenetFieldPolicy {
    /// At 3 T: run.
    Run,
    /// Off 3 T but explicitly allowed: run, and warn that the R2' is uncalibrated.
    RunUncalibrated,
    /// Off 3 T under `--r2prime-strategy auto`: skip R2' with a warning rather than fail the run
    /// over a stage the user may not have asked for by name.
    Skip,
    /// Off 3 T and R2PRIMEnet asked for by name: refuse.
    Refuse,
}

fn r2primenet_field_policy(
    field_strength: f64, strategy: R2PrimeStrategy, ignore_field_mismatch: bool,
) -> R2primenetFieldPolicy {
    match (r2primenet_field_offset(field_strength).is_some(), ignore_field_mismatch, strategy) {
        (false, _, _) => R2primenetFieldPolicy::Run,
        (true, true, _) => R2primenetFieldPolicy::RunUncalibrated,
        (true, false, R2PrimeStrategy::Auto) => R2primenetFieldPolicy::Skip,
        (true, false, _) => R2primenetFieldPolicy::Refuse,
    }
}

/// How far this acquisition sits outside the band R2PRIMEnet's weights were trained over, in
/// Tesla, or `None` when it is inside it.
///
/// Kept separate from the inference so a non-`dl` build still compiles the check, and so the
/// policy is decided before the weights are fetched rather than after.
fn r2primenet_field_offset(field_strength: f64) -> Option<f64> {
    let off = (field_strength - R2PRIMENET_TRAINED_B0_T).abs();
    (off > R2PRIMENET_B0_TOLERANCE_T).then_some(off)
}

/// qsm-core's `relaxometry::{TRAINED_B0_T, B0_TOLERANCE_T}`, which only exist with its `onnx`
/// feature; a non-`dl` build still needs them for the policy. Checked against qsm-core in tests.
const R2PRIMENET_TRAINED_B0_T: f64 = 3.0;
const R2PRIMENET_B0_TOLERANCE_T: f64 = 0.3;

/// Why R2PRIMEnet off 3 T is a problem, for the warning or the error.
///
/// R2* scales with field strength and the network has no way to be told what it is looking at,
/// so a 7 T R2* produces an R2' map that is the right shape, the right units and the wrong
/// numbers. Nothing downstream can detect that, which is the reason to say it here.
/// What is wrong with running R2PRIMEnet at this field strength under this strategy, when the
/// policy would skip or refuse it; `None` when it may run.
pub(crate) fn r2primenet_off_field(
    field_strength: f64, strategy: R2PrimeStrategy, ignore_field_mismatch: bool,
) -> Option<String> {
    match r2primenet_field_policy(field_strength, strategy, ignore_field_mismatch) {
        R2primenetFieldPolicy::Skip | R2primenetFieldPolicy::Refuse =>
            Some(r2primenet_field_message(field_strength)),
        R2primenetFieldPolicy::Run | R2primenetFieldPolicy::RunUncalibrated => None,
    }
}

fn r2primenet_field_message(field_strength: f64) -> String {
    format!(
        "R2PRIMEnet's weights are trained at {} T and this acquisition is {} T. R2* scales with \
         field strength, so a predicted R2' would be outside what the network has seen and wrong \
         in a way that looks plausible. Acquire a MESE and use --r2prime-strategy mese to measure \
         R2' instead, or supply one with --use-custom-r2prime",
        R2PRIMENET_TRAINED_B0_T, field_strength,
    )
}

#[cfg(not(feature = "dl"))]
fn run_r2primenet(
    _ctx: &StageContext, _r2star: &[f64], _mask: &[u8], _grid: &qsm_core::Grid,
) -> crate::Result<Vec<f64>> {
    Err(QsmxtError::Config(
        "estimating R2' needs a deep-learning build: R2PRIMEnet runs an ONNX network, which this \
         binary was compiled without (--no-default-features). Supply a custom R2' map, or a MESE \
         acquisition to measure it from".into()))
}

/// Bring-your-own segmentation (`*_dseg.nii*`).
fn find_custom_dseg(run: &QsmRun, tool: &str) -> Option<PathBuf> {
    find_custom_derivative(run, tool, "*_dseg.nii*", &[])
}

/// Whole-brain parcellation of the GRE magnitude via SynthSeg.
///
/// SynthSeg is trained on synthetic images with randomised contrast, which is what lets one set of
/// weights segment a GRE magnitude — so susceptibility can be reported per structure without
/// acquiring and registering a separate T1w. It takes no brain mask: it finds the brain itself.
///
/// Writes the label volume and the BIDS lookup table that gives its integers names.
fn stage_segmentation(ctx: &mut StageContext, dseg_path: &Path, progress: &dyn Fn(&str)) -> crate::Result<()> {
    let cfg = &ctx.config.segmentation;
    let params_json = serde_json::json!({
        "version": format!("{}", cfg.version),
        "crop": cfg.crop,
        "flip_averaging": cfg.flip_averaging,
        "topology_cleanup": cfg.topology_cleanup,
        "sigma_smoothing": cfg.sigma_smoothing,
        "custom_dseg_tool": cfg.custom_dseg_tool,
    });
    if ctx.is_cached_with_params("segmentation", Some("synthseg"), &params_json) {
        log::info!("Skipping segmentation (cached)");
        return Ok(());
    }
    let t = Instant::now();

    let version = to_core_synthseg_version(cfg.version);
    let labels_table = version.labels();

    // Prefer a bring-your-own segmentation, as masking does. Its integers are assumed to be
    // FreeSurfer ids, which is what the lookup table is written from.
    if let Some(tool) = cfg.custom_dseg_tool.clone() {
        if let Some(path) = find_custom_dseg(ctx.run, &tool) {
            match io::read_nifti_file(&path) {
                Ok(nd) if nd.dims == ctx.meta.dims => {
                    log::info!("Using custom segmentation from {}", path.display());
                    progress("Reading the supplied segmentation");
                    save_volume(dseg_path, &nd.data, ctx.meta)?;
                    let lut = ctx.output.dseg_lookup_path(&ctx.run.key);
                    write_dseg_lookup(&lut, labels_table)?;
                    ctx.complete_step("segmentation", Some("synthseg"), params_json,
                                      &[path.as_path()], vec![dseg_path.to_path_buf(), lut], t)?;
                    log_step_done("Segmentation (custom)", t);
                    return Ok(());
                }
                Ok(nd) => log::warn!(
                    "custom segmentation {} is {}x{}x{}, not the run's {}x{}x{} — computing instead",
                    path.display(), nd.dims.0, nd.dims.1, nd.dims.2,
                    ctx.meta.dims.0, ctx.meta.dims.1, ctx.meta.dims.2),
                Err(e) => log::warn!("could not read custom segmentation {} ({}) — computing instead",
                                     path.display(), e),
            }
        } else {
            log::info!("no custom segmentation under derivatives (tool: {}) — computing", tool);
        }
    }

    let mag_path = ctx.output.magnitude_path(&ctx.run.key);
    if !mag_path.exists() {
        log::warn!("Skipping segmentation: no combined magnitude at {}", mag_path.display());
        return Ok(());
    }

    crate::pipeline::config::synthseg_weights_available(cfg.version)?;
    prefetch_weights(cfg.version.weights_id(), &ctx.run.key.to_string())?;
    progress("Segmenting (SynthSeg)");
    log::info!("Segmentation (SynthSeg {}, {} labels{}{})",
               cfg.version, labels_table.ids.len(),
               if cfg.flip_averaging { ", flip-averaged" } else { "" },
               if cfg.topology_cleanup { ", topology cleanup" } else { "" });

    let (result, volumes) = run_synthseg(ctx, &mag_path, version)?;

    // FreeSurfer ids are integers; they travel through the f64 writer exactly (all are < 2^53).
    let as_f64: Vec<f64> = result.iter().map(|&l| l as f64).collect();
    save_volume(dseg_path, &as_f64, ctx.meta)?;
    let lut = ctx.output.dseg_lookup_path(&ctx.run.key);
    write_dseg_lookup(&lut, labels_table)?;

    let present = labels_table.ids.iter().skip(1).zip(volumes.iter().skip(1))
        .filter(|(_, &v)| v > 0.0).count();
    log::info!("Segmented {} of {} structures", present, labels_table.ids.len() - 1);

    ctx.complete_step_with_metadata(
        "segmentation", Some("synthseg"), params_json, &[mag_path.as_path()],
        vec![dseg_path.to_path_buf(), lut],
        Some(serde_json::json!({ "volumes": volumes })), t)?;
    log_step_done("Segmentation", t);
    Ok(())
}

pub(crate) fn to_core_synthseg_version(v: crate::pipeline::config::SynthSegVersion) -> qsm_core::segment::SynthSegVersion {
    match v {
        crate::pipeline::config::SynthSegVersion::V1 => qsm_core::segment::SynthSegVersion::V1,
        crate::pipeline::config::SynthSegVersion::V2 => qsm_core::segment::SynthSegVersion::V2,
    }
}

/// Run the network. Split out so the `dl`-feature gate sits in one place: without it the weights
/// cannot be run at all, and saying so plainly beats a missing-symbol build error.
#[cfg(feature = "dl")]
fn run_synthseg(
    ctx: &StageContext, mag_path: &Path, version: qsm_core::segment::SynthSegVersion,
) -> crate::Result<(Vec<i32>, Vec<f64>)> {
    let mag = load_volume(mag_path)?;
    let (nx, ny, nz) = ctx.meta.dims;
    let (vsx, vsy, vsz) = ctx.meta.voxel_size;
    let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
    let params = qsm_core::segment::SynthSegParams {
        version,
        crop: ctx.config.segmentation.crop,
        flip_averaging: ctx.config.segmentation.flip_averaging,
        topology_cleanup: ctx.config.segmentation.topology_cleanup,
        sigma_smoothing: ctx.config.segmentation.sigma_smoothing,
    };
    let weights = qsm_core::models::primary_weight(ctx.config.segmentation.version.weights_id())
        .map_err(|e| QsmxtError::Config(format!("synthseg weights: {}", e)))?;
    let (prog, _) = iter_progress_bar(&ctx.run.key.to_string(), "synthseg");
    let out = qsm_core::segment::synthseg(&mag, &grid, &ctx.meta.affine, &weights, &params, prog)
        .map_err(|e| QsmxtError::Config(format!("synthseg: {}", e)))?;
    Ok((out.labels, out.volumes))
}

#[cfg(not(feature = "dl"))]
fn run_synthseg(
    _ctx: &StageContext, _mag_path: &Path, _version: qsm_core::segment::SynthSegVersion,
) -> crate::Result<(Vec<i32>, Vec<f64>)> {
    Err(QsmxtError::Config(
        "segmentation needs a deep-learning build: SynthSeg runs an ONNX network, which this \
         binary was compiled without (--no-default-features)".into()))
}

/// The BIDS lookup table pairing each label integer with its name.
fn write_dseg_lookup(path: &Path, labels: &qsm_core::segment::SynthSegLabels) -> crate::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = String::from("index\tname\n");
    for (id, name) in labels.ids.iter().zip(labels.names) {
        out.push_str(&format!("{id}\t{name}\n"));
    }
    std::fs::write(path, out)?;
    Ok(())
}

/// Susceptibility map-weighted imaging: weight the magnitude by a mask built from χ.
///
/// Unlike SWI, the weighting comes from the susceptibility map rather than from high-pass filtered
/// phase, so the contrast sits where the source is instead of spreading with the dipole field. It
/// therefore runs after referencing, on the final `Chimap`.
///
/// Both contrasts are written: the paramagnetic mask suppresses positive χ (iron, haemorrhage), the
/// diamagnetic one negative χ (calcification, myelin). Each gets a minIP, carrying the projection's
/// own shorter geometry as the SWI minIP does.
fn stage_smwi(ctx: &mut StageContext, mask_path: &Path, progress: &dyn Fn(&str)) -> crate::Result<()> {
    let (nx, ny, nz) = ctx.dims();
    let (vsx, vsy, vsz) = ctx.voxel_size();
    let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
    let window = ctx.config.smwi.mip_window;
    // Checked before any work, so an impossible window fails early rather than after the weighting.
    let (mip_grid, _) = qsm_core::swi::mip_geometry(&grid, &ctx.meta.affine, window)
        .map_err(QsmxtError::Config)?;
    let params = serde_json::json!({
        "threshold_ppm": ctx.config.smwi.threshold_ppm,
        "power": ctx.config.smwi.power,
        "mip_window": window,
        "mip_dims": [mip_grid.nx(), mip_grid.ny(), mip_grid.nz()],
    });
    if ctx.is_cached_with_params("smwi", Some("smwi"), &params) {
        log::info!("Skipping smwi (cached)");
        return Ok(());
    }

    let qsm_path = ctx.output.qsm_path(&ctx.run.key);
    if !qsm_path.exists() {
        log::warn!("Skipping SMWI: no susceptibility map at {}", qsm_path.display());
        return Ok(());
    }
    let t = Instant::now();
    log::info!("Computing SMWI (threshold {} ppm, power {}, mIP over {} slices)",
               ctx.config.smwi.threshold_ppm, ctx.config.smwi.power, window);
    progress("Computing SMWI");

    let chi = load_volume(&qsm_path)?;
    let mag = load_volume(&ctx.output.magnitude_path(&ctx.run.key))?;
    let mask = load_mask(mask_path)?;
    let core = qsm_core::swi::SmwiParams {
        threshold_ppm: ctx.config.smwi.threshold_ppm,
        power: ctx.config.smwi.power,
        mip_window: window,
    };
    let (para, dia) = qsm_core::swi::calculate_smwi(&mag, &chi, Some(&mask), &grid, &core);

    let mut outputs = Vec::new();
    for (contrast, data) in [("paramagnetic", &para), ("diamagnetic", &dia)] {
        let smwi_path = ctx.output.smwi_path(&ctx.run.key, contrast);
        let mip_path = ctx.output.smwi_mip_path(&ctx.run.key, contrast);
        // The combined magnitude is one volume here, but calculate_smwi also accepts echoes
        // stacked along the slowest axis; collapse before projecting either way.
        let collapsed = qsm_core::swi::average_echoes(data, &grid);
        let mip = qsm_core::swi::create_mip(&collapsed, &grid, &ctx.meta.affine, window)
            .map_err(QsmxtError::Config)?;
        save_volume(&smwi_path, &collapsed, ctx.meta)?;
        write_volume(&mip_path, &mip.data, mip.grid.dims, mip.grid.voxel_size, &mip.affine)?;
        outputs.push(smwi_path);
        outputs.push(mip_path);
    }

    let mag_input = ctx.output.magnitude_path(&ctx.run.key);
    ctx.complete_step("smwi", Some("smwi"), params,
                      &[qsm_path.as_path(), mag_input.as_path(), mask_path], outputs, t)?;
    log_step_done("SMWI", t);
    Ok(())
}

/// One echo's magnitude on the working grid (0-based `echo`).
///
/// The scale_phase intermediate is the one to read: it is the same echo, already resampled when
/// the run was moved to axial, and it is the only copy when the coils were combined. The source
/// file is the fallback, and is only on the working grid when the run was not resampled.
fn load_echo_magnitude(ctx: &StageContext, echo: usize) -> crate::Result<Vec<f64>> {
    let intermediate = ctx.echo_mag_path(echo + 1);
    let path = match ctx.run.echoes[echo].magnitude_nifti.as_ref() {
        Some(raw) if !intermediate.exists() => raw.clone(),
        _ => intermediate,
    };
    let data = io::read_nifti_file(&path)
        .map_err(|e| QsmxtError::NiftiIo(format!("mag echo {} ({}): {}", echo + 1, path.display(), e)))?
        .data;
    let (nx, ny, nz) = ctx.meta.dims;
    if data.len() != nx * ny * nz {
        return Err(QsmxtError::Config(format!(
            "mag echo {}: {} has {} voxels, but this run is processed on a {}x{}x{} grid",
            echo + 1, path.display(), data.len(), nx, ny, nz,
        )));
    }
    Ok(data)
}

fn stage_t2star_r2star(ctx: &mut StageContext, mask_path: &Path, progress: &dyn Fn(&str)) -> crate::Result<()> {
    let t2r2_params = serde_json::json!({
        "n_echoes": ctx.meta.n_echoes,
        "echo_times": ctx.meta.echo_times,
    });
    if ctx.is_cached_with_params("t2star_r2star", Some("arlo"), &t2r2_params) {
        log::info!("Skipping t2star_r2star (cached)");
        return Ok(());
    }
    let t = Instant::now();
    let (nx, ny, nz) = ctx.dims();
    let n_voxels = nx * ny * nz;
    log::info!("Computing R2*/T2* maps (ARLO, {} echoes)", ctx.meta.n_echoes);
    progress("Computing R2*/T2* maps");
    let mask = load_mask(mask_path)?;

    let mut interleaved = vec![0.0f64; n_voxels * ctx.meta.n_echoes];
    for i in 0..ctx.meta.n_echoes {
        let mag_data = load_echo_magnitude(ctx, i)?;
        for vox in 0..n_voxels {
            interleaved[vox * ctx.meta.n_echoes + i] = mag_data[vox];
        }
    }

    let (vsx, vsy, vsz) = ctx.voxel_size();
    let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
    let (r2star_map, _s0) = qsm_core::utils::r2star_arlo(
        &interleaved, &mask, &ctx.meta.echo_times, &grid,
    )?;
    drop(interleaved);

    // Always compute and save both maps — T2* is trivially derived from R2*
    let r2_path = ctx.output.r2star_path(&ctx.run.key);
    save_volume(&r2_path, &r2star_map, ctx.meta)?;
    let t2star: Vec<f64> = r2star_map.iter().zip(mask.iter())
        .map(|(&r2, &m)| if m > 0 && r2 > 0.0 { 1.0 / r2 } else { 0.0 })
        .collect();
    let t2_path = ctx.output.t2star_path(&ctx.run.key);
    save_volume(&t2_path, &t2star, ctx.meta)?;
    ctx.complete_step("t2star_r2star", Some("arlo"), t2r2_params, &[mask_path], vec![r2_path, t2_path], t)?;
    log_step_done("T2*/R2* mapping", t);
    Ok(())
}

/// Say so when R2PRIMEnet is about to run on a grid that was not brought to axial.
///
/// `axial_only_algorithms` normally resamples first — R2PRIMEnet assumes B0 along `+z` and has no
/// way to be told otherwise. It decides from the config and `run_orientation_facts`, which judge
/// a declared R2 source optimistically, so a run can reach here having been promised a measured R2
/// that never materialised (a missing `--custom-r2-tool` derivative, an unreadable MESE). The
/// grid cannot be changed this late, so the one useful thing is to not be silent about it.
fn warn_if_r2primenet_grid_is_oblique(ctx: &StageContext) {
    if let Some(tilt) = r2primenet_grid_tilt(ctx.meta.source_geometry.is_some(), &ctx.meta.affine) {
        log::warn!(
            "R2PRIMEnet assumes B0 lies along +z, but this run is reconstructing on its acquired \
             grid with B0 {tilt:.1}° from the slice normal, so its R2' is off-distribution. The \
             run was not resampled because a measured R2 was expected and did not materialise. \
             Pass --obliquity-threshold to resample, or --r2prime-strategy mese to not estimate \
             R2' at all."
        );
    }
}

/// The tilt worth warning about before R2PRIMEnet runs, or `None` when there is nothing to say.
///
/// Split out from the logging so the decision can be asserted: a run resampled to a cardinal grid
/// has nothing wrong with it, and a sub-tolerance tilt is the rounding every affine carries.
fn r2primenet_grid_tilt(was_resampled: bool, affine: &[f64; 16]) -> Option<f64> {
    if was_resampled {
        return None;
    }
    let tilt = qsm_core::geometry::b0_angle_from_affine(affine);
    (tilt > AXIAL_ONLY_TOLERANCE_DEG).then_some(tilt)
}

/// The mask a MESE-measured R2 is valid over: the brain mask, narrowed to where the MESE's FOV
/// reached.
///
/// Outside that FOV the resampled echoes are zero because nothing was acquired there, not because
/// the tissue is dark, so a fit there invents an R2 out of interpolated nothing.
fn mese_measured_mask(brain: &[u8], coverage: &[u8]) -> Vec<u8> {
    brain.iter().zip(coverage.iter()).map(|(&m, &c)| ((m > 0) && (c > 0)) as u8).collect()
}

fn count_set(mask: &[u8]) -> usize {
    mask.iter().filter(|&&m| m > 0).count()
}

/// R2' = R2* - R2, over the voxels where R2 was actually measured.
///
/// `r2_mask` is where the MESE's FOV reached, when R2 came from one that does not cover the whole
/// brain. It is passed as coverage rather than substituted for the brain mask, because the two
/// mean different things to `r2prime`: the brain mask is where an R2' is wanted, the coverage is
/// where R2 exists. Subtracting over the brain mask alone would take a zero R2 for a measured one
/// and report the whole of R2* as reversible, which is a plausible-looking number standing in for
/// an absent measurement.
///
/// Returns the map and the coverage it is valid over. The coverage can be narrower than
/// `r2_mask`: `r2prime` also drops any voxel whose R2 is not finite and positive, since R2 = 0
/// means T2 = infinity and no tissue has that.
fn measured_r2prime(
    r2star: &[f64], r2: &[f64], brain: &[u8], r2_mask: Option<&[u8]>,
) -> crate::Result<(Vec<f64>, Vec<u8>)> {
    let out = qsm_core::relaxometry::r2prime(r2star, r2, brain, r2_mask)?;
    Ok((out.r2prime, out.coverage))
}

/// Whether two grids are the same one, to within the precision a NIfTI header stores.
///
/// Affines come back from 32-bit header fields, so bitwise equality is the wrong test: a volume
/// round-tripped through a header can differ in the last few bits and is still the same grid.
/// A millimetre is far below any real difference in FOV, position or orientation and far above
/// that noise.
fn same_grid(
    a_dims: (usize, usize, usize), a_affine: &[f64; 16],
    b_dims: (usize, usize, usize), b_affine: &[f64; 16],
) -> bool {
    a_dims == b_dims && a_affine.iter().zip(b_affine.iter()).all(|(x, y)| (x - y).abs() < 1e-3)
}

/// A matched MESE acquisition's magnitude, voxel-major `(n_voxels, n_se)`, on this run's grid.
///
/// The MESE is a separate acquisition, so its geometry is reconciled with the reconstruction grid
/// here instead of being assumed. Two things make them disagree, and both are ordinary rather
/// than exceptional:
///
/// - the MESE was acquired on its own grid (a 2D stack with thicker slices, a smaller matrix, a
///   tighter FOV), which is the usual shape of a spin-echo companion to a 3D GRE;
/// - the GRE was resampled to a cardinal grid, which moves it away from whatever the MESE shares.
///
/// Either way the echoes are resampled onto `dst_dims`/`dst_affine` — magnitude is continuous, so
/// trilinear interpolation is safe (wrapped phase would have to go through the complex domain
/// instead; see the `qsm_core::geometry` module docs). This used to compare voxel counts, which
/// silently dropped the whole MESE when the grids differed and silently accepted a mis-registered
/// one when they happened to agree in size.
///
/// Returns the buffer and a coverage mask marking which destination voxels the MESE's FOV
/// actually reaches: a MESE with fewer slices than the GRE leaves the rest of the grid empty, and
/// R2 must not be reported where nothing was measured.
fn load_mese_voxel_major(
    mese: &crate::bids::discovery::MeseRun,
    dst_dims: (usize, usize, usize),
    dst_affine: &[f64; 16],
) -> crate::Result<Option<(Vec<f64>, Vec<u8>)>> {
    let n_voxels = dst_dims.0 * dst_dims.1 * dst_dims.2;
    let n_se = mese.echo_times.len();
    let on_grid = same_grid(mese.dims, &mese.affine, dst_dims, dst_affine);

    // Where the MESE's FOV lands on the destination grid. Resampling ones rather than inspecting
    // the data keeps this about geometry: a genuinely dark voxel inside the FOV is measured.
    let coverage: Vec<u8> = if on_grid {
        vec![1u8; n_voxels]
    } else {
        let src_n = mese.dims.0 * mese.dims.1 * mese.dims.2;
        let Some(c) = qsm_core::geometry::resample_onto(
            &vec![1.0f64; src_n], mese.dims, &mese.affine, dst_dims, dst_affine,
        ) else {
            log::warn!(
                "MESE acquisition {} has an affine that cannot be inverted; ignoring the MESE",
                mese.key
            );
            return Ok(None);
        };
        c.iter().map(|&v| (v > 0.5) as u8).collect()
    };
    let covered = coverage.iter().filter(|&&c| c > 0).count();
    if covered == 0 {
        // Not a grid difference to interpolate across: the two acquisitions do not occupy the
        // same space at all, so there is no reading of this that produces an R2 map.
        return Err(QsmxtError::Config(format!(
            "The MESE acquisition {} shares no space with the grid this run reconstructs on \
             ({}x{}x{} vs {}x{}x{}), so R2 cannot be computed from it. Check that the MESE and \
             the GRE belong to the same subject and that their affines are not corrupt.",
            mese.key, mese.dims.0, mese.dims.1, mese.dims.2, dst_dims.0, dst_dims.1, dst_dims.2,
        )));
    }
    if !on_grid {
        log::info!(
            "Resampling the MESE onto this run's grid: {}x{}x{} -> {}x{}x{} ({:.0}% of the grid \
             is inside its FOV)",
            mese.dims.0, mese.dims.1, mese.dims.2, dst_dims.0, dst_dims.1, dst_dims.2,
            100.0 * covered as f64 / n_voxels as f64,
        );
    }

    let mut se = vec![0.0f64; n_voxels * n_se];
    for (i, p) in mese.magnitude_niftis.iter().enumerate() {
        let nf = match io::read_nifti_file(p) {
            Ok(nf) => nf,
            Err(e) => {
                log::warn!("MESE echo unreadable ({}): {}; ignoring the MESE", p.display(), e);
                return Ok(None);
            }
        };
        // The acquisition's grid was taken from its first echo. An echo that disagrees with it is
        // a broken series, not a second geometry to reconcile.
        if !same_grid(nf.dims, &nf.affine, mese.dims, &mese.affine) {
            log::warn!(
                "MESE echo {} is on a different grid from the first echo of its own acquisition \
                 ({}x{}x{} vs {}x{}x{}); ignoring the MESE",
                p.display(), nf.dims.0, nf.dims.1, nf.dims.2,
                mese.dims.0, mese.dims.1, mese.dims.2,
            );
            return Ok(None);
        }
        let values = if on_grid {
            nf.data
        } else {
            match qsm_core::geometry::resample_onto(
                &nf.data, mese.dims, &mese.affine, dst_dims, dst_affine,
            ) {
                Some(v) => v,
                None => {
                    log::warn!("Cannot resample MESE echo {}; ignoring the MESE", p.display());
                    return Ok(None);
                }
            }
        };
        for vox in 0..n_voxels { se[vox * n_se + i] = values[vox]; }
    }
    Ok(Some((se, coverage)))
}

/// Supplementary R2 (EPG from a MESE acquisition) and R2' = R2* − R2 maps.
///
/// Runs when `do_r2map`/`do_r2primemap` are set (chi-separation forces these on via
/// `enforce_separation_dependencies`). A bring-your-own R2/R2' map from `<bids>/derivatives/<tool>`
/// is preferred over computing. Missing inputs (no MESE, no R2*) skip the affected map with a
/// warning rather than failing the run.
fn stage_r2_r2prime(ctx: &mut StageContext, mask_path: &Path, progress: &dyn Fn(&str)) -> crate::Result<()> {
    let want_r2 = ctx.config.pipeline.do_r2map;
    let want_r2p = ctx.config.pipeline.do_r2primemap;
    if !want_r2 && !want_r2p {
        return Ok(());
    }
    let params = serde_json::json!({
        "do_r2map": want_r2, "do_r2primemap": want_r2p,
        "custom_r2": ctx.config.separation.custom_r2_tool,
        "custom_r2prime": ctx.config.separation.custom_r2prime_tool,
        "echo_times": ctx.meta.echo_times,
        "has_mese": ctx.run.mese.is_some(),
        // The MESE is reconciled onto this run's grid, so both grids belong in the cache key:
        // a MESE swapped for one on another grid produces a different R2 map from the same config.
        "mese_grid": ctx.run.mese.as_ref().map(|m| (m.dims, m.affine)),
        "grid": (ctx.meta.dims, ctx.meta.affine),
        "r2prime_strategy": format!("{}", ctx.config.separation.r2prime_strategy),
        "r2primenet": ctx.config.separation.r2primenet,
    });
    if ctx.is_cached_with_params("r2_r2prime", Some("epg"), &params) {
        log::info!("Skipping r2_r2prime (cached)");
        return Ok(());
    }
    let t = Instant::now();
    let (nx, ny, nz) = ctx.dims();
    let (vsx, vsy, vsz) = ctx.voxel_size();
    let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
    let mask = load_mask(mask_path)?;
    let mut outputs: Vec<PathBuf> = Vec::new();
    // The mask R2 was actually measured over. Narrower than the brain mask when the MESE's FOV
    // does not reach all of it; `None` means R2 came from somewhere other than a MESE.
    let mut r2_mask: Option<Vec<u8>> = None;

    // ── R2 map (Hz) ──
    let r2: Option<Vec<f64>> = if let Some(tool) = ctx.config.separation.custom_r2_tool.clone() {
        match find_custom_derivative(ctx.run, &tool, "*_R2map.nii*", &[]) {
            Some(p) => { log::info!("Using custom R2 map from {}", p.display()); Some(load_volume(&p)?) }
            None => { log::warn!("no custom R2 map under derivatives (tool: {}) — skipping R2", tool); None }
        }
    } else if let Some(mese) = ctx.run.mese.clone() {
        let facts = requirements::RunFacts::of(ctx.run, ctx.config);
        if let Some(why) = requirements::mese_problem(&facts) {
            // Refused up front when R2 was asked for; here only when something else implied it.
            log::warn!("Not measuring R2 from the MESE ({why})");
            None
        } else {
            progress("Computing R2 (EPG) from MESE");
            match load_mese_voxel_major(&mese, ctx.meta.dims, &ctx.meta.affine)? {
                Some((se, coverage)) => {
                    // R2 is only measured where the MESE's FOV reaches. Fitting outside it would
                    // put a number on interpolated zeros, and R2' = R2* - R2 would then quietly
                    // take that number as a measurement.
                    let fit_mask = mese_measured_mask(&mask, &coverage);
                    let (in_mask, in_both) = (count_set(&mask), count_set(&fit_mask));
                    if in_mask > 0 && in_both < in_mask {
                        log::warn!(
                            "The MESE covers {:.0}% of the brain mask; R2 (and so a measured R2') \
                             is left at zero over the remaining {} voxels.",
                            100.0 * in_both as f64 / in_mask as f64, in_mask - in_both,
                        );
                    }
                    let p = qsm_core::relaxometry::R2EpgParams::default();
                    let (r2_map, _b1) = qsm_core::relaxometry::r2_epg(&se, &fit_mask, &mese.echo_times, &grid, &p, None)?;
                    r2_mask = Some(fit_mask);
                    Some(r2_map)
                }
                None => None,
            }
        }
    } else {
        log::info!("No MESE acquisition and no custom R2 map — R2/R2' unavailable for this run");
        None
    };
    if let Some(ref r2map) = r2 {
        let out = ctx.output.r2_path(&ctx.run.key);
        save_volume(&out, r2map, ctx.meta)?;
        outputs.push(out);
    }

    // ── R2' map (Hz) = R2* − R2 ──
    if want_r2p {
        // Where this R2' came from, for its sidecar (#236): a predicted R2' looks exactly like a
        // measured one, and only the sidecar can tell a downstream reader which it is.
        let mut source = serde_json::Map::new();
        let r2p: Option<Vec<f64>> = if let Some(tool) = ctx.config.separation.custom_r2prime_tool.clone() {
            match find_custom_derivative(ctx.run, &tool, "*_R2primemap.nii*", &[]) {
                Some(p) => {
                    log::info!("Using custom R2' map from {}", p.display());
                    source.insert("R2primeSource".into(), serde_json::json!("supplied"));
                    source.insert("R2primeSuppliedFrom".into(), serde_json::json!(p.display().to_string()));
                    Some(load_volume(&p)?)
                }
                None => { log::warn!("no custom R2' map under derivatives (tool: {}) — skipping R2'", tool); None }
            }
        } else {
            let r2star_path = ctx.output.r2star_path(&ctx.run.key);
            let strategy = ctx.config.separation.r2prime_strategy;
            // Measuring wins wherever it is possible and allowed: R2' = R2* - R2 is a measurement,
            // and R2PRIMEnet's is an estimate standing in for one.
            let can_measure = r2.is_some() && strategy != R2PrimeStrategy::R2primenet;
            if can_measure {
                match (r2star_path.exists(), r2.as_ref()) {
                    (true, Some(r2map)) => {
                        log::info!("R2' measured as R2* - R2");
                        let r2s = load_volume(&r2star_path)?;
                        // Subtract only where R2 was measured: outside the MESE's FOV, R2 is zero
                        // because nothing was acquired there, and R2' = R2* - 0 would report the
                        // whole of R2* as reversible.
                        let (r2p, coverage) =
                            measured_r2prime(&r2s, r2map, &mask, r2_mask.as_deref())?;
                        // Published, so a 0 in the R2' map can be told apart from R2* = R2.
                        let cov_path = ctx.output.r2prime_coverage_path(&ctx.run.key);
                        save_mask(&cov_path, &coverage, ctx.meta)?;
                        outputs.push(cov_path);
                        let (in_mask, measured) = (count_set(&mask), count_set(&coverage));
                        if in_mask > 0 && measured < in_mask {
                            log::warn!(
                                "R2' is a measurement over {:.0}% of the brain mask; the \
                                 remaining {} voxels are left at zero because no R2 was measured \
                                 there. Maps derived from this R2' inherit that gap.",
                                100.0 * measured as f64 / in_mask as f64, in_mask - measured,
                            );
                        }
                        source.insert("R2primeSource".into(), serde_json::json!("measured"));
                        source.insert("R2primeMethod".into(), serde_json::json!(match &ctx.config.separation.custom_r2_tool {
                            Some(tool) => format!("R2* - R2 (R2 supplied by {tool})"),
                            None => "R2* - R2 (R2 by EPG fit of the MESE)".to_string(),
                        }));
                        Some(r2p)
                    }
                    _ => { log::warn!("R2' needs both R2* and R2 - one is missing; skipping R2'"); None }
                }
            } else if strategy == R2PrimeStrategy::Mese {
                log::warn!(
                    "Skipping R2': no R2 to subtract (no MESE acquisition and no custom R2 map), \
                     and --r2prime-strategy mese does not estimate one. Use `auto` or \
                     `r2primenet` to predict R2' from R2* instead."
                );
                None
            } else if !r2star_path.exists() {
                log::warn!("Skipping R2': R2PRIMEnet predicts it from R2*, which was not computed");
                None
            } else {
                // Auto with no MESE, or r2primenet outright.
                let field = ctx.meta.field_strength;
                match r2primenet_field_policy(
                    field, strategy, ctx.config.separation.r2primenet.ignore_field_mismatch,
                ) {
                    R2primenetFieldPolicy::Skip => {
                        log::warn!(
                            "Skipping R2': there is no R2 to subtract, and {}. Pass \
                             --r2primenet-ignore-field-mismatch to predict it anyway (uncalibrated).",
                            r2primenet_field_message(field));
                        None
                    }
                    R2primenetFieldPolicy::Refuse => {
                        return Err(QsmxtError::Config(format!(
                            "--r2prime-strategy r2primenet: {}; or pass \
                             --r2primenet-ignore-field-mismatch to run it anyway and treat the R2' \
                             (and any chi-separation built on it) as uncalibrated.",
                            r2primenet_field_message(field))));
                    }
                    policy => {
                        if policy == R2primenetFieldPolicy::RunUncalibrated {
                            log::warn!(
                                "{}. Running anyway (--r2primenet-ignore-field-mismatch): treat this \
                                 R2', and any chi-separation built on it, as uncalibrated.",
                                r2primenet_field_message(field));
                        }
                        if strategy == R2PrimeStrategy::Auto {
                            log::info!("No R2 to subtract - estimating R2' from R2* with R2PRIMEnet");
                        }
                        warn_if_r2primenet_grid_is_oblique(ctx);
                        prefetch_weights("r2primenet", &ctx.run.key.to_string())?;
                        progress("Estimating R2' (R2PRIMEnet)");
                        let r2s = load_volume(&r2star_path)?;
                        let predicted = run_r2primenet(ctx, &r2s, &mask, &grid)?;
                        log::info!("R2' estimated by R2PRIMEnet (an estimate, not a measurement)");
                        source.insert("R2primeSource".into(), serde_json::json!("predicted"));
                        source.insert("R2primeMethod".into(), serde_json::json!("R2PRIMEnet (from R2*)"));
                        source.insert("R2primenetTrainedFieldStrength".into(),
                                      serde_json::json!(R2PRIMENET_TRAINED_B0_T));
                        source.insert("R2primenetCalibrated".into(),
                                      serde_json::json!(policy == R2primenetFieldPolicy::Run));
                        Some(predicted)
                    }
                }
            }
        };
        if let Some(ref r2pv) = r2p {
            let out = ctx.output.r2prime_path(&ctx.run.key);
            save_volume(&out, r2pv, ctx.meta)?;
            write_r2prime_sidecar(&out, source)?;
            outputs.push(out);
        }
    }

    if !outputs.is_empty() {
        ctx.complete_step("r2_r2prime", Some("epg"), params, &[mask_path], outputs, t)?;
        log_step_done("R2/R2' mapping", t);
    }
    Ok(())
}

/// The voxels χ-separation is fitted on and writes.
///
/// `published` is the support referencing wrote for the pipeline's own χ (`desc-qsm_mask`); with
/// none — a bring-your-own χ — it is the brain mask less the voxels `chi` leaves undefined (the
/// rule of [`crate::pipeline::referencing::defined_support`]). `local_field`, for the methods that
/// fit it, limits the support further to where the field is defined: a two-pass χ is the union
/// of both passes' supports, but the local field is the main pass's alone.
fn separation_support(
    brain: &[u8], published: Option<&[u8]>, chi: &[f64], local_field: Option<&[f64]>,
) -> Vec<u8> {
    let mut support: Vec<u8> = match published {
        Some(p) => p.iter().zip(brain).map(|(&p, &b)| u8::from(p != 0 && b != 0)).collect(),
        None => crate::pipeline::referencing::defined_support(&[brain], chi),
    };
    if let Some(field) = local_field {
        for (s, &f) in support.iter_mut().zip(field) {
            if f == 0.0 || !f.is_finite() {
                *s = 0;
            }
        }
    }
    support
}

/// Write 0 wherever `support` is 0.
fn zero_outside(support: &[u8], map: &mut [f64]) {
    for (v, &s) in map.iter_mut().zip(support) {
        if s == 0 {
            *v = 0.0;
        }
    }
}

/// Susceptibility source separation (chi-separation).
///
/// Splits the conventional QSM (χ_total) into paramagnetic (χ+) and diamagnetic (χ−) maps.
/// Inputs are loaded from pipeline outputs (or bring-your-own derivatives): the QSM (`Chimap`),
/// local field, multi-echo magnitude (+ RSS), R2* and R2' (produced by earlier stages). Methods
/// whose required inputs are unavailable are skipped with a warning rather than failing the run.
/// The fit runs over, and the outputs are 0 outside, χ's support: see [`separation_support`].
fn stage_chi_separation(ctx: &mut StageContext, mask_path: &Path, progress: &dyn Fn(&str)) -> crate::Result<()> {
    let alg = ctx.config.separation.algorithm;
    let alg_name = format!("{}", alg);

    // Every method needs a conventional QSM (χ_total) — pipeline output or a custom derivative.
    let qsm_path = match ctx.config.separation.custom_qsm_tool.clone() {
        Some(tool) => match find_custom_derivative(ctx.run, &tool, "*_Chimap.nii*", &["desc-"]) {
            Some(p) => { log::info!("Using custom QSM from {}", p.display()); p }
            None => return Err(QsmxtError::Config(format!(
                "{}: --do-chisep ({}) needs a QSM, and --use-custom-qsm {} names no Chimap for \
                 this run under derivatives/{}", ctx.run.key, alg_name, tool, tool))),
        },
        None => {
            let p = ctx.output.qsm_path(&ctx.run.key);
            if !p.exists() {
                return Err(QsmxtError::Config(format!(
                    "{}: --do-chisep ({}) needs a QSM, and none was written at {}",
                    ctx.run.key, alg_name, p.display())));
            }
            p
        }
    };

    let sep_params = serde_json::json!({
        "algorithm": alg_name,
        "echo_times": ctx.meta.echo_times,
        "field_strength": ctx.meta.field_strength,
        "has_mese": ctx.run.mese.is_some(),
        "custom_qsm": ctx.config.separation.custom_qsm_tool,
        "custom_r2prime": ctx.config.separation.custom_r2prime_tool,
        // `domain` versions where the fit runs, so maps fitted over the whole brain mask by an
        // older release (the eroded rim taken as χ = 0) are recomputed rather than reused.
        "domain": "support",
    });
    if ctx.is_cached_with_params("chi_separation", Some(&alg_name), &sep_params) {
        log::info!("Skipping chi_separation (cached)");
        return Ok(());
    }

    let t = Instant::now();
    let (nx, ny, nz) = ctx.dims();
    let n_voxels = nx * ny * nz;
    progress("Chi-separation");
    log::info!("Chi-separation ({})", alg_name);
    let brain = load_mask(mask_path)?;
    let mut qsm = load_volume(&qsm_path)?;

    // Multi-echo magnitude (voxel-major) + RSS over echoes, when magnitude is available.
    let (magnitude_multi, magnitude_rss): (Option<Vec<f64>>, Option<Vec<f64>>) = if ctx.meta.has_magnitude {
        let mut interleaved = vec![0.0f64; n_voxels * ctx.meta.n_echoes];
        let mut rss = vec![0.0f64; n_voxels];
        for i in 0..ctx.meta.n_echoes {
            let mag = load_echo_magnitude(ctx, i)?;
            for vox in 0..n_voxels {
                interleaved[vox * ctx.meta.n_echoes + i] = mag[vox];
                rss[vox] += mag[vox] * mag[vox];
            }
        }
        for v in rss.iter_mut() { *v = v.sqrt(); }
        (Some(interleaved), Some(rss))
    } else {
        (None, None)
    };

    // R2* / R2' — from stage_t2star_r2star / stage_r2_r2prime, or a bring-your-own R2' map.
    let mut r2star: Option<Vec<f64>> = {
        let p = ctx.output.r2star_path(&ctx.run.key);
        if p.exists() { Some(load_volume(&p)?) } else { None }
    };
    let mut r2prime: Option<Vec<f64>> = match ctx.config.separation.custom_r2prime_tool.clone() {
        Some(tool) => find_custom_derivative(ctx.run, &tool, "*_R2primemap.nii*", &[])
            .map(|p| load_volume(&p)).transpose()?,
        None => {
            let p = ctx.output.r2prime_path(&ctx.run.key);
            if p.exists() { Some(load_volume(&p)?) } else { None }
        }
    };

    // Multi-echo spin-echo magnitude (voxel-major) for HC-ChiSep, on this run's grid. Voxels
    // outside the MESE's FOV come back zero, the same as voxels outside the brain.
    let se_multi: Option<Vec<f64>> = match ctx.run.mese.as_ref() {
        Some(mese) => load_mese_voxel_major(mese, ctx.meta.dims, &ctx.meta.affine)?.map(|(se, _)| se),
        None => None,
    };

    // Local field (ppm) for the field-based methods.
    let mut local_field: Vec<f64> = {
        let p = ctx.output.local_field_path(&ctx.run.key);
        if p.exists() { load_volume(&p)? } else { Vec::new() }
    };

    // Gate on the inputs the chosen method requires. Chi-separation is only ever asked for, never
    // implied, so a missing input fails the run: `qsmxt run` has already refused the cases the
    // data decides (`requirements::chisep_problem`), and what is left — the local field the
    // reconstruction route did not write, an R2' that did not materialise — must not be a
    // successful run with the maps missing.
    let missing = std::cell::RefCell::new(Vec::<&str>::new());
    let need = |cond: bool, what: &'static str| -> bool {
        if !cond {
            missing.borrow_mut().push(what);
        }
        cond
    };
    let ready = match alg {
        SeparationAlgorithm::R2starQsm => need(r2star.is_some() || magnitude_multi.is_some(), "R2* or multi-echo magnitude"),
        SeparationAlgorithm::Decompose => need(magnitude_multi.is_some(), "multi-echo magnitude"),
        SeparationAlgorithm::ChiSepIlsqr => {
            need(!local_field.is_empty(), "local field") & need(r2prime.is_some(), "R2'") & need(magnitude_rss.is_some(), "magnitude")
        }
        SeparationAlgorithm::ChiSepMedi => {
            need(!local_field.is_empty(), "local field") & need(r2prime.is_some(), "R2'") & need(magnitude_rss.is_some(), "magnitude")
        }
        SeparationAlgorithm::WaveSep => need(r2prime.is_some(), "R2'"),
        SeparationAlgorithm::HcChisep => need(r2prime.is_some(), "R2'") & need(magnitude_multi.is_some(), "multi-echo magnitude"),
        // SUSEP-Net / χ-sepnet (deep learning) consume [QSM, R2', local field] → [χ+, χ−].
        SeparationAlgorithm::SusepNet | SeparationAlgorithm::ChiSepNet =>
            need(!local_field.is_empty(), "local field") & need(r2prime.is_some(), "R2'"),
    };
    if !ready {
        return Err(QsmxtError::Config(format!(
            "{}: --do-chisep ({}) cannot run: missing {}",
            ctx.run.key, alg_name, missing.borrow().join(", "))));
    }

    // Fit, and write, only where χ is defined. The pipeline's χ is 0 on the rim background-field
    // removal eroded; fitting the brain mask would treat that rim as measured χ = 0. Its support
    // is the one referencing published (`desc-qsm_mask`); a bring-your-own χ has no published
    // support, so it is the brain mask less the voxels that χ leaves undefined. The field-based
    // methods also need the local field there, so they are further limited to where it exists.
    let support_path = ctx.output.qsm_support_path(&ctx.run.key);
    let published = match ctx.config.separation.custom_qsm_tool {
        None if support_path.exists() => Some(load_mask(&support_path)?),
        _ => None,
    };
    let uses_field = matches!(alg,
        SeparationAlgorithm::ChiSepIlsqr | SeparationAlgorithm::ChiSepMedi
        | SeparationAlgorithm::SusepNet | SeparationAlgorithm::ChiSepNet);
    for (what, len) in [("brain mask", brain.len()), ("QSM", qsm.len())]
        .into_iter()
        .chain(published.as_ref().map(|p| ("QSM support", p.len())))
        .chain(uses_field.then_some(("local field", local_field.len())))
    {
        if len != n_voxels {
            return Err(QsmxtError::DimensionMismatch(format!(
                "chi-separation: {what} has {len} voxels but this run is processed on {n_voxels}")));
        }
    }
    let support = separation_support(
        &brain, published.as_deref(), &qsm, uses_field.then_some(local_field.as_slice()),
    );
    log::info!("Chi-separation over the χ support: {} of {} brain-mask voxels",
        support.iter().filter(|&&s| s != 0).count(), brain.iter().filter(|&&b| b != 0).count());
    // The maps combined with χ voxel by voxel take the same support, so no method — the spatial
    // and deep-learning ones included — sees values on the rim that χ does not have.
    zero_outside(&support, &mut qsm);
    if uses_field {
        zero_outside(&support, &mut local_field);
    }
    for map in [r2prime.as_mut(), r2star.as_mut()].into_iter().flatten() {
        if map.len() != n_voxels {
            return Err(QsmxtError::DimensionMismatch(format!(
                "chi-separation: a relaxation map has {} voxels but this run is processed on {n_voxels}",
                map.len())));
        }
        zero_outside(&support, map);
    }

    let metadata = scan_metadata(ctx.meta, ctx.meta.dims);
    let mut sep_config = qsmxt_config::bridge::to_separation_config(ctx.config);
    if let Some(mese) = &ctx.run.mese {
        sep_config.hc_chisep.se_echo_times = mese.echo_times.clone();
    }

    let inputs = qsm_core::pipeline::SeparationInputs {
        local_field_ppm: &local_field,
        qsm: &qsm,
        mask: &support,
        r2prime: r2prime.as_deref(),
        r2star: r2star.as_deref(),
        magnitude_rss: magnitude_rss.as_deref(),
        magnitude_multi: magnitude_multi.as_deref(),
        se_magnitude_multi: se_multi.as_deref(),
    };

    // Fetch DL weights (SUSEP-Net / χ-sepnet) with a download bar; no-op for classical methods.
    prefetch_weights(&alg_name, &ctx.run.key.to_string())?;
    let (mut prog, _) = iter_progress_bar(&ctx.run.key.to_string(), &alg_name);
    let mut result = qsm_core::pipeline::run_separation(inputs, &metadata, &sep_config, &mut *prog)
        .map_err(|e| QsmxtError::Config(format!("chi-separation: {}", e)))?;
    // Not every method zeroes outside the mask it is given (the networks predict everywhere).
    for map in [&mut result.chi_pos, &mut result.chi_neg, &mut result.chi_total] {
        zero_outside(&support, map);
    }

    let para_path = ctx.output.chi_para_path(&ctx.run.key);
    let dia_path = ctx.output.chi_dia_path(&ctx.run.key);
    let total_path = ctx.output.chi_sep_total_path(&ctx.run.key);
    // χ− is signed-negative in qsm-core (so χ_total = χ+ + χ−); write the diamagnetic map as its
    // magnitude |χ−| so both source maps are positive. χ_total stays the signed net susceptibility.
    let chi_dia: Vec<f64> = result.chi_neg.iter().map(|&v| v.abs()).collect();
    save_volume(&para_path, &result.chi_pos, ctx.meta)?;
    save_volume(&dia_path, &chi_dia, ctx.meta)?;
    save_volume(&total_path, &result.chi_total, ctx.meta)?;
    let mut inputs: Vec<&Path> = vec![mask_path, &qsm_path];
    if published.is_some() {
        inputs.push(&support_path);
    }
    ctx.complete_step("chi_separation", Some(&alg_name), sep_params, &inputs,
        vec![para_path, dia_path, total_path], t)?;
    log_step_done(&format!("Chi-separation ({})", alg_name), t);
    Ok(())
}

/// The robust echo fit's per-echo report, next to the field map: `<field map>_robustfit.json`.
fn robust_fit_report_path(field_path: &Path) -> PathBuf {
    let name = field_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let stem = name.strip_suffix(".nii.gz").or_else(|| name.strip_suffix(".nii")).unwrap_or(&name);
    field_path.with_file_name(format!("{stem}_robustfit.json"))
}

fn stage_unwrap(
    ctx: &mut StageContext, mask_path: &Path, field_path: &Path, progress: &dyn Fn(&str),
) -> crate::Result<()> {
    let unwrap_name = format!("{}", ctx.config.field_mapping.unwrapping_algorithm);
    let do_offset = ctx.meta.n_echoes > 1 && ctx.config.field_mapping.phase_offset_removal && unwrap_name != "laplacian";
    let unwrap_alg = if do_offset { "phase_offset_removal" } else { &unwrap_name };
    let unwrap_params = serde_json::json!({
        "n_echoes": ctx.meta.n_echoes,
        "phase_offset_removal": ctx.config.field_mapping.phase_offset_removal,
        "bipolar_correction": ctx.config.field_mapping.bipolar_correction,
        "romeo_individual": ctx.config.field_mapping.romeo.individual,
        "romeo_correct_global": ctx.config.field_mapping.romeo.correct_global,
        "echo_times": ctx.meta.echo_times,
        "field_strength": ctx.meta.field_strength,
        // The echo combination: changing it must not reuse a field map cached under another.
        "b0_estimation": format!("{}", ctx.config.field_mapping.b0_estimation),
        "b0_weight_type": format!("{}", ctx.config.field_mapping.b0_weight_type),
        "b0_weight_assumed_t2star_ms": ctx.config.field_mapping.b0_weight_assumed_t2star_ms,
        "linear_fit_estimate_offset": ctx.config.field_mapping.linear_fit.estimate_offset,
        "linear_fit_reliability_threshold": ctx.config.field_mapping.linear_fit.reliability_threshold_percentile,
        "robust_fit": ctx.config.field_mapping.robust_fit,
        // Per-echo masked Laplacian + echo combination (pipeline::fieldmap); bump on a change
        // to that path so field maps from the old one are recomputed.
        "laplacian_echo_combination": if unwrap_name == "laplacian" { 2 } else { 0 },
        "laplacian_solver": format!("{}", ctx.config.field_mapping.laplacian_solver),
        "laplacian_fft_pad": ctx.config.field_mapping.laplacian_fft_pad,
    });
    if ctx.is_cached_with_params("unwrap", Some(unwrap_alg), &unwrap_params) {
        log::info!("Skipping unwrap (cached)");
        return Ok(());
    }
    let t = Instant::now();
    if do_offset {
        log::info!("Field mapping: offset removal + {} unwrapping ({} echoes)", unwrap_name, ctx.meta.n_echoes);
    } else if ctx.meta.n_echoes > 1 {
        log::info!("Field mapping: {} unwrapping ({} echoes)", unwrap_name, ctx.meta.n_echoes);
    } else {
        log::info!("Phase unwrapping ({}, single echo)", unwrap_name);
    }
    progress("Phase unwrapping / echo combination");
    let mut phases: Vec<NiftiData> = Vec::new();
    for i in 0..ctx.meta.n_echoes {
        let p = io::read_nifti_file(&ctx.echo_phase_path(i + 1))
            .map_err(|e| QsmxtError::NiftiIo(format!("echo {}: {}", i + 1, e)))?;
        phases.push(p);
    }
    let mask = load_mask(mask_path)?;

    // Load per-echo magnitudes
    let mut magnitudes: Vec<Vec<f64>> = Vec::new();
    for i in 0..ctx.meta.n_echoes {
        let m_path = ctx.echo_mag_path(i + 1);
        if m_path.exists() {
            magnitudes.push(load_volume(&m_path)?);
        }
    }

    // Build inputs for shared pipeline stage
    let phase_slices: Vec<&[f64]> = phases.iter().map(|p| p.data.as_slice()).collect();
    let mag_slices: Vec<&[f64]> = magnitudes.iter().map(|m| m.as_slice()).collect();
    let mag_option: Option<&[&[f64]]> = if mag_slices.is_empty() { None } else { Some(&mag_slices) };

    let (fm_config, _, _, _) = crate::pipeline::config::to_pipeline_stages(ctx.config);
    let scan_meta = scan_metadata(ctx.meta, ctx.meta.dims);

    if unwrap_name == "laplacian" && ctx.meta.n_echoes > 1 {
        log::info!("Laplacian unwrapping per echo ({} solver), then {} ({} weighting)",
            ctx.config.field_mapping.laplacian_solver,
            ctx.config.field_mapping.b0_estimation, ctx.config.field_mapping.b0_weight_type);
    }
    let field = crate::pipeline::fieldmap::run_field_mapping(
        &phase_slices, mag_option, &mask, &scan_meta, &fm_config,
    ).map_err(|e| QsmxtError::Config(format!("field mapping: {}", e)))?;

    save_volume(field_path, &field.b0_field_ppm, ctx.meta)?;
    // Published as a BIDS `fieldmap`, which takes Hz (or rad/s, or T) but not ppm.
    let total_field = ctx.output.total_field_path(&ctx.run.key);
    let hz_per_ppm = hz_per_ppm(ctx.meta.field_strength);
    let field_hz: Vec<f64> = field.b0_field_ppm.iter().map(|v| v * hz_per_ppm).collect();
    save_volume(&total_field, &field_hz, ctx.meta)?;
    if let Some(sidecar) = crate::bids::entities::sidecar_path(&total_field) {
        let mut obj = read_json_object(&sidecar);
        obj.insert("Units".into(), "Hz".into());
        obj.insert("Description".into(),
            "Total (unwrapped, echo-combined) field: the B0 field offset from the scanner frequency, \
             before background-field removal".into());
        std::fs::write(&sidecar, serde_json::to_string_pretty(&serde_json::Value::Object(obj))
            .map_err(|e| QsmxtError::Config(format!("{}: {e}", sidecar.display())))?)?;
    }
    let mut outputs = vec![field_path.to_path_buf(), total_field];
    // BIDS pairs a `fieldmap` with a magnitude image for anatomical reference: the first echo's,
    // the one with the most signal.
    if let Some(first) = magnitudes.first() {
        let mag_path = ctx.output.total_field_magnitude_path(&ctx.run.key);
        save_volume(&mag_path, first, ctx.meta)?;
        if let Some(sidecar) = crate::bids::entities::sidecar_path(&mag_path) {
            let mut obj = read_json_object(&sidecar);
            obj.insert("Description".into(),
                "Magnitude of the first echo, the anatomical reference for desc-total_fieldmap".into());
            obj.insert("EchoTime".into(), serde_json::json!(ctx.meta.echo_times[0]));
            std::fs::write(&sidecar, serde_json::to_string_pretty(&serde_json::Value::Object(obj))
                .map_err(|e| QsmxtError::Config(format!("{}: {e}", sidecar.display())))?)?;
        }
        outputs.push(mag_path);
    }
    if let Some(report) = &field.robust_fit {
        crate::pipeline::fieldmap::log_robust_fit_report(report);
        let json_path = robust_fit_report_path(field_path);
        let json = serde_json::to_string_pretty(&crate::pipeline::fieldmap::robust_fit_report_json(report))
            .map_err(|e| QsmxtError::Config(format!("robust fit report: {e}")))?;
        std::fs::write(&json_path, json)?;
        outputs.push(json_path);
    }
    let phase_inputs: Vec<PathBuf> = (0..ctx.meta.n_echoes).map(|i| ctx.echo_phase_path(i + 1)).collect();
    let input_refs: Vec<&Path> = phase_inputs.iter().map(|p| p.as_path()).chain(std::iter::once(mask_path)).collect();
    ctx.complete_step("unwrap", Some(unwrap_alg), unwrap_params, &input_refs, outputs, t)?;
    log_step_done("Phase unwrapping", t);
    Ok(())
}

/// Hz per ppm of field at `field_strength` T: γ̄·B0·10⁻⁶, with the γ̄ QSM.rs converts by
/// (42.576 MHz/T), so the published Hz map is exactly the ppm one the inversions read.
pub fn hz_per_ppm(field_strength: f64) -> f64 {
    42.576e6 * field_strength * 1e-6
}

/// Per-echo phase + magnitude volumes and the phase input paths (for cache provenance).
type PhaseEchoes = (Vec<Vec<f64>>, Vec<Vec<f64>>, Vec<PathBuf>);

/// Load per-echo wrapped (offset-scaled) phase and, when present, magnitude volumes — the
/// raw inputs for the phase-domain deep-learning models (iQSM / iQSM+ / iQFM). Returns
/// `(phases, magnitudes, phase_input_paths)`; `magnitudes` may be empty (→ uniform weights).
fn load_phase_echoes(ctx: &StageContext) -> crate::Result<PhaseEchoes> {
    let mut phases = Vec::new();
    let mut phase_inputs = Vec::new();
    for i in 0..ctx.meta.n_echoes {
        let p = ctx.echo_phase_path(i + 1);
        phases.push(load_volume(&p)?);
        phase_inputs.push(p);
    }
    let mut magnitudes = Vec::new();
    for i in 0..ctx.meta.n_echoes {
        let m = ctx.echo_mag_path(i + 1);
        if m.exists() {
            magnitudes.push(load_volume(&m)?);
        }
    }
    Ok((phases, magnitudes, phase_inputs))
}

/// iQSM / iQSM+ end-to-end reconstruction from wrapped **phase** (joint unwrapping +
/// The inversion stage's cache key: the algorithm's user-facing parameters, as JSON.
///
/// A parameter missing here is a parameter whose change does not invalidate a cached
/// reconstruction, so the run silently reuses a map computed with the old value. Extracted from
/// the stage so it can be tested directly.
fn invert_params(config: &PipelineConfig) -> serde_json::Value {
    match config.inversion.algorithm {
        QsmAlgorithm::Rts => serde_json::json!({
            "delta": config.inversion.rts.delta, "mu": config.inversion.rts.mu,
            "tol": config.inversion.rts.tol, "max_iter": config.inversion.rts.max_iter,
        }),
        QsmAlgorithm::Tv => serde_json::json!({
            "lambda": config.inversion.tv.lambda, "max_iter": config.inversion.tv.max_iter,
        }),
        QsmAlgorithm::Tkd => serde_json::json!({ "threshold": config.inversion.tkd.threshold }),
        QsmAlgorithm::Tsvd => serde_json::json!({ "threshold": config.inversion.tsvd.threshold }),
        QsmAlgorithm::Ilsqr => serde_json::json!({
            "tol": config.inversion.ilsqr.tol, "max_iter": config.inversion.ilsqr.max_iter,
        }),
        // HEIDI's cache key carries the LSQR group too — its seed is an LSQR solve, so changing
        // `--lsqr-tol` changes the HEIDI result and has to invalidate it.
        QsmAlgorithm::Lsqr | QsmAlgorithm::Heidi => {
            let l = &config.inversion.lsqr;
            let mut key = serde_json::json!({
                "lsqr_residual_weighting": l.residual_weighting,
                "lsqr_fit_global_offset": l.fit_global_offset,
                "lsqr_tol": l.tol, "lsqr_max_iter": l.max_iter,
            });
            if config.inversion.algorithm == QsmAlgorithm::Heidi {
                let h = &config.inversion.heidi;
                key["heidi"] = serde_json::json!({
                    "cone_threshold": h.cone_threshold,
                    "gradient_threshold": h.gradient_threshold,
                    "apply_laplacian_correction": h.apply_laplacian_correction,
                    "laplacian_threshold": h.laplacian_threshold,
                    "gradient_mask_floor": h.gradient_mask_floor,
                    "continuation_steps": h.continuation_steps,
                    "inner_iterations": h.inner_iterations,
                    "mu_min": h.mu_min, "tol": h.tol,
                    "denoise": h.denoise,
                    "denoise_iterations": h.denoise_iterations,
                    "denoise_time_step": h.denoise_time_step,
                    "denoise_conductance": h.denoise_conductance,
                });
            }
            key
        }
        QsmAlgorithm::Tikhonov => serde_json::json!({ "lambda": config.inversion.tikhonov.lambda }),
        QsmAlgorithm::Nltv => serde_json::json!({
            "lambda": config.inversion.nltv.lambda, "max_iter": config.inversion.nltv.max_iter,
        }),
        QsmAlgorithm::Medi => serde_json::json!({
            "lambda": config.inversion.medi.lambda, "max_iter": config.inversion.medi.max_iter,
            "smv": config.inversion.medi.smv,
        }),
        QsmAlgorithm::Ndi => serde_json::json!({
            "tau": config.inversion.ndi.tau, "alpha": config.inversion.ndi.alpha,
            "max_iter": config.inversion.ndi.max_iter,
        }),
        QsmAlgorithm::Fansi => serde_json::json!({
            "is_tgv": false, "alpha1": config.inversion.fansi.alpha1,
            "max_iter": config.inversion.fansi.max_iter,
        }),
        QsmAlgorithm::FansiTgv => serde_json::json!({
            "is_tgv": true, "alpha1": config.inversion.fansi.alpha1,
            "alpha0": config.inversion.fansi.alpha0,
            "max_iter": config.inversion.fansi.max_iter,
        }),
        QsmAlgorithm::L1qsm => serde_json::json!({
            "alpha1": config.inversion.l1qsm.alpha1, "lambda": config.inversion.l1qsm.lambda,
            "max_iter": config.inversion.l1qsm.max_iter,
        }),
        QsmAlgorithm::Whqsm => serde_json::json!({
            "alpha1": config.inversion.whqsm.alpha1, "beta": config.inversion.whqsm.beta,
            "max_iter": config.inversion.whqsm.max_iter,
        }),
        QsmAlgorithm::Hdqsm => serde_json::json!({
            "alpha_l2": config.inversion.hdqsm.alpha_l2,
            "max_iter_l1": config.inversion.hdqsm.max_iter_l1,
            "max_iter_l2": config.inversion.hdqsm.max_iter_l2,
        }),
        QsmAlgorithm::AmpPe => serde_json::json!({
            "wave_order": config.inversion.amp_pe.wave_order,
            "nlevel": config.inversion.amp_pe.nlevel,
            "wave_pec": config.inversion.amp_pe.wave_pec,
            "simulated_te": config.inversion.amp_pe.simulated_te,
            "max_linearization_ite": config.inversion.amp_pe.max_linearization_ite,
            "tikhonov_beta": config.inversion.amp_pe.tikhonov_beta,
        }),
        _ => serde_json::json!({}),
    }
}

/// Whether this run actually gets two-pass artefact reduction.
///
/// Two-pass derives its reliable mask from a mask recipe, so a bring-your-own mask rules it out:
/// a mask read off disk has no recipe to leave unfilled, and `--two-pass` with
/// `--use-custom-masks` would otherwise reconstruct twice from the same mask and combine a map
/// with itself. v8 disabled it for the same reason. The fallback matters too — `--use-custom-masks`
/// falls back to computing the mask when no derivative is found, and then two-pass is fine.
fn two_pass_enabled(config: &PipelineConfig, run: &QsmRun) -> bool {
    if !config.pipeline.do_qsm || !config.masking.two_pass {
        return false;
    }
    let byo = config.masking.custom_mask_tool.as_deref()
        .and_then(|tool| find_custom_mask(run, tool));
    match byo {
        Some(path) => {
            log::warn!(
                "Ignoring two-pass artefact reduction: the mask comes from {}, so there is no \
                 recipe to derive a reliable-phase mask from",
                path.display(),
            );
            false
        }
        None => true,
    }
}

/// Run one pass's reconstruction — background removal and dipole inversion, by whichever route
/// the chosen algorithm takes.
///
/// Both passes of a two-pass run go through here, so neither can end up on a different code path
/// from the other.
fn reconstruct(
    ctx: &mut StageContext, pass: &Pass, field_path: &Path, progress: &dyn Fn(&str),
) -> crate::Result<()> {
    match ctx.config.inversion.algorithm {
        QsmAlgorithm::Tgv => stage_tgv(ctx, pass, field_path, progress),
        QsmAlgorithm::Qsmart => stage_qsmart(ctx, pass, field_path, progress),
        QsmAlgorithm::Iqsm | QsmAlgorithm::IqsmPlus => stage_iqsm(ctx, pass, progress),
        _ => stage_standard_qsm(ctx, pass, field_path, progress),
    }
}

/// Whether the configured inversion runs without a separate background-removal stage, and so
/// leaves its map defined over the brain mask rather than an eroded one.
///
/// Read off QSM.rs's `InversionConfig::input` (#265): the total-field and wrapped-phase methods
/// skip it. That does not depend on the echo count (multi-echo TGV takes the total field, a
/// single echo its wrapped phase), so any count gives the same answer.
pub(crate) fn skips_bgremove(config: &PipelineConfig) -> bool {
    !qsmxt_config::bridge::inversion_input(config, 1).needs_bg_removal()
}

/// One reconstruction pass: the mask it runs on, where its outputs go, and how its steps are
/// named in the pipeline state.
///
/// Two-pass runs background removal and dipole inversion twice, so neither the step names nor the
/// intermediate paths can be derived from the run alone — a second pass writing to the first's
/// `bgremove/` outputs would corrupt the very map it is supposed to be compared against. Every
/// stage from background removal to inversion takes one of these instead.
struct Pass {
    /// Appended to each step name; empty for the main pass.
    suffix: &'static str,
    mask: PathBuf,
    local_field: PathBuf,
    bg_mask: PathBuf,
    chi_raw: PathBuf,
}

impl Pass {
    /// The ordinary, single-pass reconstruction — also the "filled mask" pass of a two-pass run.
    ///
    /// `mask` is the mask the run's field map was computed on: the brain mask, or with motion
    /// correction the part of it every echo still covers. Background removal and inversion have to
    /// stay inside it — outside, the field was never computed.
    fn main(output: &DerivativeOutputs, key: &AcquisitionKey, mask: &Path) -> Self {
        Self {
            suffix: "",
            mask: mask.to_path_buf(),
            local_field: output.local_field_path(key),
            bg_mask: output.bg_mask_path(key),
            chi_raw: output.chi_raw_path(key),
        }
    }

    /// The reliable pass of a two-pass run: the mask with its holes left unfilled.
    fn reliable(output: &DerivativeOutputs, key: &AcquisitionKey) -> Self {
        Self {
            suffix: crate::pipeline::graph::RELIABLE_SUFFIX,
            mask: output.two_pass_mask_path(key),
            local_field: output.reliable_local_field_path(key),
            bg_mask: output.reliable_bg_mask_path(key),
            chi_raw: output.reliable_chi_raw_path(key),
        }
    }

    /// This pass's name for a pipeline step.
    fn step(&self, name: &str) -> String {
        format!("{name}{}", self.suffix)
    }

    /// How this pass is named in progress messages and the log. Empty for a single-pass run, so
    /// nothing changes for the runs that are not two-pass; without it the two passes' background
    /// removal and inversion lines are indistinguishable in the log.
    fn label(&self) -> &'static str {
        if self.suffix.is_empty() { "" } else { " (reliable pass)" }
    }

    /// Where this pass's susceptibility map is actually defined: the eroded mask background
    /// removal produced, or the brain mask when the inversion did its own background removal.
    fn support(&self, skip_bgremove: bool) -> &Path {
        if skip_bgremove { &self.mask } else { &self.bg_mask }
    }
}

/// background removal + dipole inversion in one network). Writes the raw susceptibility
/// map; referencing is applied downstream by [`stage_reference`].
fn stage_iqsm(ctx: &mut StageContext, pass: &Pass, progress: &dyn Fn(&str)) -> crate::Result<()> {
    let plus = matches!(ctx.config.inversion.algorithm, QsmAlgorithm::IqsmPlus);
    let alg = if plus { "iqsm-plus" } else { "iqsm" };
    let (mask_path, chi_raw_path) = (pass.mask.as_path(), pass.chi_raw.clone());
    let invert_step = pass.step("invert");
    let params = serde_json::json!({
        "echo_times": ctx.meta.echo_times,
        "field_strength": ctx.meta.field_strength,
        "b0_direction": [ctx.meta.b0_direction.0, ctx.meta.b0_direction.1, ctx.meta.b0_direction.2],
    });
    if ctx.is_cached_with_params(&invert_step, Some(alg), &params) {
        log::info!("Skipping {} (cached)", alg);
        return Ok(());
    }
    let t = Instant::now();
    progress(&format!("iQSM reconstruction{}", pass.label()));
    let (phases, magnitudes, phase_inputs) = load_phase_echoes(ctx)?;
    let mask = load_mask(mask_path)?;
    let scan_meta = scan_metadata(ctx.meta, ctx.meta.dims);
    let phase_refs: Vec<&[f64]> = phases.iter().map(|p| p.as_slice()).collect();
    let mag_refs: Vec<&[f64]> = magnitudes.iter().map(|m| m.as_slice()).collect();
    prefetch_weights(alg, &ctx.run.key.to_string())?;
    log::info!("{} reconstruction (end-to-end from phase)", if plus { "iQSM+" } else { "iQSM" });
    // Referencing is done by stage_reference, so request None from the pipeline runner.
    let chi = if plus {
        qsm_core::pipeline::run_iqsm_plus(&phase_refs, &mag_refs, &mask, &scan_meta, qsm_core::pipeline::QsmReference::None)
    } else {
        qsm_core::pipeline::run_iqsm(&phase_refs, &mag_refs, &mask, &scan_meta, qsm_core::pipeline::QsmReference::None)
    }.map_err(|e| QsmxtError::Config(format!("{}: {}", alg, e)))?;
    save_volume(&chi_raw_path, &chi, ctx.meta)?;
    let mut inputs: Vec<&Path> = phase_inputs.iter().map(|p| p.as_path()).collect();
    inputs.push(mask_path);
    ctx.complete_step(&invert_step, Some(alg), params, &inputs, vec![chi_raw_path], t)?;
    log_step_done(&format!("{}{}", if plus { "iQSM+" } else { "iQSM" }, pass.label()), t);
    Ok(())
}

fn stage_tgv(
    ctx: &mut StageContext, pass: &Pass, field_path: &Path, progress: &dyn Fn(&str),
) -> crate::Result<()> {
    let (mask_path, chi_raw_path) = (pass.mask.as_path(), pass.chi_raw.clone());
    let tgv_step = pass.step("tgv");
    // Multi-echo: the combined B0 field map, in ppm. Single echo: echo 1's wrapped phase, which
    // TGV unwraps itself and converts to ppm with TE1 and B0. α are in ppm either way.
    let multi_echo = ctx.meta.n_echoes > 1;
    let te1_s = ctx.meta.echo_times[0];
    let b0_t = ctx.meta.field_strength;
    let mut tgv_params = serde_json::json!({
        "iterations": ctx.config.inversion.tgv.iterations,
        "alphas_ppm": [ctx.config.inversion.tgv.alpha1, ctx.config.inversion.tgv.alpha0],
        "erosions": ctx.config.inversion.tgv.erosions,
        "step_size": ctx.config.inversion.tgv.step_size,
        "tol": ctx.config.inversion.tgv.tol,
        "input": if multi_echo { "field_ppm" } else { "wrapped_phase" },
    });
    if !multi_echo {
        tgv_params["te_ms"] = serde_json::json!(te1_s * 1000.0);
        tgv_params["field_strength"] = serde_json::json!(b0_t);
    }
    if ctx.is_cached_with_params(&tgv_step, Some("tgv"), &tgv_params) {
        log::info!("Skipping tgv (cached)");
        return Ok(());
    }
    let t = Instant::now();
    if multi_echo {
        log::info!(
            "TGV-QSM on the B0 field map (iterations={}, alphas=[{}, {}] ppm, erosions={})",
            ctx.config.inversion.tgv.iterations, ctx.config.inversion.tgv.alpha1,
            ctx.config.inversion.tgv.alpha0, ctx.config.inversion.tgv.erosions,
        );
    } else {
        log::info!(
            "TGV-QSM on wrapped echo-1 phase (iterations={}, alphas=[{}, {}] ppm, erosions={}, TE={:.3}ms, B0={:.1}T)",
            ctx.config.inversion.tgv.iterations, ctx.config.inversion.tgv.alpha1,
            ctx.config.inversion.tgv.alpha0, ctx.config.inversion.tgv.erosions, te1_s * 1000.0, b0_t,
        );
    }
    progress(&format!("TGV-QSM reconstruction{}", pass.label()));
    let mask = load_mask(mask_path)?;
    let bdir = ctx.meta.b0_direction;

    let data = if multi_echo {
        load_volume(field_path)?
    } else {
        load_volume(&ctx.echo_phase_path(1))?
    };
    let input = if multi_echo {
        qsm_core::inversion::TgvInput::FieldPpm(&data)
    } else {
        if !(te1_s.is_finite() && te1_s > 0.0 && b0_t.is_finite() && b0_t > 0.0) {
            return Err(QsmxtError::Config(format!(
                "single-echo TGV needs a positive echo time and field strength (TE1 = {te1_s} s, B0 = {b0_t} T)"
            )));
        }
        qsm_core::inversion::TgvInput::WrappedPhase { phase: &data, te_s: te1_s, b0_t }
    };

    let (nx, ny, nz) = ctx.meta.dims;
    let (vsx, vsy, vsz) = ctx.meta.voxel_size;
    let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
    let params = tgv_params_from_config(&ctx.config.inversion.tgv);
    let chi = qsm_core::inversion::tgv_qsm(input, &mask, &grid, &params, bdir, |_, _| {})?;

    save_volume(&chi_raw_path, &chi, ctx.meta)?;
    ctx.complete_step(&tgv_step, Some("tgv"), tgv_params, &[mask_path, field_path], vec![chi_raw_path], t)?;
    log_step_done(&format!("TGV-QSM{}", pass.label()), t);
    Ok(())
}

/// qsm-core's TGV parameters from QSMxT's TGV config. α are in ppm.
fn tgv_params_from_config(c: &crate::pipeline::config::TgvConfig) -> qsm_core::inversion::TgvParams {
    qsm_core::inversion::TgvParams {
        alpha0: c.alpha0 as f32,
        alpha1: c.alpha1 as f32,
        iterations: c.iterations,
        erosions: c.erosions,
        step_size: c.step_size as f32,
        tol: c.tol as f32,
    }
}

fn stage_qsmart(
    ctx: &mut StageContext, pass: &Pass, field_path: &Path, progress: &dyn Fn(&str),
) -> crate::Result<()> {
    let (mask_path, chi_raw_path) = (pass.mask.as_path(), pass.chi_raw.clone());
    let qsmart_step = pass.step("qsmart");
    let qsmart_params = serde_json::json!({
        "inversion": format!("{}", ctx.config.inversion.qsmart.inversion),
        "sdf_spatial_radius": ctx.config.inversion.qsmart.sdf_spatial_radius,
        "vasc_sphere_radius": ctx.config.inversion.qsmart.vasc_sphere_radius,
        // The inner iLSQR's tolerance is the iLSQR stage's.
        "ilsqr_tol": ctx.config.inversion.ilsqr.tol,
        "ilsqr_max_iter": ctx.config.inversion.qsmart.ilsqr_max_iter,
        "ilsqr_pad_mm": ctx.config.inversion.qsmart.ilsqr_pad_mm,
        "sdf_sigma1_stage1": ctx.config.inversion.qsmart.sdf_sigma1_stage1,
        "sdf_sigma2_stage1": ctx.config.inversion.qsmart.sdf_sigma2_stage1,
        "sdf_sigma1_stage2": ctx.config.inversion.qsmart.sdf_sigma1_stage2,
        "sdf_sigma2_stage2": ctx.config.inversion.qsmart.sdf_sigma2_stage2,
        "sdf_lower_lim": ctx.config.inversion.qsmart.sdf_lower_lim,
        "sdf_curv_constant": ctx.config.inversion.qsmart.sdf_curv_constant,
        "frangi_scale_min": ctx.config.inversion.qsmart.frangi_scale_min,
        "frangi_scale_max": ctx.config.inversion.qsmart.frangi_scale_max,
        "frangi_scale_ratio": ctx.config.inversion.qsmart.frangi_scale_ratio,
        "frangi_c": ctx.config.inversion.qsmart.frangi_c,
    });
    if ctx.is_cached_with_params(&qsmart_step, Some("qsmart"), &qsmart_params) {
        log::info!("Skipping qsmart (cached)");
        return Ok(());
    }
    let t = Instant::now();
    log::info!(
        "QSMART (inversion={}, ilsqr tol={:.0e}, max_iter={}, pad={} mm, vasc_radius={}, sdf_radius={})",
        ctx.config.inversion.qsmart.inversion,
        ctx.config.inversion.ilsqr.tol, ctx.config.inversion.qsmart.ilsqr_max_iter,
        ctx.config.inversion.qsmart.ilsqr_pad_mm,
        ctx.config.inversion.qsmart.vasc_sphere_radius, ctx.config.inversion.qsmart.sdf_spatial_radius,
    );
    progress("QSMART reconstruction");
    let field_ppm = load_volume(field_path)?;
    let mask = load_mask(mask_path)?;

    // Delegate the full two-stage QSMART reconstruction to qsm-core. The inner
    // dipole inversion (default iLSQR) is selected via config.inversion.qsmart.inversion.
    let (_, _, mut inv_config, _) = crate::pipeline::config::to_pipeline_stages(ctx.config);
    let scan_meta = scan_metadata(ctx.meta, ctx.meta.dims);

    // The vasculature sphere radius and Frangi vessel scales are configured in mm
    // (matching qsmbly); convert to voxels using the dataset voxel size before running.
    {
        let (vsx, vsy, vsz) = ctx.meta.voxel_size;
        let avg = (vsx + vsy + vsz) / 3.0;
        let q = &mut inv_config.qsmart;
        q.vasc_sphere_radius = (((q.vasc_sphere_radius as f64) / avg).round() as i32).max(2);
        q.frangi_scale_range = [q.frangi_scale_range[0] / avg, q.frangi_scale_range[1] / avg];
        q.frangi_scale_ratio = (q.frangi_scale_ratio / avg).max(0.1);
    }

    // Combined magnitude drives vasculature detection (and MEDI edge weighting if used).
    let mag_combined_path = ctx.output.magnitude_path(&ctx.run.key);
    let magnitude: Option<Vec<f64>> = if mag_combined_path.exists() {
        Some(load_volume(&mag_combined_path)?)
    } else {
        None
    };

    let (mut prog, _) = iter_progress_bar(&ctx.run.key.to_string(), "QSMART");
    // Reference with None here: stage_qsmart writes the unreferenced chi to chi_raw,
    // and the separate reference stage applies the chosen referencing.
    let chi = qsm_core::pipeline::run_qsmart(
        &field_ppm, &mask, magnitude.as_deref(), &scan_meta, &inv_config,
        qsm_core::pipeline::QsmReference::None, &mut *prog,
    ).map_err(|e| QsmxtError::Config(format!("qsmart: {}", e)))?;

    save_volume(&chi_raw_path, &chi, ctx.meta)?;
    ctx.complete_step(&qsmart_step, Some("qsmart"), qsmart_params, &[mask_path, field_path], vec![chi_raw_path], t)?;
    log_step_done(&format!("QSMART{}", pass.label()), t);
    Ok(())
}

fn stage_standard_qsm(
    ctx: &mut StageContext, pass: &Pass, field_path: &Path, progress: &dyn Fn(&str),
) -> crate::Result<()> {
    let mask_path = pass.mask.as_path();
    let (bgremove_step, invert_step) = (pass.step("bgremove"), pass.step("invert"));
    // --- Background removal ---
    // The total-field methods that reach this stage (TFI, MEDI+SMV, AutoQSM, NeXtQSM) remove or
    // model the background themselves, so the standalone BFR stage is skipped and the
    // (unwrapped) total field is fed straight in, with the brain mask.
    let skip_bgremove = skips_bgremove(ctx.config);
    // iQFM replaces unwrap+BFR: it produces the local field directly from wrapped phase.
    let is_iqfm = ctx.config.bg_removal.algorithm == BfAlgorithm::Iqfm;
    let local_field_path = pass.local_field.clone();
    let bg_mask_path = pass.bg_mask.clone();
    let bf_name = format!("{}", ctx.config.bg_removal.algorithm);
    let bf_params = match ctx.config.bg_removal.algorithm {
        BfAlgorithm::Vsharp => serde_json::json!({
            "deconvolve": ctx.config.bg_removal.vsharp.deconvolve,
            "max_radius": ctx.config.bg_removal.vsharp.max_radius,
            "min_radius": ctx.config.bg_removal.vsharp.min_radius,
            "threshold": ctx.config.bg_removal.vsharp.threshold,
        }),
        BfAlgorithm::Pdf => serde_json::json!({ "tol": ctx.config.bg_removal.pdf.tol }),
        BfAlgorithm::Lbv => serde_json::json!({ "tol": ctx.config.bg_removal.lbv.tol }),
        BfAlgorithm::Ismv => serde_json::json!({
            "radius": ctx.config.bg_removal.ismv.radius,
            "tol": ctx.config.bg_removal.ismv.tol,
            "max_iter": ctx.config.bg_removal.ismv.max_iter,
        }),
        BfAlgorithm::Sharp => serde_json::json!({
            "radius": ctx.config.bg_removal.sharp.radius,
            "threshold": ctx.config.bg_removal.sharp.threshold,
        }),
        BfAlgorithm::Resharp => serde_json::json!({
            "radius": ctx.config.bg_removal.resharp.radius,
            "tik_reg": ctx.config.bg_removal.resharp.tik_reg,
            "tol": ctx.config.bg_removal.resharp.tol,
            "max_iter": ctx.config.bg_removal.resharp.max_iter,
        }),
        BfAlgorithm::Harperella => serde_json::json!({
            "radius": ctx.config.bg_removal.harperella.radius,
            "max_iter": ctx.config.bg_removal.harperella.max_iter,
            "tol": ctx.config.bg_removal.harperella.tol,
        }),
        BfAlgorithm::Iharperella => serde_json::json!({
            "max_iter": ctx.config.bg_removal.iharperella.max_iter,
            "tol": ctx.config.bg_removal.iharperella.tol,
        }),
        // BFRnet / iQFM (deep learning) have no user-tunable parameters; cache key is the weights.
        BfAlgorithm::Bfrnet | BfAlgorithm::Iqfm => serde_json::json!({}),
    };
    if skip_bgremove {
        log::info!("Skipping background removal (single-step inversion handles it internally)");
    }
    // iQFM: produce the local field directly from wrapped phase (joint unwrap + BFR).
    if is_iqfm && !skip_bgremove && !ctx.is_cached_with_params(&bgremove_step, Some(&bf_name), &bf_params) {
        let t = Instant::now();
        progress(&format!("iQFM field preparation{}", pass.label()));
        let (phases, magnitudes, phase_inputs) = load_phase_echoes(ctx)?;
        let mask = load_mask(mask_path)?;
        let scan_meta = scan_metadata(ctx.meta, ctx.meta.dims);
        let phase_refs: Vec<&[f64]> = phases.iter().map(|p| p.as_slice()).collect();
        let mag_refs: Vec<&[f64]> = magnitudes.iter().map(|m| m.as_slice()).collect();
        prefetch_weights("iqfm", &ctx.run.key.to_string())?;
        log::info!("Background removal (iQFM, deep-learning joint unwrap+BFR)");
        let local_field = qsm_core::pipeline::run_iqfm(&phase_refs, &mag_refs, &mask, &scan_meta)
            .map_err(|e| QsmxtError::Config(format!("iQFM: {}", e)))?;
        // iQFM preserves the brain edge (no erosion): reuse the brain mask.
        save_volume(&local_field_path, &local_field, ctx.meta)?;
        save_mask(&bg_mask_path, &mask, ctx.meta)?;
        let mut inputs: Vec<&Path> = phase_inputs.iter().map(|p| p.as_path()).collect();
        inputs.push(mask_path);
        ctx.complete_step(&bgremove_step, Some(&bf_name), bf_params.clone(), &inputs,
            vec![local_field_path.clone(), bg_mask_path.clone()], t)?;
        log_step_done(&format!("Background removal (iQFM){}", pass.label()), t);
    } else if is_iqfm {
        log::info!("Skipping iQFM field preparation (cached)");
    }
    if !is_iqfm && !skip_bgremove && !ctx.is_cached_with_params(&bgremove_step, Some(&bf_name), &bf_params) {
        let t = Instant::now();
        progress(&format!("Background field removal{}", pass.label()));
        let field_ppm = load_volume(field_path)?;
        let mask = load_mask(mask_path)?;

        let (_, bg_config, _, _) = crate::pipeline::config::to_pipeline_stages(ctx.config);
        let cb = crop_box_for(ctx, &mask, "Background removal");
        let scan_meta = scan_metadata(ctx.meta, cb.dims);

        // Fetch DL weights (BFRnet) with a download bar before removal; no-op otherwise.
        prefetch_weights(&bf_name, &ctx.run.key.to_string())?;
        log::info!("Background removal ({}){}", bf_name, pass.label());
        let (mut prog, _) = iter_progress_bar(&ctx.run.key.to_string(), &bf_name);
        let bg_result = qsm_core::pipeline::run_bg_removal(
            &qsm_core::crop::crop_volume(&field_ppm, &cb),
            &qsm_core::crop::crop_volume(&mask, &cb),
            &scan_meta, &bg_config, &mut *prog,
        ).map_err(|e| QsmxtError::Config(format!("bg removal: {}", e)))?;

        // Back onto the full grid: the local field and the eroded mask are both undefined
        // outside the box, so zero is the right fill for each.
        let (local_field, eroded_mask) = (
            qsm_core::crop::uncrop_volume(&bg_result.local_field_ppm, &cb, 0.0),
            qsm_core::crop::uncrop_volume(&bg_result.eroded_mask, &cb, 0u8),
        );
        save_volume(&local_field_path, &local_field, ctx.meta)?;
        save_mask(&bg_mask_path, &eroded_mask, ctx.meta)?;
        ctx.complete_step(&bgremove_step, Some(&bf_name),
            bf_params.clone(), &[field_path, mask_path],
            vec![local_field_path.clone(), bg_mask_path.clone()], t,
        )?;
        log_step_done(&format!("Background removal ({}){}", bf_name, pass.label()), t);
    } else if !skip_bgremove {
        log::info!("Skipping bgremove (cached)");
    }

    // --- Dipole inversion ---
    let chi_raw_path = pass.chi_raw.clone();
    let alg_name = format!("{}", ctx.config.inversion.algorithm);
    let invert_params = invert_params(ctx.config);
    if !ctx.is_cached_with_params(&invert_step, Some(&alg_name), &invert_params) {
        let t = Instant::now();
        progress(&format!("Dipole inversion{}", pass.label()));
        let local_field = if skip_bgremove { load_volume(field_path)? } else { load_volume(&local_field_path)? };
        let eroded_mask = if skip_bgremove { load_mask(mask_path)? } else { load_mask(&bg_mask_path)? };

        let (_, _, inv_config, _) = crate::pipeline::config::to_pipeline_stages(ctx.config);
        // The dipole kernel has infinite support, so this is the stage most exposed to the
        // periodic boundary moving inward; the margin is there to keep it off the object.
        let cb = crop_box_for(ctx, &eroded_mask, "Dipole inversion");
        let scan_meta = scan_metadata(ctx.meta, cb.dims);

        // Load combined magnitude for MEDI edge weighting
        let mag_combined_path = ctx.output.magnitude_path(&ctx.run.key);
        let magnitude: Option<Vec<f64>> = if mag_combined_path.exists() {
            Some(qsm_core::crop::crop_volume(&load_volume(&mag_combined_path)?, &cb))
        } else {
            None
        };

        // Fetch DL weights (with a download bar) before inference; no-op for classical algs.
        prefetch_weights(&alg_name, &ctx.run.key.to_string())?;
        log::info!("Dipole inversion ({}){}", alg_name, pass.label());
        let (mut prog, _) = iter_progress_bar(&ctx.run.key.to_string(), &alg_name);
        let chi = qsm_core::pipeline::run_dipole_inversion(
            &qsm_core::crop::crop_volume(&local_field, &cb),
            &qsm_core::crop::crop_volume(&eroded_mask, &cb),
            &scan_meta, &inv_config, magnitude.as_deref(), &mut *prog,
        ).map_err(|e| QsmxtError::Config(format!("inversion: {}", e)))?;
        let chi = qsm_core::crop::uncrop_volume(&chi, &cb, 0.0);
        save_volume(&chi_raw_path, &chi, ctx.meta)?;
        let lf_input = if skip_bgremove { field_path } else { local_field_path.as_path() };
        let mask_input = if skip_bgremove { mask_path } else { bg_mask_path.as_path() };
        ctx.complete_step(&invert_step, Some(&alg_name),
            invert_params, &[lf_input, mask_input], vec![chi_raw_path], t,
        )?;
        log_step_done(&format!("Dipole inversion ({}){}", ctx.config.inversion.algorithm, pass.label()), t);
    } else {
        log::info!("Skipping invert (cached)");
    }
    Ok(())
}

/// Reference a raw susceptibility map and write it out as a final derivative.
///
/// A two-pass run calls this twice — once for the combined map and once for the single-pass one —
/// so the input, the output and the step name are all explicit.
///
/// `support_masks` are the masks the map was reconstructed on ([`Pass::support`]: the
/// background-removal mask, or the brain mask when the inversion removed the background itself;
/// both passes' for a two-pass map). Every reference is measured over the map's support only —
/// see [`crate::pipeline::referencing::defined_support`] — and the map is written as 0 outside it.
/// Referencing over the brain mask instead averaged in the rim background removal eroded (0 in
/// the raw map), biasing the offset, and then left that rim at a constant `-offset`.
///
/// `support_out`, when given, is where the support is published.
#[allow(clippy::too_many_arguments)]
fn stage_reference(
    ctx: &mut StageContext, support_masks: &[&Path], chi_raw_path: &Path, qsm_path: PathBuf,
    support_out: Option<PathBuf>, step: &str, progress: &dyn Fn(&str),
) -> crate::Result<()> {
    let ref_method = format!("{}", ctx.config.qsm.reference);
    // `domain` versions what the reference is measured over, so maps referenced over the brain
    // mask by an older release are re-referenced rather than reused from the cache.
    let mut ref_params = serde_json::json!({ "method": ref_method, "domain": "support" });

    // A region reference depends on the parcellation as well as the map. Rather than a static
    // `segmentation -> reference` edge — which would invalidate referencing, and everything that
    // reads the referenced map, whenever any SynthSeg setting changed on a `mean` run — the
    // dependency is carried here, only when it is real: the region, and the hash of the
    // segmentation that measured it.
    let region = if ctx.config.qsm.reference == QsmReference::Region {
        let spec = ctx.config.qsm.reference_region.clone().unwrap_or_default();
        let ids = qsmxt_config::regions::resolve(&spec, ctx.config.segmentation.version)
            .map_err(|e| QsmxtError::Config(format!("--qsm-reference: {e}")))?;
        ref_params["region"] = serde_json::json!(spec);
        ref_params["region_ids"] = serde_json::json!(ids);
        ref_params["segmentation"] = serde_json::json!(
            ctx.state.completed_steps.get("segmentation").and_then(|r| r.params_hash.clone()));
        Some((spec, ids))
    } else {
        None
    };

    if ctx.is_cached_with_params(step, Some(&ref_method), &ref_params) {
        log::info!("Skipping {} (cached)", step);
        return Ok(());
    }
    let t = Instant::now();
    log::info!("QSM referencing ({})", ctx.config.qsm.reference);
    progress("Referencing QSM");
    let chi = load_volume(chi_raw_path)?;
    let declared = support_masks.iter().map(|p| load_mask(p)).collect::<crate::Result<Vec<_>>>()?;
    let declared_refs: Vec<&[u8]> = declared.iter().map(|m| m.as_slice()).collect();

    let mut inputs: Vec<PathBuf> = vec![chi_raw_path.to_path_buf()];
    inputs.extend(support_masks.iter().map(|p| p.to_path_buf()));
    let dseg = match region {
        Some((spec, _)) => {
            let dseg_path = resolve_input(
                ctx, &ctx.output.dseg_path(&ctx.run.key),
                ctx.config.segmentation.custom_dseg_tool.as_deref(), "*_dseg.nii*", &[],
            )
            .ok_or_else(|| QsmxtError::Config(format!(
                "--qsm-reference {spec}: no segmentation to measure the region on. Enable \
                 --do-segmentation, or supply one with --use-custom-dseg")))?;
            let dseg = load_volume(&dseg_path)?;
            inputs.push(dseg_path);
            Some(dseg)
        }
        None => None,
    };
    let Referenced { chi: chi_final, support: mask, offset } =
        reference_chi(ctx.config, &chi, &declared_refs, dseg.as_deref())?;

    save_volume(&qsm_path, &chi_final, ctx.meta)?;
    let mut outputs = vec![qsm_path.clone()];
    if let Some(path) = support_out {
        save_mask(&path, &mask, ctx.meta)?;
        outputs.push(path);
    }

    // Record what the map was referenced to, beside the map itself. Each map gets its own: the
    // single-pass map of a two-pass run is referenced over its own support, so its offset is not
    // the combined map's, and the tables read every map's reference from its own sidecar.
    write_reference_sidecar(ctx, &qsm_path, offset)?;

    let input_refs: Vec<&Path> = inputs.iter().map(|p| p.as_path()).collect();
    ctx.complete_step(step, Some(&ref_method), ref_params, &input_refs, outputs, t)?;
    log_step_done("QSM referencing", t);
    Ok(())
}

/// A susceptibility map after referencing: the map, the support it was referenced over, and what
/// the reference removed (when that is a single number worth recording).
pub(crate) struct Referenced {
    pub chi: Vec<f64>,
    pub support: Vec<u8>,
    pub offset: Option<crate::pipeline::referencing::Offset>,
}

/// Reference `chi` with `--qsm-reference`, over the voxels it is defined on within `declared`.
///
/// Shared by the per-run maps and the multi-orientation (COSMOS) map, so every susceptibility
/// map QSMxT publishes is referenced the same way. `dseg` must be on `chi`'s grid; it is needed
/// only for a region reference.
pub(crate) fn reference_chi(
    config: &PipelineConfig, chi: &[f64], declared: &[&[u8]], dseg: Option<&[f64]>,
) -> crate::Result<Referenced> {
    if let Some(m) = declared.iter().find(|m| m.len() != chi.len()) {
        return Err(QsmxtError::DimensionMismatch(format!(
            "support mask has {} voxels but the susceptibility map has {}", m.len(), chi.len())));
    }
    let support = crate::pipeline::referencing::defined_support(declared, chi);
    log::info!(
        "Referencing over the map's support: {} voxels",
        support.iter().filter(|&&m| m != 0).count(),
    );
    if config.qsm.reference == QsmReference::Region {
        let spec = config.qsm.reference_region.clone().unwrap_or_default();
        let ids = qsmxt_config::regions::resolve(&spec, config.segmentation.version)
            .map_err(|e| QsmxtError::Config(format!("--qsm-reference: {e}")))?;
        let dseg = dseg.ok_or_else(|| QsmxtError::Config(format!(
            "--qsm-reference {spec}: no segmentation to measure the region on")))?;
        if dseg.len() != chi.len() {
            return Err(QsmxtError::DimensionMismatch(format!(
                "segmentation has {} voxels but the susceptibility map has {}",
                dseg.len(), chi.len())));
        }
        let roi = crate::pipeline::referencing::region_mask(dseg, &ids);
        let (out, off) = crate::pipeline::referencing::reference_to_region(chi, &support, &roi, &spec)?;
        return Ok(Referenced { chi: out, support, offset: Some(off) });
    }
    let (_, _, _, ref_method_core) = crate::pipeline::config::to_pipeline_stages(config);
    // What the mean reference removed is worth recording for the same reason a region's offset
    // is; `apply_reference` applies it without saying.
    let offset = (config.qsm.reference == QsmReference::Mean)
        .then(|| crate::pipeline::referencing::mask_mean(chi, &support))
        .flatten();
    let out = qsm_core::pipeline::apply_reference(chi, &support, ref_method_core);
    Ok(Referenced { chi: out, support, offset })
}

/// The `QsmReference*` sidecar fields for a map referenced under `config`.
pub(crate) fn reference_sidecar_fields(
    config: &PipelineConfig, offset: Option<&crate::pipeline::referencing::Offset>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut obj = serde_json::Map::new();
    obj.insert("QsmReference".into(), serde_json::json!(config.qsm.reference_spec()));
    if let Some(off) = offset {
        obj.insert("QsmReferenceOffsetPpm".into(), serde_json::json!(off.ppm));
        obj.insert("QsmReferenceVoxels".into(), serde_json::json!(off.voxels));
    }
    if config.qsm.reference == QsmReference::Region {
        let spec = config.qsm.reference_region.clone().unwrap_or_default();
        if let Ok(ids) = qsmxt_config::regions::resolve(&spec, config.segmentation.version) {
            obj.insert("QsmReferenceLabels".into(), serde_json::json!(ids));
        }
    }
    obj
}

/// Record the reference in the susceptibility map's JSON sidecar.
///
/// The offset is a measurement in its own right — the susceptibility of the reference tissue — and
/// it is what shows whether a subject's reference was sound: one far from the cohort, or measured
/// on a handful of voxels, means a parcellation that went wrong rather than a brain that differs.
///
/// Merged into whatever is already there, since BIDS-Prov adds `GeneratedBy` to the same file.
fn write_reference_sidecar(
    ctx: &StageContext, qsm_path: &Path,
    offset: Option<crate::pipeline::referencing::Offset>,
) -> crate::Result<()> {
    let Some(sidecar) = crate::bids::entities::sidecar_path(qsm_path) else { return Ok(()) };
    let mut obj = read_json_object(&sidecar);
    obj.extend(reference_sidecar_fields(ctx.config, offset.as_ref()));
    if let Some(parent) = sidecar.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&sidecar, serde_json::to_string_pretty(&obj)
        .map_err(|e| QsmxtError::Config(format!("{}: {e}", sidecar.display())))?)?;
    Ok(())
}

/// Record where an R2' map came from in its sidecar, replacing what an earlier run recorded.
/// Merged into the rest of the file, which BIDS-Prov also writes to.
fn write_r2prime_sidecar(
    r2prime_path: &Path, source: serde_json::Map<String, serde_json::Value>,
) -> crate::Result<()> {
    let Some(sidecar) = crate::bids::entities::sidecar_path(r2prime_path) else { return Ok(()) };
    let mut obj = read_json_object(&sidecar);
    obj.retain(|k, _| !k.starts_with("R2prime"));
    obj.extend(source);
    std::fs::write(&sidecar, serde_json::to_string_pretty(&obj)
        .map_err(|e| QsmxtError::Config(format!("{}: {e}", sidecar.display())))?)?;
    Ok(())
}

/// A JSON file's top-level object, or an empty one if it is absent or unreadable.
fn read_json_object(path: &Path) -> serde_json::Map<String, serde_json::Value> {
    std::fs::read_to_string(path).ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

/// Combine the two reconstructions of a two-pass run into one raw susceptibility map.
///
/// The reliable pass wins wherever it produced a value; the ordinary pass fills the holes its mask
/// left, and the rim background removal eroded off it. Nothing is averaged — each voxel comes from
/// exactly one pass — so this cannot blur the reliable pass's values with the ones it was run to
/// avoid.
fn stage_two_pass_combine(
    ctx: &mut StageContext, main: &Pass, reliable: &Pass, skip_bgremove: bool,
    progress: &dyn Fn(&str),
) -> crate::Result<()> {
    let out_path = ctx.output.two_pass_chi_raw_path(&ctx.run.key);
    let params = serde_json::json!({ "support": reliable.support(skip_bgremove).to_string_lossy() });
    if ctx.is_cached_with_params("twopass", None, &params) {
        log::info!("Skipping twopass (cached)");
        return Ok(());
    }
    let t = Instant::now();
    progress("Combining two-pass reconstructions");

    let support_path = reliable.support(skip_bgremove).to_path_buf();
    let chi_reliable = load_volume(&reliable.chi_raw)?;
    let chi_main = load_volume(&main.chi_raw)?;

    // Where the reliable pass actually produced a value: its declared mask, minus any voxel the
    // reconstruction left at exactly zero. Several algorithms erode beyond the mask they were
    // handed — V-SHARP by its kernel radius, TGV by `tgv_erosions` — and trusting the mask file
    // over that rim would ring every hole with a seam of zeros instead of filling it from the
    // single-pass map. An exact 0.0 inside a reconstruction is masking, not a measurement.
    let declared = load_mask(&support_path)?;
    let support: Vec<u8> = declared.iter().zip(&chi_reliable)
        .map(|(&m, &chi)| u8::from(m != 0 && chi != 0.0))
        .collect();
    let kept = support.iter().filter(|&&v| v != 0).count();
    log::info!(
        "Two-pass combination: reliable pass over {} voxels ({:.1}% of the brain mask), \
         single-pass elsewhere",
        kept,
        100.0 * kept as f64 / load_mask(&main.mask)?.iter().filter(|&&v| v != 0).count().max(1) as f64,
    );

    let combined = qsm_core::pipeline::combine_two_pass(&chi_reliable, &chi_main, &support)
        .map_err(|e| QsmxtError::Config(format!("two-pass combination: {}", e)))?;
    save_volume(&out_path, &combined, ctx.meta)?;
    ctx.complete_step(
        "twopass", None, params,
        &[reliable.chi_raw.as_path(), main.chi_raw.as_path(), support_path.as_path()],
        vec![out_path], t,
    )?;
    log_step_done("Two-pass combination", t);
    Ok(())
}


/// Cache keys cover the whole configuration each step reads (#272).
#[cfg(test)]
mod cache_key_tests {
    use super::*;

    /// Every leaf of `value`, as a JSON pointer.
    fn leaves(value: &serde_json::Value, path: String, out: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(m) => for (k, v) in m { leaves(v, format!("{path}/{k}"), out) },
            serde_json::Value::Array(a) if !a.is_empty() && !a[0].is_object() => out.push(format!("{path}/0")),
            serde_json::Value::Array(a) => for (i, v) in a.iter().enumerate() { leaves(v, format!("{path}/{i}"), out) },
            _ => out.push(path),
        }
    }

    /// A different value of the same type, or `None` where there is no generic one (strings —
    /// enum names — and absent options).
    fn changed(v: &serde_json::Value) -> Option<serde_json::Value> {
        use serde_json::Value as V;
        Some(match v {
            V::Bool(b) => V::Bool(!b),
            V::Number(n) if n.is_u64() => serde_json::json!(n.as_u64().unwrap() + 1),
            V::Number(n) if n.is_i64() => serde_json::json!(n.as_i64().unwrap() + 1),
            V::Number(n) => serde_json::json!(n.as_f64().unwrap() * 1.5 + 0.125),
            _ => return None,
        })
    }

    /// Change each leaf of a config section, one at a time, and the step that reads the section
    /// must no longer be cached. Generated from the serialised config like `param_coverage`, so a
    /// field added later is covered without anyone listing it.
    #[test]
    fn every_leaf_of_a_consumed_section_is_in_the_cache_key() {
        let base = PipelineConfig::default();
        let base_json = serde_json::to_value(&base).unwrap();
        let params = serde_json::json!({});
        for (section, step) in [("inversion", "invert"), ("bg_removal", "bgremove"),
                                ("field_mapping", "unwrap"), ("separation", "chi_separation"),
                                ("masking", "mask-reliable"), ("swi", "swi"), ("smwi", "smwi")] {
            let mut paths = Vec::new();
            leaves(&base_json[section], format!("/{section}"), &mut paths);
            let mut tested = 0;
            for p in &paths {
                let Some(new) = changed(base_json.pointer(p).unwrap()) else { continue };
                let mut j = base_json.clone();
                *j.pointer_mut(p).unwrap() = new;
                let Ok(cfg) = serde_json::from_value::<PipelineConfig>(j) else { continue };
                tested += 1;
                assert_ne!(step_cache_key(step, None, &params, &cfg), step_cache_key(step, None, &params, &base),
                           "changing {p} leaves {step} cached");
            }
            assert!(tested * 2 >= paths.len().min(4), "{section}: only {tested} of {} leaves exercised", paths.len());
        }
    }

    /// Cropping changes what background removal and the inversion compute, so it is in their keys
    /// — and not in the field map's, which it does not touch.
    #[test]
    fn cropping_is_in_the_reconstruction_keys() {
        let base = PipelineConfig::default();
        let mut c = base.clone();
        c.pipeline.crop_to_mask = !c.pipeline.crop_to_mask;
        let p = serde_json::json!({});
        for step in ["bgremove", "invert", "tgv", "qsmart", "invert-reliable"] {
            assert_ne!(step_cache_key(step, None, &p, &c), step_cache_key(step, None, &p, &base), "{step}");
        }
        assert_eq!(step_cache_key("unwrap", None, &p, &c), step_cache_key("unwrap", None, &p, &base));
        // The two-pass settings are the reliable pass's business, not the main mask's.
        let mut t = base.clone();
        t.masking.two_pass = true;
        assert_eq!(step_cache_key("mask", None, &p, &t), step_cache_key("mask", None, &p, &base));
        assert_ne!(step_cache_key("mask-reliable", None, &p, &t), step_cache_key("mask-reliable", None, &p, &base));
    }
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
        use crate::bids::entities::AcquisitionKey;

        let output = DerivativeOutputs::new(std::path::Path::new("/out"));
        let key = AcquisitionKey {
            subject: "1".into(), session: None, acquisition: None,
            reconstruction: None, inversion: None, run: None, suffix: "MEGRE".into(),
        };
        let (main, reliable) = (super::Pass::main(&output, &key, &output.mask_path(&key)), super::Pass::reliable(&output, &key));

        for (a, b) in [
            (&main.mask, &reliable.mask),
            (&main.local_field, &reliable.local_field),
            (&main.bg_mask, &reliable.bg_mask),
            (&main.chi_raw, &reliable.chi_raw),
        ] {
            assert_ne!(a, b, "the two passes share an output path");
        }

        // And their steps are cached separately, or one pass's cache hit would skip the other.
        for step in ["mask", "bgremove", "invert", "tgv", "qsmart"] {
            assert_ne!(main.step(step), reliable.step(step));
        }
        assert_eq!(main.step("invert"), "invert", "the main pass keeps the unsuffixed step names");
    }

    /// HEIDI seeds itself with an LSQR solve, so an LSQR parameter changes the HEIDI result and
    /// must invalidate it. Its cache key therefore has to carry both groups — otherwise editing
    /// `--lsqr-tol` silently reuses a HEIDI map computed with the old tolerance.
    #[test]
    fn the_heidi_cache_key_covers_its_lsqr_seed() {
        use crate::pipeline::config::{PipelineConfig, QsmAlgorithm};
        let key = |c: &PipelineConfig| crate::pipeline::graph::step_params_hash(
            Some(&format!("{}", c.inversion.algorithm)), &super::invert_params(c));

        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Heidi;
        let base = key(&c);

        let mut edited = c.clone();
        edited.inversion.lsqr.tol *= 10.0;
        assert_ne!(key(&edited), base, "an LSQR tolerance change must invalidate HEIDI");

        let mut edited = c.clone();
        edited.inversion.lsqr.max_iter += 1;
        assert_ne!(key(&edited), base, "an LSQR iteration change must invalidate HEIDI");

        // And HEIDI's own knobs, including the flattened denoise sub-parameters.
        for mutate in [
            (|c: &mut PipelineConfig| c.inversion.heidi.cone_threshold += 0.05) as fn(&mut PipelineConfig),
            |c: &mut PipelineConfig| c.inversion.heidi.continuation_steps += 1,
            |c: &mut PipelineConfig| c.inversion.heidi.denoise = !c.inversion.heidi.denoise,
            |c: &mut PipelineConfig| c.inversion.heidi.denoise_conductance += 0.5,
            |c: &mut PipelineConfig| c.inversion.heidi.gradient_mask_floor += 0.05,
        ] {
            let mut edited = c.clone();
            mutate(&mut edited);
            assert_ne!(key(&edited), base, "a HEIDI parameter change must invalidate the cache");
        }

        // Plain LSQR must not be invalidated by HEIDI-only knobs it never reads.
        let mut l = PipelineConfig::default();
        l.inversion.algorithm = QsmAlgorithm::Lsqr;
        let lbase = key(&l);
        let mut edited = l.clone();
        edited.inversion.heidi.cone_threshold += 0.05;
        assert_eq!(key(&edited), lbase, "plain LSQR does not read HEIDI's parameters");
        let mut edited = l.clone();
        edited.inversion.lsqr.tol *= 10.0;
        assert_ne!(key(&edited), lbase, "but it does read its own");
    }

    /// A reliable mask with no holes, or no voxels, makes the second reconstruction pointless —
    /// and both are easy to produce by mistake with a slightly-wrong threshold.
    #[test]
    fn two_pass_coverage_advice_flags_a_useless_reliable_mask() {
        // A mask with real holes is the working case: no advice.
        assert!(super::two_pass_coverage_advice(800, 1000).is_none());
        assert!(super::two_pass_coverage_advice(997, 1000).is_none());

        // Essentially the whole brain — including the handful-of-voxels case that is not
        // exactly equal but is just as useless.
        assert!(super::two_pass_coverage_advice(1000, 1000).unwrap().contains("almost nothing"));
        assert!(super::two_pass_coverage_advice(999, 1000).unwrap().contains("almost nothing"));

        assert!(super::two_pass_coverage_advice(0, 1000).unwrap().contains("empty"));
        // No brain mask at all is someone else's error to report.
        assert!(super::two_pass_coverage_advice(0, 0).is_none());
    }

    /// The chain the warning exists for, end to end on real masking: a reliable recipe that fills
    /// its holes produces a mask covering the whole brain, and the advice fires on it — while the
    /// default recipe keeps its holes and stays silent.
    ///
    /// The pieces were tested separately before; this is the join between them, which is where the
    /// feature would quietly become a no-op that costs a second reconstruction.
    #[test]
    fn hole_filling_collapses_the_reliable_mask_onto_the_brain() {
        use crate::pipeline::config::{MaskCombine, MaskOp, MaskSection, MaskThresholdMethod, MaskingInput};
        use super::combine_mask_sections;

        // A 12³ block of bright signal with a dark 3³ void inside it — a strong susceptibility
        // source as a quality map sees it.
        let dims = (12usize, 12, 12);
        let (nx, ny, nz) = dims;
        let mut quality = vec![1.0f64; nx * ny * nz];
        for z in 4..7 { for y in 4..7 { for x in 4..7 {
            quality[z * nx * ny + y * nx + x] = 0.0;
        }}}
        let meta = crate::pipeline::config::to_scan_metadata(dims, (1.0, 1.0, 1.0), &[0.005], 3.0, (0.0, 0.0, 1.0));

        let recipe = |refinements: Vec<MaskOp>| crate::pipeline::config::to_mask_sections(&[MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(0.5) },
            refinements,
        }], &Default::default());
        let build = |refinements: Vec<MaskOp>| {
            combine_mask_sections(&recipe(refinements), MaskCombine::Or, &[], &[], &[&quality], &meta).unwrap()
        };
        let count = |m: &[u8]| m.iter().filter(|&&v| v != 0).count();

        // The brain mask, standing in for the main pass: holes filled.
        let brain = build(vec![MaskOp::FillHoles { max_size: 0 }]);
        assert_eq!(count(&brain), nx * ny * nz, "the filled mask should have swallowed the void");

        // The default reliable recipe keeps the void, so the two passes reconstruct different
        // regions and the advice stays quiet.
        let unfilled = build(vec![]);
        assert_eq!(count(&unfilled), nx * ny * nz - 27, "the void is what the reliable pass excludes");
        assert!(super::two_pass_coverage_advice(count(&unfilled), count(&brain)).is_none());

        // Fill the holes in the *reliable* mask and it becomes the brain mask — a second
        // reconstruction of the same region.
        let filled = build(vec![MaskOp::FillHoles { max_size: 0 }]);
        assert_eq!(count(&filled), count(&brain));
        let advice = super::two_pass_coverage_advice(count(&filled), count(&brain))
            .expect("a reliable mask with no holes must be flagged");
        assert!(advice.contains("almost nothing to reduce"), "{advice}");
    }

    /// The support is the eroded mask when background removal ran, and the brain mask when the
    /// inversion did its own — picking the wrong one would hand the combination a mask that does
    /// not exist on disk.
    #[test]
    fn pass_support_follows_the_background_removal_route() {
        use crate::bids::derivatives::DerivativeOutputs;
        use crate::bids::entities::AcquisitionKey;

        let output = DerivativeOutputs::new(std::path::Path::new("/out"));
        let key = AcquisitionKey {
            subject: "1".into(), session: None, acquisition: None,
            reconstruction: None, inversion: None, run: None, suffix: "MEGRE".into(),
        };
        let pass = super::Pass::reliable(&output, &key);
        assert_eq!(pass.support(false), pass.bg_mask.as_path());
        assert_eq!(pass.support(true), pass.mask.as_path());
    }

    /// Single-step and end-to-end algorithms produce a map over the brain mask, with no separate
    /// background-removal stage to erode it.
    #[test]
    fn single_step_algorithms_skip_background_removal() {
        use crate::pipeline::config::{PipelineConfig, QsmAlgorithm};
        let mut config = PipelineConfig::default();

        for alg in [QsmAlgorithm::Rts, QsmAlgorithm::Tkd, QsmAlgorithm::Ilsqr] {
            config.inversion.algorithm = alg;
            assert!(!super::skips_bgremove(&config), "{alg} runs its own background removal stage");
        }
        for alg in [QsmAlgorithm::Nextqsm, QsmAlgorithm::Autoqsm, QsmAlgorithm::Iqsm,
                    QsmAlgorithm::IqsmPlus, QsmAlgorithm::Tgv, QsmAlgorithm::Qsmart, QsmAlgorithm::Tfi] {
            config.inversion.algorithm = alg;
            assert!(super::skips_bgremove(&config), "{alg} has no separate background removal");
        }

        // MEDI only skips it with SMV enabled.
        config.inversion.algorithm = QsmAlgorithm::Medi;
        config.inversion.medi.smv = false;
        assert!(!super::skips_bgremove(&config));
        config.inversion.medi.smv = true;
        assert!(super::skips_bgremove(&config));
    }

    /// A bring-your-own mask has no recipe to leave unfilled, so two-pass has nothing to build a
    /// reliable mask from. Without this gate the run would reconstruct twice from the same mask
    /// and "combine" a map with itself — twice the time, and an output indistinguishable from the
    /// single-pass one.
    #[test]
    fn a_custom_mask_turns_two_pass_off() {
        use crate::pipeline::config::PipelineConfig;

        let dir = tempfile::tempdir().unwrap();
        let bids = dir.path();
        let anat = bids.join("sub-1/anat");
        std::fs::create_dir_all(&anat).unwrap();
        let phase = anat.join("sub-1_echo-1_part-phase_MEGRE.nii");
        write_vol(&phase, (2, 2, 2));
        let run = run_with_echoes(vec![(phase, None)]);

        let mut config = PipelineConfig::default();
        config.masking.two_pass = true;
        assert!(super::two_pass_enabled(&config, &run), "computed mask: two-pass applies");

        // Asking for a BYO mask that is actually there turns it off.
        let deriv_anat = bids.join("derivatives/bet/sub-1/anat");
        std::fs::create_dir_all(&deriv_anat).unwrap();
        write_vol(&deriv_anat.join("sub-1_MEGRE_mask.nii"), (2, 2, 2));
        config.masking.custom_mask_tool = Some("bet".to_string());
        assert!(!super::two_pass_enabled(&config, &run));

        // --use-custom-masks falls back to computing the mask when nothing matches, and then
        // there *is* a recipe — so two-pass applies after all.
        config.masking.custom_mask_tool = Some("nonexistent".to_string());
        assert!(super::two_pass_enabled(&config, &run), "a BYO mask that isn't there is not a BYO mask");

        // And it is off whenever QSM is, or whenever it was not asked for.
        config.masking.custom_mask_tool = None;
        config.pipeline.do_qsm = false;
        assert!(!super::two_pass_enabled(&config, &run));
        config.pipeline.do_qsm = true;
        config.masking.two_pass = false;
        assert!(!super::two_pass_enabled(&config, &run));
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

    /// Two magnitude sections with fixed thresholds, so the section masks are known exactly:
    /// OR is their union, AND their intersection, and the post-combine refinements run once on
    /// the result rather than per section.
    #[test]
    fn test_combine_mask_sections_or_and_and() {
        use super::combine_mask_sections;
        use crate::pipeline::config::{MaskCombine, MaskOp, MaskSection, MaskThresholdMethod, MaskingInput};

        let dims = (4usize, 4, 4);
        let n = dims.0 * dims.1 * dims.2;
        // A ramp: voxel i has value i, so `threshold:fixed:t` keeps exactly the voxels above t.
        let mag: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let meta = crate::pipeline::config::to_scan_metadata(dims, (1.0, 1.0, 1.0), &[0.005], 3.0, (0.0, 0.0, 1.0));

        let section = |t: f64| MaskSection {
            input: MaskingInput::Magnitude,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(t) },
            refinements: vec![],
        };
        // > 10 keeps 53 voxels, > 30 keeps 33; the second is a strict subset of the first.
        let sections = crate::pipeline::config::to_mask_sections(&[section(10.0), section(30.0)], &Default::default());
        let count = |m: &[u8]| m.iter().filter(|&&v| v == 1).count();

        let or = combine_mask_sections(&sections, MaskCombine::Or, &[], &[], &[&mag], &meta).unwrap();
        assert_eq!(count(&or), n - 11, "union is the looser threshold");

        let and = combine_mask_sections(&sections, MaskCombine::And, &[], &[], &[&mag], &meta).unwrap();
        assert_eq!(count(&and), n - 31, "intersection is the tighter threshold");
        assert!(or.iter().zip(&and).all(|(o, a)| o >= a), "AND ⊆ OR");

        // One section: the combine mode makes no difference.
        let one = crate::pipeline::config::to_mask_sections(&[section(10.0)], &Default::default());
        for mode in [MaskCombine::Or, MaskCombine::And] {
            assert_eq!(combine_mask_sections(&one, mode, &[], &[], &[&mag], &meta).unwrap(), or);
        }

        // Post-combine refinements run on the combined mask.
        let refinements = crate::pipeline::config::to_mask_ops(&[MaskOp::Erode { iterations: 1 }], &Default::default());
        let eroded = combine_mask_sections(&sections, MaskCombine::And, &refinements, &[], &[&mag], &meta).unwrap();
        assert!(count(&eroded) < count(&and), "erosion shrank the combined mask");

        // No sections is a configuration error, not an empty mask.
        assert!(combine_mask_sections(&[], MaskCombine::Or, &[], &[], &[&mag], &meta).is_err());
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

    fn sidecar(thickness: Option<f64>, acq: Option<&str>, dir: Option<&str>) -> crate::bids::sidecar::QsmSidecar {
        crate::bids::sidecar::QsmSidecar {
            echo_time: 0.004, magnetic_field_strength: 3.0, b0_dir: None,
            slice_thickness: thickness, mr_acquisition_type: acq.map(str::to_string),
            slice_encoding_direction: dir.map(str::to_string), manufacturer: None,
        }
    }

    #[test]
    fn slice_geometry_is_read_only_where_a_gap_can_be_checked() {
        let vs = (0.8, 0.8, 3.0);
        assert_eq!(super::slice_geometry_from_sidecar(&sidecar(Some(2.0), Some("2D"), None), vs), Some((2.0, 2)));
        assert_eq!(super::slice_geometry_from_sidecar(&sidecar(Some(0.8), Some("2D"), Some("j-")), vs), Some((0.8, 1)));
        // 3D: the partition thickness is the pitch; nothing to check.
        assert_eq!(super::slice_geometry_from_sidecar(&sidecar(Some(3.0), Some("3D"), None), vs), None);
        // No SliceThickness: nothing can be said.
        assert_eq!(super::slice_geometry_from_sidecar(&sidecar(None, Some("2D"), None), vs), None);
        // A thickness that is not the pitch is checked even without MRAcquisitionType.
        assert_eq!(super::slice_geometry_from_sidecar(&sidecar(Some(2.0), None, None), vs), Some((2.0, 2)));
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
        let meta = super::resolve_geometry(&run, &phase, &cfg).unwrap();
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
        assert!(super::scan_metadata(&gapped, gapped.dims).require_contiguous_slices().is_err());
    }

    #[test]
    fn geometry_axial_run_is_untouched() {
        let mut cfg = crate::pipeline::config::PipelineConfig::default();
        cfg.pipeline.obliquity_threshold = 10.0;
        let affine = [
            0.8, 0.0, 0.0, 0.0, 0.0, 0.8, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let meta = super::resolve_geometry(&run_for_geometry(None), &nifti_with(affine, (4, 4, 4)), &cfg).unwrap();
        assert!(meta.source_geometry.is_none(), "an axial run must not be resampled");
        assert_eq!(meta.dims, (4, 4, 4));
        assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
    }

    #[test]
    fn geometry_oblique_without_threshold_rotates_the_kernel() {
        let cfg = crate::pipeline::config::PipelineConfig::default(); // threshold -1 (disabled)
        let meta = super::resolve_geometry(&run_for_geometry(None), &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
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
        cfg.pipeline.obliquity_threshold = 10.0; // obliquity here is ~32.5 degrees
        let meta = super::resolve_geometry(&run_for_geometry(None), &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
        let (src_dims, _) = meta.source_geometry.expect("should resample");
        assert_eq!(src_dims, (16, 16, 8), "source dims are recorded for scale_phase");
        assert!(meta.dims.0 >= 16 && meta.dims.1 >= 16 && meta.dims.2 >= 8,
                "the cardinal box is at least as large, got {:?}", meta.dims);
        assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0), "resampled grid puts B0 along z");
        assert!(qsm_core::geometry::obliquity_from_affine(&meta.affine) < 1e-9);
    }

    #[test]
    fn geometry_threshold_above_obliquity_does_not_resample() {
        let mut cfg = crate::pipeline::config::PipelineConfig::default();
        cfg.pipeline.obliquity_threshold = 45.0; // above this run's ~32.5 degrees
        let meta = super::resolve_geometry(&run_for_geometry(None), &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
        assert!(meta.source_geometry.is_none());
        assert!(meta.b0_direction.2 < 0.99, "still uses the affine direction");
    }

    #[test]
    fn geometry_sidecar_b0_wins_over_resampling() {
        let mut cfg = crate::pipeline::config::PipelineConfig::default();
        cfg.pipeline.obliquity_threshold = 10.0;
        let run = run_for_geometry(Some((0.0, 0.5, 0.866)));
        let meta = super::resolve_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
        assert!(meta.source_geometry.is_none(), "an explicit B0_dir means the grid is left alone");
        assert_eq!(meta.b0_direction, (0.0, 0.5, 0.866));
    }

    // --- reconciling a MESE's geometry with the grid the run reconstructs on ---

    /// Write `n_echoes` MESE magnitude volumes on `dims`/`affine`, echo `i` filled by `value(i)`,
    /// and return a `MeseRun` pointing at them.
    fn mese_files(
        dir: &std::path::Path,
        dims: (usize, usize, usize),
        affine: [f64; 16],
        n_echoes: usize,
        value: impl Fn(usize, usize, usize, usize) -> f64,
    ) -> crate::bids::discovery::MeseRun {
        let mut run = mese_on(dims, affine);
        run.magnitude_niftis.clear();
        run.echo_times.clear();
        for e in 0..n_echoes {
            let mut data = vec![0.0f64; dims.0 * dims.1 * dims.2];
            for k in 0..dims.2 {
                for j in 0..dims.1 {
                    for i in 0..dims.0 {
                        data[i + j * dims.0 + k * dims.0 * dims.1] = value(e, i, j, k);
                    }
                }
            }
            let path = dir.join(format!("mese_echo-{}.nii", e + 1));
            qsm_core::io::save_nifti_to_file(
                &path, &data, dims,
                qsm_core::geometry::voxel_sizes_from_affine(&affine), &affine,
            ).unwrap();
            run.magnitude_niftis.push(path);
            run.echo_times.push(0.01 * (e + 1) as f64);
        }
        run
    }

    fn diag_affine(vs: f64, origin_x: f64) -> [f64; 16] {
        [
            vs, 0.0, 0.0, origin_x,
            0.0, vs, 0.0, 0.0,
            0.0, 0.0, vs, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]
    }

    /// The broad case: a 2D-style MESE on a coarser grid of its own. The whole acquisition used to
    /// be discarded here (the voxel counts disagree), so R2 and R2' were never computed.
    #[test]
    fn mese_on_a_different_grid_is_resampled_rather_than_dropped() {
        let dir = tempfile::tempdir().unwrap();
        // 4x4x4 at 2 mm: voxel centres span 0..6 mm on every axis.
        let mese = mese_files(dir.path(), (4, 4, 4), diag_affine(2.0, 0.0), 3,
                              |e, _, _, _| 7.0 * (e + 1) as f64);
        // 7x7x7 at 1 mm over the same 0..6 mm box: every destination centre is inside the source.
        let dst_dims = (7, 7, 7);
        let dst_affine = diag_affine(1.0, 0.0);

        let (se, coverage) = super::load_mese_voxel_major(&mese, dst_dims, &dst_affine)
            .expect("a differently-gridded MESE is reconcilable, not an error")
            .expect("and must not be dropped");

        let n = dst_dims.0 * dst_dims.1 * dst_dims.2;
        assert_eq!(se.len(), n * 3, "one value per destination voxel per echo");
        assert!(coverage.iter().all(|&c| c > 0), "the destination box is inside the MESE's FOV");
        // Each echo is constant, so interpolation must reproduce it exactly — and land on the
        // right echo: the buffer is voxel-major, (n_voxels, n_se).
        for vox in 0..n {
            for e in 0..3 {
                let got = se[vox * 3 + e];
                let want = 7.0 * (e + 1) as f64;
                assert!((got - want).abs() < 1e-9,
                        "voxel {vox} echo {e}: got {got}, want {want}");
            }
        }
    }

    /// The correctness case: the same voxel count, so the old length check waved it through, but a
    /// FOV shifted 2 mm along x. Copying it voxel-for-voxel would use it mis-registered.
    #[test]
    fn mese_with_a_matching_voxel_count_but_a_shifted_fov_is_registered_not_copied() {
        let dir = tempfile::tempdir().unwrap();
        // Source value is the x index, so a mis-registration along x is visible in the values.
        let mese = mese_files(dir.path(), (8, 8, 8), diag_affine(1.0, 0.0), 3,
                              |_, i, _, _| i as f64);
        let dst_dims = (8, 8, 8);                       // same voxel count
        let dst_affine = diag_affine(1.0, 2.0);         // ... shifted 2 mm along x

        let (se, coverage) = super::load_mese_voxel_major(&mese, dst_dims, &dst_affine)
            .unwrap()
            .expect("a shifted FOV is reconcilable");

        // Along the row y = 0, z = 0, where the voxel index is just the x index.
        let at = |i: usize, e: usize| se[i * 3 + e];
        // Destination voxel i sits at world x = i + 2, which is source voxel i + 2.
        for i in 0..6 {
            assert!((at(i, 0) - (i + 2) as f64).abs() < 1e-9,
                    "dst x={i} should sample source x={}, got {}", i + 2, at(i, 0));
        }
        assert!((at(0, 0) - 2.0).abs() < 1e-9,
                "a voxel-for-voxel copy would have put source x=0 here, not x=2");
        // The two rightmost columns fall outside the source entirely.
        let covered = |i: usize| coverage[i] > 0;
        assert!(covered(5) && !covered(6) && !covered(7),
                "the shift leaves the last two columns outside the MESE's FOV");
    }

    /// The genuinely undecidable case, which still refuses: the two acquisitions do not occupy the
    /// same space, so there is no interpolation that produces an R2 map.
    #[test]
    fn mese_sharing_no_space_with_the_run_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mese = mese_files(dir.path(), (4, 4, 4), diag_affine(1.0, 0.0), 3, |_, _, _, _| 1.0);
        let err = super::load_mese_voxel_major(&mese, (4, 4, 4), &diag_affine(1.0, 1000.0))
            .expect_err("a metre apart is not a grid difference to interpolate across");
        let msg = err.to_string();
        assert!(msg.contains("shares no space"), "the error should say why: {msg}");
    }

    /// Echoes of one acquisition must agree with each other. That is a broken series rather than a
    /// second geometry to reconcile, so the MESE is dropped (with a warning) and the run goes on.
    #[test]
    fn mese_whose_echoes_disagree_with_each_other_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let mut mese = mese_files(dir.path(), (4, 4, 4), diag_affine(1.0, 0.0), 3, |_, _, _, _| 1.0);
        // Overwrite the last echo on a grid of its own.
        let odd = dir.path().join("mese_echo-3.nii");
        qsm_core::io::save_nifti_to_file(
            &odd, &[1.0f64; 2 * 2 * 2], (2, 2, 2), (1.0, 1.0, 1.0), &diag_affine(1.0, 0.0),
        ).unwrap();
        mese.magnitude_niftis[2] = odd;
        assert!(super::load_mese_voxel_major(&mese, (4, 4, 4), &diag_affine(1.0, 0.0))
                    .unwrap().is_none(),
                "a series whose echoes are on different grids cannot be fitted");
    }

    /// A MESE that does not reach all of the brain leaves R2 unmeasured there, and R2' must not
    /// quietly report R2* - 0 as if the whole of R2* were reversible.
    #[test]
    fn r2_outside_the_mese_fov_is_not_subtracted_as_a_measurement() {
        // Four voxels of brain; the MESE's FOV reaches the first two.
        let brain = [1u8, 1, 1, 1];
        let coverage = [1u8, 1, 0, 0];
        let measured = super::mese_measured_mask(&brain, &coverage);
        assert_eq!(measured, vec![1, 1, 0, 0]);
        assert_eq!(super::count_set(&measured), 2);

        let r2star = [30.0, 30.0, 30.0, 30.0];
        // R2 was fitted only inside the FOV; outside it the echoes were zero, so it is zero.
        let r2 = [10.0, 10.0, 0.0, 0.0];
        let (r2p, cov) = super::measured_r2prime(&r2star, &r2, &brain, Some(&measured)).unwrap();
        assert_eq!(&r2p[..2], &[20.0, 20.0], "measured where the MESE reached");
        assert_eq!(&r2p[2..], &[0.0, 0.0],
                   "R2* - 0 = 30 here would be an estimate dressed as a measurement");
        assert_eq!(cov, vec![1, 1, 0, 0], "the coverage reported back is the FOV, not the brain");
    }

    /// The FOV mask has to do the work on its own when R2 is non-zero outside it.
    ///
    /// A MESE resampled onto the reconstruction grid interpolates across its own boundary, so the
    /// voxels just outside the acquired slices carry small non-zero R2 values that look exactly
    /// like tissue. Nothing in the map distinguishes them, which is why `load_mese_voxel_major`
    /// resamples a volume of ones to find the FOV geometrically rather than thresholding the data.
    #[test]
    fn interpolated_r2_outside_the_fov_is_still_not_a_measurement() {
        let brain = [1u8, 1, 1, 1];
        let measured = super::mese_measured_mask(&brain, &[1u8, 1, 0, 0]);
        let r2star = [30.0, 30.0, 30.0, 30.0];
        // Interpolation bleed: small, positive, and entirely plausible as tissue R2.
        let r2 = [10.0, 10.0, 0.6, 0.2];
        let (r2p, cov) = super::measured_r2prime(&r2star, &r2, &brain, Some(&measured)).unwrap();
        assert_eq!(&r2p[2..], &[0.0, 0.0],
                   "29.4 and 29.8 here would be nearly the whole of R2* called reversible");
        assert_eq!(cov, vec![1, 1, 0, 0]);
    }

    /// With R2 from a custom derivative there is no FOV to narrow by, so the brain mask stands,
    /// bar any voxel whose R2 is zero: T2 = infinity is not a tissue property, so a zero there is
    /// a gap in the map rather than a measured value to subtract.
    #[test]
    fn a_custom_r2_map_is_subtracted_over_the_whole_brain() {
        let brain = [1u8, 1, 0, 1];
        let (r2p, cov) = super::measured_r2prime(
            &[30.0, 30.0, 30.0, 30.0], &[10.0, 10.0, 10.0, 0.0], &brain, None,
        ).unwrap();
        assert_eq!(r2p, vec![20.0, 20.0, 0.0, 0.0]);
        assert_eq!(cov, vec![1, 1, 0, 0],
                   "a zero R2 inside the brain mask is an absent measurement, not R2' = R2*");
    }

    /// R2PRIMEnet's 3 T band: everything sold as "3 T" is inside it, 1.5 T and 7 T are not.
    ///
    /// The warning this drives is the only thing standing between a 7 T user and an R2' map that
    /// is the right shape, the right units and the wrong numbers.
    #[test]
    fn r2primenet_field_band_covers_3t_and_excludes_7t() {
        for inside in [3.0, 2.89, 2.8, 3.2] {
            assert!(super::r2primenet_field_offset(inside).is_none(),
                    "{inside} T is a 3 T acquisition");
        }
        for outside in [1.5, 7.0, 9.4, 0.55] {
            assert!(super::r2primenet_field_offset(outside).is_some(),
                    "{outside} T is not what R2PRIMEnet was trained on");
        }
    }

    /// qsmxt-config writes R2PRIMEnet's defaults out (it builds without qsm-core's `onnx`); they
    /// must stay qsm-core's.
    #[cfg(feature = "dl")]
    #[test]
    fn r2primenet_config_defaults_are_qsm_cores() {
        let ours = super::r2primenet_params(&Default::default()).unwrap();
        let core = qsm_core::relaxometry::R2PrimeNetParams::default();
        assert_eq!(ours.patch, core.patch);
        assert_eq!(ours.ignore_field_mismatch, core.ignore_field_mismatch);
        assert_eq!(super::R2PRIMENET_TRAINED_B0_T, qsm_core::relaxometry::TRAINED_B0_T);
        assert_eq!(super::R2PRIMENET_B0_TOLERANCE_T, qsm_core::relaxometry::B0_TOLERANCE_T);
    }

    #[test]
    fn robust_fit_report_sits_next_to_the_field_map() {
        let p = std::path::Path::new("/w/unwrap/sub-1_field-ppm.nii");
        assert_eq!(super::robust_fit_report_path(p), std::path::PathBuf::from("/w/unwrap/sub-1_field-ppm_robustfit.json"));
        let gz = std::path::Path::new("/w/unwrap/sub-1_field-ppm.nii.gz");
        assert_eq!(super::robust_fit_report_path(gz), std::path::PathBuf::from("/w/unwrap/sub-1_field-ppm_robustfit.json"));
    }

    /// Off 3 T: `auto` skips, an explicit `r2primenet` refuses, and the override runs either.
    #[test]
    fn r2primenet_field_policy_by_strategy() {
        use super::{r2primenet_field_policy as policy, R2primenetFieldPolicy as P};
        use crate::pipeline::config::R2PrimeStrategy as S;
        assert_eq!(policy(3.0, S::Auto, false), P::Run);
        assert_eq!(policy(3.0, S::R2primenet, false), P::Run);
        assert_eq!(policy(7.0, S::Auto, false), P::Skip);
        assert_eq!(policy(7.0, S::R2primenet, false), P::Refuse);
        assert_eq!(policy(7.0, S::Auto, true), P::RunUncalibrated);
        assert_eq!(policy(1.5, S::R2primenet, true), P::RunUncalibrated);
    }

    /// The backstop for the residual case the gate judges optimistically: a declared R2 source
    /// that never materialises leaves R2PRIMEnet running on an unresampled grid, and this is the
    /// only thing that says so.
    #[test]
    fn r2primenet_warns_only_when_the_grid_was_left_oblique() {
        let oblique = oblique_affine();
        assert!(super::r2primenet_grid_tilt(false, &oblique).is_some_and(|t| t > 1.0),
                "an unresampled oblique grid is exactly the case worth warning about");
        assert!(super::r2primenet_grid_tilt(true, &oblique).is_none(),
                "a run resampled to a cardinal grid has nothing wrong with it");
        let axial = [
            0.8, 0.0, 0.0, 0.0, 0.0, 0.8, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        assert!(super::r2primenet_grid_tilt(false, &axial).is_none(),
                "an axial acquisition needs no warning");
    }

    #[test]
    fn same_grid_tolerates_header_precision_but_not_a_real_difference() {
        let a = diag_affine(1.0, 0.0);
        let mut rounded = a;
        rounded[3] = 1e-5; // what a 32-bit header field can cost
        assert!(super::same_grid((4, 4, 4), &a, (4, 4, 4), &rounded));
        assert!(!super::same_grid((4, 4, 4), &a, (4, 4, 4), &diag_affine(1.0, 0.5)),
                "half a millimetre of offset is a real difference");
        assert!(!super::same_grid((4, 4, 4), &a, (4, 4, 5), &a), "dims count too");
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
        cfg.pipeline.obliquity_threshold = 10.0;
        let mut run = run_for_geometry(None);
        run.mese = Some(mese_on((16, 16, 8), oblique_affine()));
        let meta = super::resolve_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
        assert!(meta.source_geometry.is_some(),
                "a MESE no longer suppresses a resample the threshold asked for");
        assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
    }

    // --- which box the FFT stages reconstruct in ---

    /// The grid a 32.5-degree oblique UK Biobank SWI resamples to. Awkward on every axis:
    /// 272 = 2^4 * 17, 339 = 3 * 113, 77 = 7 * 11.
    const UKB_RESAMPLED: (usize, usize, usize) = (272, 339, 77);

    #[test]
    fn reconstruction_box_pads_to_a_friendly_size_when_asked() {
        let mask = vec![1u8; 8];
        let b = super::reconstruction_box(false, true, &mask, UKB_RESAMPLED, (0.8, 0.8, 3.0), 32.0);
        assert_eq!(b.dims, (280, 343, 80), "every awkward axis should grow");
        assert!(b.dims.0 >= UKB_RESAMPLED.0 && b.dims.1 >= UKB_RESAMPLED.1 && b.dims.2 >= UKB_RESAMPLED.2,
                "padding must never discard a voxel");
    }

    #[test]
    fn reconstruction_box_leaves_an_already_friendly_grid_alone() {
        let mask = vec![1u8; 8];
        let friendly = (256, 288, 48); // 2^8, 2^5*3^2, 2^4*3
        let b = super::reconstruction_box(false, true, &mask, friendly, (0.8, 0.8, 3.0), 32.0);
        assert_eq!(b.dims, friendly, "nothing to gain, so no copy");
        assert_eq!(b.origin, (0, 0, 0));
    }

    #[test]
    fn reconstruction_box_does_not_pad_by_default() {
        let mask = vec![1u8; 8];
        let b = super::reconstruction_box(false, false, &mask, UKB_RESAMPLED, (0.8, 0.8, 3.0), 32.0);
        assert_eq!(b.dims, UKB_RESAMPLED,
                   "padding changes where k-space is sampled, so it must be asked for");
        assert_eq!(b.origin, (0, 0, 0));
    }

    #[test]
    fn reconstruction_box_crops_to_the_mask_when_asked() {
        // A small blob in the middle of a large grid: the case cropping is actually for.
        let dims = (64, 64, 64);
        let mut mask = vec![0u8; dims.0 * dims.1 * dims.2];
        for k in 30..34 {
            for j in 30..34 {
                for i in 30..34 {
                    mask[i + j * dims.0 + k * dims.0 * dims.1] = 1;
                }
            }
        }
        let cropped = super::reconstruction_box(true, false, &mask, dims, (1.0, 1.0, 1.0), 4.0);
        assert!(cropped.dims.0 < dims.0, "a 4-voxel blob with a 4 mm margin should shrink 64");
        let padded = super::reconstruction_box(false, true, &mask, dims, (1.0, 1.0, 1.0), 4.0);
        assert_eq!(padded.dims, dims, "padding ignores the mask; 64 is already friendly");
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
        let meta = super::resolve_geometry(&run_for_geometry(None), &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
        assert!(meta.source_geometry.is_some(),
                "a network that assumes B0 is +z must be given axial data, threshold or not");
        assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
    }

    #[test]
    fn geometry_axial_only_algorithm_outranks_a_sidecar_b0() {
        let cfg = cfg_with_axial_only_inversion();
        let run = run_for_geometry(Some((0.0, 0.5, 0.866)));
        let meta = super::resolve_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
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
        let meta = super::resolve_geometry(&run_for_geometry(None), &nifti_with(affine, (4, 4, 4)), &cfg).unwrap();
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
        let meta = super::resolve_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg)
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
        let meta = super::resolve_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg)
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
        qsmxt_config::config::enforce_separation_dependencies(&mut cfg);
        // No MESE, so `--r2prime-strategy auto` predicts R2′ with R2PRIMEnet.
        let run = run_for_geometry(None);
        let meta = super::resolve_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
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
        qsmxt_config::config::enforce_separation_dependencies(&mut cfg);
        let mut run = run_for_geometry(None);
        run.mese = Some(mese_on((16, 16, 8), oblique_affine()));
        let meta = super::resolve_geometry(&run, &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
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
        let meta = super::resolve_geometry(&run_for_geometry(None), &nifti_with(oblique_affine(), (16, 16, 8)), &cfg).unwrap();
        assert!(meta.source_geometry.is_some(),
                "iQFM learned the same axial prior as iQSM and takes no B0 direction");
        assert_eq!(meta.b0_direction, (0.0, 0.0, 1.0));
    }

    #[test]
    fn geometry_classical_algorithm_needs_no_resampling() {
        let cfg = crate::pipeline::config::PipelineConfig::default(); // iLSQR by default
        assert!(qsmxt_config::bridge::axial_only_algorithms(&cfg, Default::default()).is_empty(),
                "the default pipeline takes B0 as a parameter throughout");
    }

    fn meta_4x4x4() -> crate::pipeline::graph::RunMetadata {
        crate::pipeline::graph::RunMetadata {
            dims: (4, 4, 4), voxel_size: (1.0, 1.0, 1.0),
            affine: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            n_echoes: 1, echo_times: vec![0.004], b0_direction: (0.0, 0.0, 1.0),
            field_strength: 3.0, has_magnitude: true,
            source_geometry: None, slice_geometry: None,
        }
    }

    #[test]
    fn test_homogeneity_correct_rejects_size_mismatch() {
        // The guard must report the mismatch rather than let qsm-core index past the volume.
        let cfg = crate::pipeline::config::HomogeneityConfig::default();
        let short = vec![1.0f64; 4 * 4 * 3];
        let err = super::homogeneity_correct(&short, &meta_4x4x4(), &cfg, "the magnitude").unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("48 voxels"), "got: {}", msg);
        assert!(msg.contains("4x4x4"), "got: {}", msg);
        assert!(msg.contains("--no-inhomogeneity-correction"), "got: {}", msg);
    }

    #[test]
    fn test_homogeneity_correct_runs_on_matching_volume() {
        let cfg = crate::pipeline::config::HomogeneityConfig::default();
        let data: Vec<f64> = (0..4 * 4 * 4).map(|i| 100.0 + i as f64).collect();
        let out = super::homogeneity_correct(&data, &meta_4x4x4(), &cfg, "the magnitude").unwrap();
        assert_eq!(out.len(), data.len());
        assert!(out.iter().all(|v| v.is_finite()));
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

        let state_path = dir.path().join("state.json");
        let mut config = PipelineConfig::default();
        let mut state = PipelineState::load_or_create(&state_path, &config, &k, true);
        {
            let mut ctx = StageContext { run: &run, config: &config, output: &output, meta: &meta, state: &mut state, state_path: &state_path };
            assert_eq!(stage_motion(&mut ctx, &mask_path, &|_| {}).unwrap(), mask_path, "off: the mask is unchanged");
            assert_eq!(ctx.echo_mag_path(2), output.mag_path(&k, 2), "off: the echoes are the scaled ones");
        }

        config.motion_correction.enabled = true;
        let mut ctx = StageContext { run: &run, config: &config, output: &output, meta: &meta, state: &mut state, state_path: &state_path };
        let shrunk = stage_motion(&mut ctx, &mask_path, &|_| {}).unwrap();
        assert_eq!(shrunk, output.motion_coverage_path(&k));
        assert_eq!(ctx.echo_mag_path(2), output.motion_mag_path(&k, 2));
        assert_eq!(ctx.echo_phase_path(1), output.motion_phase_path(&k, 1));
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

        let mut no_mag = meta.clone();
        no_mag.has_magnitude = false;
        let mut ctx = StageContext { run: &run, config: &config, output: &output, meta: &no_mag, state: &mut state, state_path: &state_path };
        let e = stage_motion(&mut ctx, &mask_path, &|_| {}).unwrap_err().to_string();
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

    /// Run `stage_swi` end to end over synthetic inputs on the given grid.
    fn run_swi_stage(
        dir: &Path,
        dims: (usize, usize, usize),
        affine: [f64; 16],
        window: usize,
    ) -> (crate::Result<()>, DerivativeOutputs, crate::bids::entities::AcquisitionKey) {
        let (nx, ny, nz) = dims;
        let k = key();
        let run = run_for(k.clone());
        let output = DerivativeOutputs::new(dir);
        let meta = meta_for(dims, affine);

        let mut config = PipelineConfig::default();
        config.swi.mip_window = window;

        let mask_path = output.mask_path(&k);
        write_volume(&output.phase_scaled_path(&k, 1), &textured(dims), dims, meta.voxel_size, &affine).unwrap();
        write_volume(&output.magnitude_path(&k), &textured(dims), dims, meta.voxel_size, &affine).unwrap();
        write_volume(&mask_path, &vec![1.0; nx * ny * nz], dims, meta.voxel_size, &affine).unwrap();

        let state_path = dir.join("state.json");
        let mut state = PipelineState::load_or_create(&state_path, &config, &k, true);
        let mut ctx = StageContext {
            run: &run, config: &config, output: &output, meta: &meta,
            state: &mut state, state_path: &state_path,
        };
        let result = stage_swi(&mut ctx, &mask_path, &|_| {});
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

    /// A minIP written with full-volume dimensions must not survive as a cache hit.
    #[test]
    fn a_stale_minip_cache_is_rebuilt() {
        let dir = tempfile::tempdir().unwrap();
        let dims = (8, 8, 16);
        let (result, _, _) = run_swi_stage(dir.path(), dims, IDENTITY, 7);
        result.unwrap();

        // Replay the pre-fix parameter set: same window, no minIP dimensions recorded.
        let stale = serde_json::json!({
            "scaling": PipelineConfig::default().swi.scaling,
            "strength": PipelineConfig::default().swi.strength,
            "hp_sigma": PipelineConfig::default().swi.hp_sigma,
            "mip_window": 7,
        });
        let stale_hash = crate::pipeline::graph::step_params_hash(Some("clear-swi"), &stale);
        let state_path = dir.path().join("state.json");
        let mut state: PipelineState =
            serde_json::from_str(&std::fs::read_to_string(&state_path).unwrap()).unwrap();
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

    fn resampling_config() -> PipelineConfig {
        let mut config = PipelineConfig::default();
        config.pipeline.obliquity_threshold = 5.0;
        config
    }

    fn voxels(path: &Path) -> usize {
        io::read_nifti_file(path).unwrap().data.len()
    }

    const ACQUIRED: usize = DIMS.0 * DIMS.1 * DIMS.2;

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
        let mut config = PipelineConfig::default();
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
        let config = PipelineConfig::default();
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

    /// End to end with V-SHARP: the published χ is 0 on the rim background removal eroded, its
    /// mean over the published support is the reference zero, and the support is smaller than
    /// the brain mask (or the test would prove nothing).
    #[test]
    fn the_published_map_is_zero_outside_its_published_support() {
        let dir = tempfile::tempdir().unwrap();
        let run = oblique_run(dir.path(), "a", 3);
        let output = DerivativeOutputs::new(&dir.path().join("out"));
        run_pipeline_cached(&run, &PipelineConfig::default(), &output, false, &|_| {}).unwrap();

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
        run_pipeline_cached(&run, &PipelineConfig::default(), &output, false, &|_| {}).unwrap();
        assert!(!resampled(&output));
        run_pipeline_cached(&run, &resampling_config(), &output, false, &|_| {}).unwrap();
        assert!(resampled(&output));
        run_pipeline_cached(&run, &PipelineConfig::default(), &output, false, &|_| {}).unwrap();
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
        for contrast in ["paramagnetic", "diamagnetic"] {
            let mip = io::read_nifti_file(&output.smwi_mip_path(&run.key, contrast)).unwrap();
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
        assert!(negates_real_imaginary_phase(Some("GE")));
        assert!(negates_real_imaginary_phase(Some("GE MEDICAL SYSTEMS")));
        assert!(negates_real_imaginary_phase(Some("General Electric")));
        assert!(!negates_real_imaginary_phase(Some("Siemens")));
        assert!(!negates_real_imaginary_phase(Some("Philips")));
        assert!(!negates_real_imaginary_phase(Some("GEMINI")));
        assert!(!negates_real_imaginary_phase(None));

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
    fn reference(f: &Fixture, config: &PipelineConfig) -> Vec<f64> {
        let m = meta();
        let state_path = f.output.state_path(&f.run.key);
        let mut state = PipelineState::load_or_create(&state_path, config, &f.run.key, true);
        let mut ctx = StageContext {
            run: &f.run, config, output: &f.output, meta: &m,
            state: &mut state, state_path: &state_path,
        };
        let pass = Pass::main(&f.output, &f.run.key, &f.output.mask_path(&f.run.key));
        stage_reference(
            &mut ctx, &[pass.support(false)], &pass.chi_raw, f.output.qsm_path(&f.run.key),
            Some(f.output.qsm_support_path(&f.run.key)), "reference", &|_| {},
        ).unwrap();
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
        let ids = qsmxt_config::regions::resolve("left-thalamus", config.segmentation.version).unwrap();

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
        let raw2_path = f.output.reliable_chi_raw_path(&f.run.key);
        save_volume(&raw2_path, &raw2, &m).unwrap();
        let state_path = f.output.state_path(&f.run.key);
        let mut state = PipelineState::load_or_create(&state_path, &config, &f.run.key, true);
        let mut ctx = StageContext {
            run: &f.run, config: &config, output: &f.output, meta: &m,
            state: &mut state, state_path: &state_path,
        };
        let pass = Pass::main(&f.output, &f.run.key, &f.output.mask_path(&f.run.key));
        stage_reference(
            &mut ctx, &[pass.support(false)], &raw2_path, f.output.singlepass_qsm_path(&f.run.key),
            None, "reference-singlepass", &|_| {},
        ).unwrap();
        // χ-separation maps, which are never referenced.
        for p in [f.output.chi_para_path(&f.run.key), f.output.chi_dia_path(&f.run.key),
                  f.output.chi_sep_total_path(&f.run.key)] {
            save_volume(&p, &f.chi_raw, &m).unwrap();
        }
        let dseg: Vec<f64> = f.brain.iter().map(|&b| if b != 0 { 10.0 } else { 0.0 }).collect();
        let dseg_path = f.output.dseg_path(&f.run.key);
        save_volume(&dseg_path, &dseg, &m).unwrap();
        stage_analysis(&mut ctx, &dseg_path, &|_| {}).unwrap();

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

        let state_path = f.output.state_path(k);
        let mut state = PipelineState::load_or_create(&state_path, config, k, true);
        let mut ctx = StageContext {
            run: &f.run, config, output: &f.output, meta: &m,
            state: &mut state, state_path: &state_path,
        };
        let pass = Pass::main(&f.output, k, &f.output.mask_path(k));
        stage_reference(
            &mut ctx, &[pass.support(false)], &pass.chi_raw, f.output.qsm_path(k),
            Some(f.output.qsm_support_path(k)), "reference", &|_| {},
        ).unwrap();
        stage_chi_separation(&mut ctx, &f.output.mask_path(k), &|_| {}).unwrap();
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
            m.dims, m.voxel_size, &m.echo_times, m.field_strength, m.b0_direction);
        let r = qsm_core::pipeline::run_separation(
            inputs, &metadata, &qsmxt_config::bridge::to_separation_config(config), &mut |_, _| {},
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

    #[test]
    fn separation_support_rules() {
        let brain = [1u8, 1, 1, 1, 0];
        let chi = [0.1, 0.0, 0.2, 0.3, 0.4];
        // The published support wins over χ's own zeros (a referenced voxel can land on 0.0), and
        // never reaches past the brain mask.
        assert_eq!(separation_support(&brain, Some(&[1, 1, 0, 1, 1]), &chi, None), vec![1, 1, 0, 1, 0]);
        // No published support (bring-your-own χ): the brain mask less χ's undefined voxels.
        assert_eq!(separation_support(&brain, None, &chi, None), vec![1, 0, 1, 1, 0]);
        // The field-based methods also need the local field.
        let field = [0.01, 0.01, 0.01, 0.0, 0.01];
        assert_eq!(separation_support(&brain, Some(&[1, 1, 1, 1, 0]), &chi, Some(&field)), vec![1, 1, 1, 0, 0]);
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
