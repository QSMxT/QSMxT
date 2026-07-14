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
use crate::pipeline::memory;
use crate::pipeline::phase;
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
        let hash = crate::pipeline::graph::step_params_hash(algorithm, params);
        self.state.is_step_cached_with_hash(step, Some(&hash))
    }

    fn is_cached(&mut self, step: &str) -> bool {
        self.state.is_step_cached(step)
    }

    fn dims(&self) -> (usize, usize, usize) { self.meta.dims }
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
        // Write provenance.json
        let output_refs: Vec<String> = output_paths.iter().map(|p| p.display().to_string()).collect();
        let prov = Provenance {
            step: step.to_string(),
            algorithm: algorithm.map(|s| s.to_string()),
            parameters: parameters.clone(),
            inputs: inputs.iter().map(|p| p.display().to_string()).collect(),
            outputs: output_refs,
            duration_secs: start.elapsed().as_secs_f64(),
            peak_memory_bytes: memory::process_rss_bytes(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        let prov_path = self.output.provenance_path(&self.run.key, step);
        if let Some(parent) = prov_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&prov)
            .map_err(|e| QsmxtError::Config(format!("Failed to serialize provenance: {}", e)))?;
        std::fs::write(&prov_path, json)?;

        // Mark step done with params hash
        let hash = crate::pipeline::graph::step_params_hash(algorithm, &parameters);
        self.state.mark_completed(step, output_paths, Some(hash));
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
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    io::save_nifti_to_file(path, data, meta.dims, meta.voxel_size, &meta.affine)
        .map_err(QsmxtError::NiftiIo)
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
    clean_intermediates: bool,
    progress: &dyn Fn(&str),
) -> crate::Result<()> {
    let state_path = output.state_path(&qsm_run.key);
    let mut state = PipelineState::load_or_create(&state_path, config, &qsm_run.key, force);

    let meta = stage_load(qsm_run, &mut state, &state_path, progress)?;

    let needs_mask = config.pipeline.do_qsm || config.pipeline.do_swi
        || (config.pipeline.do_t2starmap && meta.n_echoes >= 3 && meta.has_magnitude)
        || (config.pipeline.do_r2starmap && meta.n_echoes >= 3 && meta.has_magnitude);
    let needs_phase = needs_mask || config.pipeline.do_qsm;

    if !needs_phase {
        log::info!("No outputs enabled — nothing to process");
        state.mark_run_complete();
        state.save(&state_path)?;
        return Ok(());
    }

    let mut ctx = StageContext {
        run: qsm_run, config, output, meta: &meta,
        state: &mut state, state_path: &state_path,
    };

    stage_scale_phase(&mut ctx, progress)?;
    stage_magnitude(&mut ctx, progress)?;

    let mask_path = output.mask_path(&qsm_run.key);
    if needs_mask {
        stage_mask(&mut ctx, &mask_path, progress)?;
    }

    if config.pipeline.do_swi && meta.has_magnitude {
        stage_swi(&mut ctx, &mask_path, progress)?;
    }

    if (config.pipeline.do_t2starmap || config.pipeline.do_r2starmap) && meta.n_echoes >= 3 && meta.has_magnitude {
        stage_t2star_r2star(&mut ctx, &mask_path, progress)?;
    }

    if !config.pipeline.do_qsm {
        log::info!("QSM processing disabled — skipping reconstruction");
    }

    if config.pipeline.do_qsm {
        let field_path = output.field_ppm_path(&qsm_run.key);
        let need_field = !matches!(config.inversion.algorithm, QsmAlgorithm::Tgv if meta.n_echoes == 1);

        if need_field {
            stage_unwrap(&mut ctx, &mask_path, &field_path, progress)?;
        }

        match config.inversion.algorithm {
            QsmAlgorithm::Tgv => stage_tgv(&mut ctx, &mask_path, &field_path, progress)?,
            QsmAlgorithm::Qsmart => stage_qsmart(&mut ctx, &mask_path, &field_path, progress)?,
            _ => stage_standard_qsm(&mut ctx, &mask_path, &field_path, progress)?,
        }

        stage_reference(&mut ctx, &mask_path, progress)?;
    }

    ctx.state.mark_run_complete();
    ctx.state.save(&state_path)?;

    if clean_intermediates {
        crate::pipeline::graph::clean_intermediates(ctx.state, &output.output_dir, &qsm_run.key);
    }

    Ok(())
}

