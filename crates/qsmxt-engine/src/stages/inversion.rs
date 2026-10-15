//! Dipole inversion: the local field → the raw (unreferenced) susceptibility map, by whichever
//! route the algorithm takes — a local-field inversion, TGV, iQSM/iQSM+ from phase — and the
//! combination of a two-pass run's two maps.

use crate::config::{PipelineConfig, TgvConfig};
use crate::enums::QsmAlgorithm;
use crate::error::EngineError;
use crate::observe::Observer;
use crate::scan::{scan_metadata, scan_metadata_in, RunMetadata};
use crate::stages::crop::crop_box_for;

/// The inversion step's parameters: the algorithm's user-facing parameters (the algorithm is
/// recorded beside them).
///
/// A parameter missing here is a parameter whose change does not invalidate a cached
/// reconstruction, so the run silently reuses a map computed with the old value.
pub fn params(config: &PipelineConfig) -> serde_json::Value {
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
            "pad_mm": config.inversion.ilsqr.pad_mm,
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

/// Invert the local field `local_field` (ppm) inside `mask` (the mask background removal left, or
/// the brain mask for an inversion that removes the background itself, which then takes the total
/// field) with the configured algorithm. `magnitude` is the combined magnitude, for the
/// algorithms that weight by it (MEDI's edges). Returns the raw susceptibility map in ppm.
///
/// Runs in the reconstruction box ([`crop_box_for`]); the dipole kernel has infinite support, so
/// this is the stage most exposed to the periodic boundary moving inward, and the margin is there
/// to keep it off the object.
pub fn invert(
    local_field: &[f64], mask: &[u8], magnitude: Option<&[f64]>, meta: &RunMetadata,
    config: &PipelineConfig, observer: &mut dyn Observer,
) -> Result<Vec<f64>, EngineError> {
    let (_, _, inv_config, _) = crate::bridge::to_pipeline_stages(config);
    let cb = crop_box_for(config, meta, mask, "Dipole inversion");
    let magnitude = magnitude.map(|m| qsm_core::crop::crop_volume(m, &cb));
    let chi = qsm_core::pipeline::run_dipole_inversion(
        &qsm_core::crop::crop_volume(local_field, &cb),
        &qsm_core::crop::crop_volume(mask, &cb),
        &scan_metadata_in(meta, &cb), &inv_config, magnitude.as_deref(),
        &mut |done, total| observer.progress(done, total),
    ).map_err(|e| EngineError::Config(format!("inversion: {}", e)))?;
    Ok(qsm_core::crop::uncrop_volume(&chi, &cb, 0.0))
}

/// What TGV reconstructs from: the combined field map of a multi-echo run, or the first echo's
/// wrapped phase of a single-echo one (which TGV unwraps itself and converts to ppm with TE1 and
/// B0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TgvSource {
    /// The total field in ppm.
    FieldPpm,
    /// Echo 1's wrapped phase in radians.
    WrappedPhase,
}

/// What TGV reconstructs from for this run.
pub fn tgv_source(meta: &RunMetadata) -> TgvSource {
    if meta.n_echoes > 1 { TgvSource::FieldPpm } else { TgvSource::WrappedPhase }
}

/// The TGV step's parameters. α are in ppm whichever the input.
pub fn tgv_params(config: &PipelineConfig, meta: &RunMetadata) -> serde_json::Value {
    let tgv = &config.inversion.tgv;
    let wrapped = tgv_source(meta) == TgvSource::WrappedPhase;
    let mut params = serde_json::json!({
        "iterations": tgv.iterations,
        "alphas_ppm": [tgv.alpha1, tgv.alpha0],
        "erosions": tgv.erosions,
        "step_size": tgv.step_size,
        "tol": tgv.tol,
        "input": if wrapped { "wrapped_phase" } else { "field_ppm" },
    });
    if wrapped {
        params["te_ms"] = serde_json::json!(meta.echo_times[0] * 1000.0);
        params["field_strength"] = serde_json::json!(meta.field_strength);
    }
    params
}

/// qsm-core's TGV parameters from QSMxT's TGV config. α are in ppm.
pub fn tgv_params_from_config(c: &TgvConfig) -> qsm_core::inversion::TgvParams {
    qsm_core::inversion::TgvParams {
        alpha0: c.alpha0 as f32,
        alpha1: c.alpha1 as f32,
        iterations: c.iterations,
        erosions: c.erosions,
        step_size: c.step_size as f32,
        tol: c.tol as f32,
    }
}