// ─── Stage functions ───

fn stage_load(
    qsm_run: &QsmRun,
    state: &mut PipelineState,
    state_path: &Path,
    progress: &dyn Fn(&str),
) -> crate::Result<RunMetadata> {
    if !state.is_step_cached("load") {
        let t = Instant::now();
        progress("Loading NIfTI metadata");
        let first_phase = io::read_nifti_file(&qsm_run.echoes[0].phase_nifti)
            .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", qsm_run.echoes[0].phase_nifti.display(), e)))?;

        let meta = RunMetadata {
            dims: first_phase.dims,
            voxel_size: first_phase.voxel_size,
            affine: first_phase.affine,
            n_echoes: qsm_run.echoes.len(),
            echo_times: qsm_run.echo_times.clone(),
            b0_direction: qsm_run.b0_dir,
            field_strength: qsm_run.magnetic_field_strength,
            has_magnitude: qsm_run.has_magnitude,
        };
        log::info!(
            "Volume: {}x{}x{}, {:.1}mm iso, {} echoes, B0={:.1}T, TEs={:?}s",
            meta.dims.0, meta.dims.1, meta.dims.2,
            meta.voxel_size.0, meta.n_echoes, meta.field_strength, meta.echo_times,
        );
        state.run_metadata = Some(meta.clone());
        state.mark_completed("load", vec![], None);
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

fn stage_scale_phase(ctx: &mut StageContext, progress: &dyn Fn(&str)) -> crate::Result<()> {
    if ctx.is_cached("scale_phase") {
        log::info!("Skipping scale_phase (cached)");
        return Ok(());
    }
    let t = Instant::now();
    progress("Rescaling phase to radians");
    let mut phase_paths = Vec::new();
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
    let mut mag_paths = Vec::new();
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

    let mut all_paths = phase_paths;
    all_paths.extend(mag_paths.clone());
    let input_paths: Vec<PathBuf> = ctx.run.echoes.iter().map(|e| e.phase_nifti.clone()).collect();
    let input_refs: Vec<&Path> = input_paths.iter().map(|p| p.as_path()).collect();
    ctx.complete_step("scale_phase", None, serde_json::json!({}), &input_refs, all_paths, t)?;
    log_step_done("Rescale phase", t);
    Ok(())
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
        let mut combined = phase::rss_combine(&refs);

        // Apply homogeneity correction to the combined result
        if ctx.config.masking.inhomogeneity_correction {
            progress("Applying inhomogeneity correction");
            let (nx, ny, nz) = ctx.meta.dims;
            let (vsx, vsy, vsz) = ctx.meta.voxel_size;
            let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
            combined = qsm_core::utils::makehomogeneous(
                &combined, &grid, ctx.config.homogeneity.sigma_mm, ctx.config.homogeneity.nbox,
            );
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

/// Locate a bring-your-own mask under `<bids>/derivatives/<tool>/sub-*/[ses-*/]anat/*_mask.nii*`
/// for this run. `tool == "*"` searches every derivatives dir alphabetically; within a tool the
/// first matching mask (alphabetical) wins. Returns None if nothing matches (caller falls back).
fn find_custom_mask(run: &QsmRun, tool: &str) -> Option<PathBuf> {
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
        let pattern = format!("{}/*_mask.nii*", anat_dir.display());
        if let Ok(paths) = glob(&pattern) {
            let mut hits: Vec<PathBuf> = paths.filter_map(|r| r.ok()).collect();
            hits.sort();
            if let Some(p) = hits.into_iter().next() {
                return Some(p);
            }
        }
    }
    None
}

fn stage_mask(ctx: &mut StageContext, mask_path: &Path, progress: &dyn Fn(&str)) -> crate::Result<()> {
    let mask_params = serde_json::json!({
        "sections": ctx.config.masking.sections.iter().map(|s| format!("{}", s)).collect::<Vec<_>>(),
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

    log::info!("Creating mask ({} section(s))", ctx.config.masking.sections.len());

    // Load phases (needed for PhaseQuality masking input)
    let mut phases: Vec<Vec<f64>> = Vec::new();
    for i in 0..ctx.meta.n_echoes {
        let p_path = ctx.output.phase_scaled_path(&ctx.run.key, i + 1);
        if p_path.exists() {
            phases.push(load_volume(&p_path)?);
        }
    }

    // Resolve magnitude data
    let magnitude = resolve_mask_magnitude(ctx)?;
    let mag_data: Option<Vec<f64>> = magnitude.first().map(|m| m.data.clone());

    // Convert config masking sections to qsm-core types
    let core_sections = crate::pipeline::config::to_mask_sections(&ctx.config.masking.sections);
    let scan_meta = crate::pipeline::config::to_scan_metadata(
        ctx.meta.dims, ctx.meta.voxel_size, &ctx.meta.echo_times,
        ctx.meta.field_strength, ctx.meta.b0_direction,
    );
    let phase_refs: Vec<&[f64]> = phases.iter().map(|p| p.as_slice()).collect();

    let working_mask = qsm_core::pipeline::run_masking(
        &core_sections, &phase_refs, mag_data.as_deref(), &scan_meta,
    ).map_err(|e| QsmxtError::Config(format!("masking: {}", e)))?;
    save_mask(mask_path, &working_mask, ctx.meta)?;
    let mag_path = ctx.output.magnitude_path(&ctx.run.key);
    ctx.complete_step("mask", None, mask_params, &[mag_path.as_path()], vec![mask_path.to_path_buf()], t)?;
    log_step_done("Mask creation", t);
    Ok(())
}

/// Load magnitude data for masking: returns a single-element Vec containing
/// the appropriate magnitude volume based on what the mask sections require.
fn resolve_mask_magnitude(ctx: &StageContext) -> crate::Result<Vec<NiftiData>> {
    use crate::pipeline::config::MaskingInput;
    let (nx, ny, nz) = ctx.dims();
    let (vsx, vsy, vsz) = ctx.voxel_size();

    // Determine which masking inputs are needed
    let inputs: Vec<MaskingInput> = ctx.config.masking.sections.iter()
        .map(|s| s.input)
        .collect();

    // For MagnitudeFirst or MagnitudeLast, load the specific echo from source
    // and apply homogeneity correction if enabled. For Magnitude (RSS) and
    // PhaseQuality, use the pre-computed combined magnitude.
    let needs_first = inputs.iter().any(|i| matches!(i, MaskingInput::MagnitudeFirst));
    let needs_last = inputs.iter().any(|i| matches!(i, MaskingInput::MagnitudeLast));

    if needs_first || needs_last {
        let echo_idx = if needs_first { 0 } else { ctx.run.echoes.len() - 1 };
        if let Some(ref src) = ctx.run.echoes[echo_idx].magnitude_nifti {
            let nifti = io::read_nifti_file(src)
                .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", src.display(), e)))?;
            let data = if ctx.config.masking.inhomogeneity_correction {
                let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
                qsm_core::utils::makehomogeneous(
                    &nifti.data, &grid, ctx.config.homogeneity.sigma_mm, ctx.config.homogeneity.nbox,
                )
            } else {
                nifti.data
            };
            return Ok(vec![NiftiData { data, ..nifti }]);
        }
    }

    // Default: load the pre-computed RSS-combined magnitude
    let combined_path = ctx.output.magnitude_path(&ctx.run.key);
    if combined_path.exists() {
        let m = io::read_nifti_file(&combined_path)
            .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", combined_path.display(), e)))?;
        Ok(vec![m])
    } else {
        Ok(Vec::new())
    }
}

fn stage_swi(ctx: &mut StageContext, mask_path: &Path, progress: &dyn Fn(&str)) -> crate::Result<()> {
    let swi_params = serde_json::json!({
        "scaling": ctx.config.swi.scaling,
        "strength": ctx.config.swi.strength,
        "hp_sigma": ctx.config.swi.hp_sigma,
        "mip_window": ctx.config.swi.mip_window,
    });
    if ctx.is_cached_with_params("swi", Some("clear-swi"), &swi_params) {
        log::info!("Skipping swi (cached)");
        return Ok(());
    }
    let t = Instant::now();
    let (nx, ny, nz) = ctx.dims();
    let (vsx, vsy, vsz) = ctx.voxel_size();
    log::info!("Computing SWI (Laplacian unwrap + CLEAR-SWI + MIP)");
    progress("Computing SWI");
    let phase_data = load_volume(&ctx.output.phase_scaled_path(&ctx.run.key, 1))?;
    let mag_data = load_volume(&ctx.output.magnitude_path(&ctx.run.key))?;
    let mask = load_mask(mask_path)?;

    let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
    let unwrapped = qsm_core::unwrap::laplacian_unwrap(&phase_data, &mask, &grid);
    let swi_scaling = match ctx.config.swi.scaling.as_str() {
        "negative_tanh" => qsm_core::swi::PhaseScaling::NegativeTanh,
        "positive" => qsm_core::swi::PhaseScaling::Positive,
        "negative" => qsm_core::swi::PhaseScaling::Negative,
        "triangular" => qsm_core::swi::PhaseScaling::Triangular,
        _ => qsm_core::swi::PhaseScaling::Tanh,
    };
    let swi_params_core = qsm_core::swi::SwiParams {
        hp_sigma: ctx.config.swi.hp_sigma, scaling: swi_scaling,
        strength: ctx.config.swi.strength, mip_window: ctx.config.swi.mip_window,
    };
    let swi = qsm_core::swi::calculate_swi(
        &unwrapped, &mag_data, &mask, &grid, &swi_params_core,
    );
    let mip = qsm_core::swi::create_mip(&swi, &grid, ctx.config.swi.mip_window);

    let swi_path = ctx.output.swi_path(&ctx.run.key);
    let mip_path = ctx.output.swi_mip_path(&ctx.run.key);
    save_volume(&swi_path, &swi, ctx.meta)?;
    save_volume(&mip_path, &mip, ctx.meta)?;
    let phase_path = ctx.output.phase_scaled_path(&ctx.run.key, 1);
    let mag_input = ctx.output.magnitude_path(&ctx.run.key);
    ctx.complete_step("swi", Some("clear-swi"), swi_params, &[phase_path.as_path(), mag_input.as_path(), mask_path], vec![swi_path, mip_path], t)?;
    log_step_done("SWI", t);
    Ok(())
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
        let mag_data = if let Some(ref raw_path) = ctx.run.echoes[i].magnitude_nifti {
            let nifti = io::read_nifti_file(raw_path)
                .map_err(|e| QsmxtError::NiftiIo(format!("mag echo {}: {}", i + 1, e)))?;
            nifti.data
        } else {
            load_volume(&ctx.output.mag_path(&ctx.run.key, i + 1))?
        };
        for vox in 0..n_voxels {
            interleaved[vox * ctx.meta.n_echoes + i] = mag_data[vox];
        }
    }

    let (vsx, vsy, vsz) = ctx.voxel_size();
    let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
    let (r2star_map, _s0) = qsm_core::utils::r2star_arlo(
        &interleaved, &mask, &ctx.meta.echo_times, &grid,
    );
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
        let p = io::read_nifti_file(&ctx.output.phase_scaled_path(&ctx.run.key, i + 1))
            .map_err(|e| QsmxtError::NiftiIo(format!("echo {}: {}", i + 1, e)))?;
        phases.push(p);
    }
    let mask = load_mask(mask_path)?;

    // Load per-echo magnitudes
    let mut magnitudes: Vec<Vec<f64>> = Vec::new();
    for i in 0..ctx.meta.n_echoes {
        let m_path = ctx.output.mag_path(&ctx.run.key, i + 1);
        if m_path.exists() {
            magnitudes.push(load_volume(&m_path)?);
        }
    }

    // Build inputs for shared pipeline stage
    let phase_slices: Vec<&[f64]> = phases.iter().map(|p| p.data.as_slice()).collect();
    let mag_slices: Vec<&[f64]> = magnitudes.iter().map(|m| m.as_slice()).collect();
    let mag_option: Option<&[&[f64]]> = if mag_slices.is_empty() { None } else { Some(&mag_slices) };

    let (fm_config, _, _, _) = crate::pipeline::config::to_pipeline_stages(ctx.config);
    let scan_meta = crate::pipeline::config::to_scan_metadata(
        ctx.meta.dims, ctx.meta.voxel_size, &ctx.meta.echo_times,
        ctx.meta.field_strength, ctx.meta.b0_direction,
    );

    let result = qsm_core::pipeline::run_field_mapping(
        &phase_slices, mag_option, &mask, &scan_meta, &fm_config,
        &mut |_, _| {},
    ).map_err(|e| QsmxtError::Config(format!("field mapping: {}", e)))?;

    let field_ppm = result.b0_field_ppm;

    save_volume(field_path, &field_ppm, ctx.meta)?;
    let phase_inputs: Vec<PathBuf> = (0..ctx.meta.n_echoes).map(|i| ctx.output.phase_scaled_path(&ctx.run.key, i + 1)).collect();
    let input_refs: Vec<&Path> = phase_inputs.iter().map(|p| p.as_path()).chain(std::iter::once(mask_path)).collect();
    ctx.complete_step("unwrap", Some(unwrap_alg), unwrap_params, &input_refs, vec![field_path.to_path_buf()], t)?;
    log_step_done("Phase unwrapping", t);
    Ok(())
}

fn stage_tgv(
    ctx: &mut StageContext, mask_path: &Path, field_path: &Path, progress: &dyn Fn(&str),
) -> crate::Result<()> {
    let chi_raw_path = ctx.output.chi_raw_path(&ctx.run.key);
    let tgv_params = serde_json::json!({
        "iterations": ctx.config.inversion.tgv.iterations,
        "alphas": [ctx.config.inversion.tgv.alpha1, ctx.config.inversion.tgv.alpha0],
        "erosions": ctx.config.inversion.tgv.erosions,
        "step_size": ctx.config.inversion.tgv.step_size,
        "tol": ctx.config.inversion.tgv.tol,
        "te_ms": ctx.meta.echo_times[0] * 1000.0,
        "field_strength": ctx.meta.field_strength,
    });
    if ctx.is_cached_with_params("tgv", Some("tgv"), &tgv_params) {
        log::info!("Skipping tgv (cached)");
        return Ok(());
    }
    let t = Instant::now();
    log::info!(
        "TGV-QSM (iterations={}, alphas=[{}, {}], erosions={}, TE={:.3}ms, B0={:.1}T)",
        ctx.config.inversion.tgv.iterations, ctx.config.inversion.tgv.alpha1, ctx.config.inversion.tgv.alpha0,
        ctx.config.inversion.tgv.erosions, ctx.meta.echo_times[0] * 1000.0, ctx.meta.field_strength,
    );
    progress("TGV-QSM reconstruction");
    let mask = load_mask(mask_path)?;
    let bdir = ctx.meta.b0_direction;

    let phase_data = if ctx.meta.n_echoes > 1 {
        load_volume(field_path)?
    } else {
        load_volume(&ctx.output.phase_scaled_path(&ctx.run.key, 1))?
    };

    let (nx, ny, nz) = ctx.meta.dims;
    let (vsx, vsy, vsz) = ctx.meta.voxel_size;
    let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
    let params = qsm_core::inversion::TgvParams {
        alpha0: ctx.config.inversion.tgv.alpha0 as f32,
        alpha1: ctx.config.inversion.tgv.alpha1 as f32,
        iterations: ctx.config.inversion.tgv.iterations,
        erosions: ctx.config.inversion.tgv.erosions,
        step_size: ctx.config.inversion.tgv.step_size as f32,
        tol: ctx.config.inversion.tgv.tol as f32,
        fieldstrength: ctx.meta.field_strength as f32,
        te: ctx.meta.echo_times[0] as f32,
    };
    let chi = qsm_core::inversion::tgv_qsm(
        &phase_data, &mask, &grid, &params, bdir, |_, _| {},
    );

    save_volume(&chi_raw_path, &chi, ctx.meta)?;
    ctx.complete_step("tgv", Some("tgv"), tgv_params, &[mask_path, field_path], vec![chi_raw_path], t)?;
    log_step_done("TGV-QSM", t);
    Ok(())
}

fn stage_qsmart(
    ctx: &mut StageContext, mask_path: &Path, field_path: &Path, progress: &dyn Fn(&str),
) -> crate::Result<()> {
    let chi_raw_path = ctx.output.chi_raw_path(&ctx.run.key);
    let qsmart_params = serde_json::json!({
        "inversion": format!("{}", ctx.config.inversion.qsmart.inversion),
        "sdf_spatial_radius": ctx.config.inversion.qsmart.sdf_spatial_radius,
        "vasc_sphere_radius": ctx.config.inversion.qsmart.vasc_sphere_radius,
        "ilsqr_tol": ctx.config.inversion.qsmart.ilsqr_tol,
        "ilsqr_max_iter": ctx.config.inversion.qsmart.ilsqr_max_iter,
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
    if ctx.is_cached_with_params("qsmart", Some("qsmart"), &qsmart_params) {
        log::info!("Skipping qsmart (cached)");
        return Ok(());
    }
    let t = Instant::now();
    log::info!(
        "QSMART (inversion={}, ilsqr tol={:.0e}, max_iter={}, vasc_radius={}, sdf_radius={})",
        ctx.config.inversion.qsmart.inversion,
        ctx.config.inversion.qsmart.ilsqr_tol, ctx.config.inversion.qsmart.ilsqr_max_iter,
        ctx.config.inversion.qsmart.vasc_sphere_radius, ctx.config.inversion.qsmart.sdf_spatial_radius,
    );
    progress("QSMART reconstruction");
    let field_ppm = load_volume(field_path)?;
    let mask = load_mask(mask_path)?;

    // Delegate the full two-stage QSMART reconstruction to qsm-core. The inner
    // dipole inversion (default iLSQR) is selected via config.inversion.qsmart.inversion.
    let (_, _, mut inv_config, _) = crate::pipeline::config::to_pipeline_stages(ctx.config);
    let scan_meta = crate::pipeline::config::to_scan_metadata(
        ctx.meta.dims, ctx.meta.voxel_size, &ctx.meta.echo_times,
        ctx.meta.field_strength, ctx.meta.b0_direction,
    );

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
    ctx.complete_step("qsmart", Some("qsmart"), qsmart_params, &[mask_path, field_path], vec![chi_raw_path], t)?;
    log_step_done("QSMART", t);
    Ok(())
}

fn stage_standard_qsm(
    ctx: &mut StageContext, mask_path: &Path, field_path: &Path, progress: &dyn Fn(&str),
) -> crate::Result<()> {
    // --- Background removal ---
    let skip_bgremove = ctx.config.inversion.algorithm == QsmAlgorithm::Medi && ctx.config.inversion.medi.smv;
    let local_field_path = ctx.output.local_field_path(&ctx.run.key);
    let bg_mask_path = ctx.output.bg_mask_path(&ctx.run.key);
    let bf_name = format!("{}", ctx.config.bg_removal.algorithm);
    let bf_params = match ctx.config.bg_removal.algorithm {
        BfAlgorithm::Vsharp => serde_json::json!({
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
            "radius": ctx.config.bg_removal.iharperella.radius,
            "max_iter": ctx.config.bg_removal.iharperella.max_iter,
            "tol": ctx.config.bg_removal.iharperella.tol,
        }),
    };
    if skip_bgremove {
        log::info!("Skipping background removal (MEDI SMV handles it internally)");
    }
    if !skip_bgremove && !ctx.is_cached_with_params("bgremove", Some(&bf_name), &bf_params) {
        let t = Instant::now();
        progress("Background field removal");
        let field_ppm = load_volume(field_path)?;
        let mask = load_mask(mask_path)?;

        let (_, bg_config, _, _) = crate::pipeline::config::to_pipeline_stages(ctx.config);
        let scan_meta = crate::pipeline::config::to_scan_metadata(
            ctx.meta.dims, ctx.meta.voxel_size, &ctx.meta.echo_times,
            ctx.meta.field_strength, ctx.meta.b0_direction,
        );

        log::info!("Background removal ({})", bf_name);
        let (mut prog, _) = iter_progress_bar(&ctx.run.key.to_string(), &bf_name);
        let bg_result = qsm_core::pipeline::run_bg_removal(
            &field_ppm, &mask, &scan_meta, &bg_config, &mut *prog,
        ).map_err(|e| QsmxtError::Config(format!("bg removal: {}", e)))?;

        let (local_field, eroded_mask) = (bg_result.local_field_ppm, bg_result.eroded_mask);
        save_volume(&local_field_path, &local_field, ctx.meta)?;
        save_mask(&bg_mask_path, &eroded_mask, ctx.meta)?;
        ctx.complete_step("bgremove", Some(&bf_name),
            bf_params.clone(), &[field_path, mask_path],
            vec![local_field_path.clone(), bg_mask_path.clone()], t,
        )?;
        log_step_done(&format!("Background removal ({})", bf_name), t);
    } else if !skip_bgremove {
        log::info!("Skipping bgremove (cached)");
    }

    // --- Dipole inversion ---
    let chi_raw_path = ctx.output.chi_raw_path(&ctx.run.key);
    let alg_name = format!("{}", ctx.config.inversion.algorithm);
    let invert_params = match ctx.config.inversion.algorithm {
        QsmAlgorithm::Rts => serde_json::json!({
            "delta": ctx.config.inversion.rts.delta, "mu": ctx.config.inversion.rts.mu,
            "tol": ctx.config.inversion.rts.tol, "max_iter": ctx.config.inversion.rts.max_iter,
        }),
        QsmAlgorithm::Tv => serde_json::json!({
            "lambda": ctx.config.inversion.tv.lambda, "max_iter": ctx.config.inversion.tv.max_iter,
        }),
        QsmAlgorithm::Tkd => serde_json::json!({ "threshold": ctx.config.inversion.tkd.threshold }),
        QsmAlgorithm::Tsvd => serde_json::json!({ "threshold": ctx.config.inversion.tsvd.threshold }),
        QsmAlgorithm::Ilsqr => serde_json::json!({
            "tol": ctx.config.inversion.ilsqr.tol, "max_iter": ctx.config.inversion.ilsqr.max_iter,
        }),
        QsmAlgorithm::Tikhonov => serde_json::json!({ "lambda": ctx.config.inversion.tikhonov.lambda }),
        QsmAlgorithm::Nltv => serde_json::json!({
            "lambda": ctx.config.inversion.nltv.lambda, "max_iter": ctx.config.inversion.nltv.max_iter,
        }),
        QsmAlgorithm::Medi => serde_json::json!({
            "lambda": ctx.config.inversion.medi.lambda, "max_iter": ctx.config.inversion.medi.max_iter,
            "smv": ctx.config.inversion.medi.smv,
        }),
        _ => serde_json::json!({}),
    };
    if !ctx.is_cached_with_params("invert", Some(&alg_name), &invert_params) {
        let t = Instant::now();
        progress("Dipole inversion");
        let local_field = if skip_bgremove { load_volume(field_path)? } else { load_volume(&local_field_path)? };
        let eroded_mask = if skip_bgremove { load_mask(mask_path)? } else { load_mask(&bg_mask_path)? };

        let (_, _, inv_config, _) = crate::pipeline::config::to_pipeline_stages(ctx.config);
        let scan_meta = crate::pipeline::config::to_scan_metadata(
            ctx.meta.dims, ctx.meta.voxel_size, &ctx.meta.echo_times,
            ctx.meta.field_strength, ctx.meta.b0_direction,
        );

        // Load combined magnitude for MEDI edge weighting
        let mag_combined_path = ctx.output.magnitude_path(&ctx.run.key);
        let magnitude: Option<Vec<f64>> = if mag_combined_path.exists() {
            Some(load_volume(&mag_combined_path)?)
        } else {
            None
        };

        log::info!("Dipole inversion ({})", alg_name);
        let (mut prog, _) = iter_progress_bar(&ctx.run.key.to_string(), &alg_name);
        let chi = qsm_core::pipeline::run_dipole_inversion(
            &local_field, &eroded_mask, &scan_meta, &inv_config,
            magnitude.as_deref(), &mut *prog,
        ).map_err(|e| QsmxtError::Config(format!("inversion: {}", e)))?;
        save_volume(&chi_raw_path, &chi, ctx.meta)?;
        let lf_input = if skip_bgremove { field_path } else { local_field_path.as_path() };
        let mask_input = if skip_bgremove { mask_path } else { bg_mask_path.as_path() };
        ctx.complete_step("invert", Some(&alg_name),
            invert_params, &[lf_input, mask_input], vec![chi_raw_path], t,
        )?;
        log_step_done(&format!("Dipole inversion ({})", ctx.config.inversion.algorithm), t);
    } else {
        log::info!("Skipping invert (cached)");
    }
    Ok(())
}

fn stage_reference(ctx: &mut StageContext, mask_path: &Path, progress: &dyn Fn(&str)) -> crate::Result<()> {
    let qsm_path = ctx.output.qsm_path(&ctx.run.key);
    let ref_method = format!("{}", ctx.config.qsm.reference);
    let ref_params = serde_json::json!({ "method": ref_method });
    if ctx.is_cached_with_params("reference", Some(&ref_method), &ref_params) {
        log::info!("Skipping reference (cached)");
        return Ok(());
    }
    let t = Instant::now();
    log::info!("QSM referencing ({})", ctx.config.qsm.reference);
    progress("Referencing QSM");
    let chi_raw_path = ctx.output.chi_raw_path(&ctx.run.key);
    let chi = load_volume(&chi_raw_path)?;
    let mask = load_mask(mask_path)?;
    let (_, _, _, ref_method_core) = crate::pipeline::config::to_pipeline_stages(ctx.config);
    let chi_final = qsm_core::pipeline::apply_reference(&chi, &mask, ref_method_core);
    save_volume(&qsm_path, &chi_final, ctx.meta)?;
    ctx.complete_step("reference", Some(&ref_method), ref_params, &[chi_raw_path.as_path(), mask_path], vec![qsm_path], t)?;
    log_step_done("QSM referencing", t);
    Ok(())
}


#[cfg(test)]
mod tests {
    use qsm_core::pipeline::config::QsmReference as CoreRef;

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
            echoes: vec![EchoFiles {
                echo_number: 1, phase_nifti: phase.clone(), phase_json: phase.clone(),
                magnitude_nifti: None, magnitude_json: None,
            }],
            magnetic_field_strength: 3.0, echo_times: vec![0.004], b0_dir: (0.0, 0.0, 1.0),
            dims: (2, 2, 2), has_magnitude: false,
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
}