/// TGV-QSM inside the brain `mask`, from `data`: what [`tgv_source`] says for this run (the total
/// field in ppm, or echo 1's wrapped phase). Returns the raw susceptibility map in ppm.
pub fn tgv(
    data: &[f64], mask: &[u8], meta: &RunMetadata, config: &PipelineConfig,
) -> Result<Vec<f64>, EngineError> {
    let tgv = &config.inversion.tgv;
    let te1_s = meta.echo_times[0];
    let b0_t = meta.field_strength;
    let input = match tgv_source(meta) {
        TgvSource::FieldPpm => {
            log::info!(
                "TGV-QSM on the B0 field map (iterations={}, alphas=[{}, {}] ppm, erosions={})",
                tgv.iterations, tgv.alpha1, tgv.alpha0, tgv.erosions,
            );
            qsm_core::inversion::TgvInput::FieldPpm(data)
        }
        TgvSource::WrappedPhase => {
            log::info!(
                "TGV-QSM on wrapped echo-1 phase (iterations={}, alphas=[{}, {}] ppm, erosions={}, TE={:.3}ms, B0={:.1}T)",
                tgv.iterations, tgv.alpha1, tgv.alpha0, tgv.erosions, te1_s * 1000.0, b0_t,
            );
            if !(te1_s.is_finite() && te1_s > 0.0 && b0_t.is_finite() && b0_t > 0.0) {
                return Err(EngineError::Config(format!(
                    "single-echo TGV needs a positive echo time and field strength (TE1 = {te1_s} s, B0 = {b0_t} T)"
                )));
            }
            qsm_core::inversion::TgvInput::WrappedPhase { phase: data, te_s: te1_s, b0_t }
        }
    };
    let (nx, ny, nz) = meta.dims;
    let (vsx, vsy, vsz) = meta.voxel_size;
    let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
    Ok(qsm_core::inversion::tgv_qsm(input, mask, &grid, &tgv_params_from_config(tgv), meta.b0_direction, |_, _| {})?)
}

/// The algorithm id of an end-to-end reconstruction from phase (`iqsm` or `iqsm-plus`): its
/// step's algorithm and the model whose weights it needs. `None` for any other inversion.
pub fn iqsm_algorithm(config: &PipelineConfig) -> Option<&'static str> {
    match config.inversion.algorithm {
        QsmAlgorithm::Iqsm => Some("iqsm"),
        QsmAlgorithm::IqsmPlus => Some("iqsm-plus"),
        _ => None,
    }
}

/// The iQSM/iQSM+ step's parameters.
pub fn iqsm_params(meta: &RunMetadata) -> serde_json::Value {
    serde_json::json!({
        "echo_times": meta.echo_times,
        "field_strength": meta.field_strength,
        "b0_direction": [meta.b0_direction.0, meta.b0_direction.1, meta.b0_direction.2],
    })
}

/// iQSM / iQSM+: the raw susceptibility map straight from every echo's wrapped phase (joint
/// unwrapping, background removal and dipole inversion in one network), inside the brain `mask`.
/// `magnitudes` may be empty (uniform weights). Referencing is left to the referencing stage.
pub fn iqsm(
    phases: &[&[f64]], magnitudes: &[&[f64]], mask: &[u8], meta: &RunMetadata,
    config: &PipelineConfig,
) -> Result<Vec<f64>, EngineError> {
    let alg = iqsm_algorithm(config).ok_or_else(|| EngineError::Config(format!(
        "iQSM reconstruction asked for with --qsm-algorithm {}", config.inversion.algorithm)))?;
    let plus = alg == "iqsm-plus";
    log::info!("{} reconstruction (end-to-end from phase)", if plus { "iQSM+" } else { "iQSM" });
    let scan_meta = scan_metadata(meta);
    let none = qsm_core::pipeline::QsmReference::None;
    if plus {
        qsm_core::pipeline::run_iqsm_plus(phases, magnitudes, mask, &scan_meta, none)
    } else {
        qsm_core::pipeline::run_iqsm(phases, magnitudes, mask, &scan_meta, none)
    }.map_err(|e| EngineError::Config(format!("{}: {}", alg, e)))
}

/// Combine the two reconstructions of a two-pass run into one raw susceptibility map.
///
/// The reliable pass wins wherever it produced a value; the ordinary pass fills the holes its mask
/// left, and the rim background removal eroded off it. Nothing is averaged — each voxel comes from
/// exactly one pass — so this cannot blur the reliable pass's values with the ones it was run to
/// avoid.
///
/// Where the reliable pass produced a value is its declared support (`reliable_support`: the
/// mask its reconstruction was defined on), minus any voxel the reconstruction left at exactly
/// zero. Several algorithms erode beyond the mask they were handed — V-SHARP by its kernel radius,
/// TGV by `tgv_erosions` — and trusting the mask over that rim would ring every hole with a seam of
/// zeros instead of filling it from the single-pass map. An exact 0.0 inside a reconstruction is
/// masking, not a measurement. `brain` (the main pass's mask) is only for the log.
pub fn combine_two_pass(
    chi_reliable: &[f64], chi_main: &[f64], reliable_support: &[u8], brain: &[u8],
) -> Result<Vec<f64>, EngineError> {
    let support: Vec<u8> = reliable_support.iter().zip(chi_reliable)
        .map(|(&m, &chi)| u8::from(m != 0 && chi != 0.0))
        .collect();
    let kept = support.iter().filter(|&&v| v != 0).count();
    log::info!(
        "Two-pass combination: reliable pass over {} voxels ({:.1}% of the brain mask), \
         single-pass elsewhere",
        kept,
        100.0 * kept as f64 / brain.iter().filter(|&&v| v != 0).count().max(1) as f64,
    );
    qsm_core::pipeline::combine_two_pass(chi_reliable, chi_main, &support)
        .map_err(|e| EngineError::Config(format!("two-pass combination: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// HEIDI seeds itself with an LSQR solve, so an LSQR parameter changes the HEIDI result and
    /// must invalidate it. Its cache key therefore has to carry both groups — otherwise editing
    /// `--lsqr-tol` silently reuses a HEIDI map computed with the old tolerance.
    #[test]
    fn the_heidi_cache_key_covers_its_lsqr_seed() {
        let key = |c: &PipelineConfig| crate::cache::step_params_hash(
            Some(&format!("{}", c.inversion.algorithm)), &params(c));

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

    #[test]
    fn tgv_reads_the_field_for_several_echoes_and_echo_1_phase_for_one() {
        let mut meta = crate::stages::magnitude::tests::meta_4x4x4();
        let config = PipelineConfig::default();
        assert_eq!(tgv_source(&meta), TgvSource::WrappedPhase);
        assert_eq!(tgv_params(&config, &meta)["input"], "wrapped_phase");
        assert!(tgv_params(&config, &meta).get("te_ms").is_some());
        meta.n_echoes = 3;
        meta.echo_times = vec![0.004, 0.008, 0.012];
        assert_eq!(tgv_source(&meta), TgvSource::FieldPpm);
        assert!(tgv_params(&config, &meta).get("te_ms").is_none(), "TE does not change a field-map run");

        // A single echo without a usable echo time is refused, not divided by.
        meta.n_echoes = 1;
        meta.echo_times = vec![0.0];
        let err = tgv(&[0.0; 64], &[1; 64], &meta, &config).unwrap_err();
        assert!(format!("{err}").contains("single-echo TGV"), "{err}");
    }

    /// The reliable pass is kept where it has a value; a zero inside its mask is a hole to fill.
    #[test]
    fn two_pass_fills_the_reliable_pass_holes_from_the_main_pass() {
        let reliable = [1.0, 0.0, 3.0, 4.0];
        let main = [10.0, 20.0, 30.0, 40.0];
        let support = [1u8, 1, 1, 0];
        let out = combine_two_pass(&reliable, &main, &support, &[1, 1, 1, 1]).unwrap();
        assert_eq!(out, vec![1.0, 20.0, 3.0, 40.0]);
    }

    #[test]
    fn only_the_phase_networks_are_iqsm() {
        let mut config = PipelineConfig::default();
        assert_eq!(iqsm_algorithm(&config), None);
        config.inversion.algorithm = QsmAlgorithm::IqsmPlus;
        assert_eq!(iqsm_algorithm(&config), Some("iqsm-plus"));
    }
}
