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
    // Each method's settings serialised whole, so that a setting added to its config is recorded
    // (and keyed) without anyone listing it here.
    let inv = &config.inversion;
    match inv.algorithm {
        QsmAlgorithm::Rts => all(&inv.rts),
        QsmAlgorithm::Tv => all(&inv.tv),
        QsmAlgorithm::Tkd => all(&inv.tkd),
        QsmAlgorithm::Tsvd => all(&inv.tsvd),
        QsmAlgorithm::Ilsqr => all(&inv.ilsqr),
        QsmAlgorithm::Tikhonov => all(&inv.tikhonov),
        QsmAlgorithm::Nltv => all(&inv.nltv),
        QsmAlgorithm::Medi => all(&inv.medi),
        QsmAlgorithm::Tfi => all(&inv.tfi),
        QsmAlgorithm::Ndi => all(&inv.ndi),
        QsmAlgorithm::Fansi | QsmAlgorithm::FansiTgv => {
            let mut key = all(&inv.fansi);
            key["is_tgv"] = serde_json::json!(inv.algorithm == QsmAlgorithm::FansiTgv);
            key
        }
        QsmAlgorithm::L1qsm => all(&inv.l1qsm),
        QsmAlgorithm::Whqsm => all(&inv.whqsm),
        QsmAlgorithm::Hdqsm => all(&inv.hdqsm),
        QsmAlgorithm::AmpPe => all(&inv.amp_pe),
        // HEIDI's key carries the LSQR group too — its seed is an LSQR solve, so changing
        // `--lsqr-tol` changes the HEIDI result and has to invalidate it.
        QsmAlgorithm::Lsqr => serde_json::json!({ "lsqr": all(&inv.lsqr) }),
        QsmAlgorithm::Heidi => serde_json::json!({ "lsqr": all(&inv.lsqr), "heidi": all(&inv.heidi) }),
        // The networks have no parameters; a tiled run is a different result from a
        // whole-volume one, so the tiling is in the key.
        a if a.tileable() => match config.dl_tile() {
            Some((core, halo)) => serde_json::json!({ "tile": [core, halo] }),
            None => serde_json::json!({}),
        },
        _ => serde_json::json!({}),
    }
}

/// A method's settings, serialised whole.
fn all<T: serde::Serialize>(settings: &T) -> serde_json::Value {
    serde_json::to_value(settings).unwrap_or_default()
}

/// Which magnitude an inversion takes, by what qsm-core's dispatcher asks of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MagnitudeInput {
    /// The combined magnitude, when the run has one: the data weight of the methods that weight
    /// by the magnitude.
    Combined,
    /// Every echo's magnitude, stacked with the echo slowest: MEDI and TFI, which make their
    /// edge mask and noise map (MEDI toolbox `Fit_ppm_complex`) from the echoes.
    Echoes,
}

/// The magnitude the configured inversion takes ([`MagnitudeInput`]), or `None` for one that
/// takes none.
pub fn magnitude_input(config: &PipelineConfig) -> Option<MagnitudeInput> {
    use QsmAlgorithm as A;
    match config.inversion.algorithm {
        A::Medi | A::Tfi => Some(MagnitudeInput::Echoes),
        A::Nltv | A::Ndi | A::Fansi | A::FansiTgv | A::L1qsm | A::Hdqsm | A::AmpPe | A::Lsqr | A::Heidi => {
            Some(MagnitudeInput::Combined)
        }
        // WH-QSM weights by the magnitude only with a magnitude weight (WH_nlTV's default is flat).
        A::Whqsm => crate::bridge::to_pipeline_stages(config).2.whqsm.weight.needs_magnitude()
            .then_some(MagnitudeInput::Combined),
        _ => None,
    }
}

/// `volume` cropped to `cb`, or, for a stack of several volumes of `n` voxels each (the echoes,
/// echo slowest), each volume cropped and the results stacked again.
pub(crate) fn crop_stack(volume: &[f64], n: usize, cb: &qsm_core::crop::CropBox) -> Vec<f64> {
    if n == 0 || volume.len() == n {
        return qsm_core::crop::crop_volume(volume, cb);
    }
    volume.chunks(n).flat_map(|v| qsm_core::crop::crop_volume(v, cb)).collect()
}

/// Invert the local field `local_field` (ppm) inside `mask` (the mask background removal left, or
/// the brain mask for an inversion that removes the background itself, which then takes the total
/// field) with the configured algorithm. `magnitude` is what [`magnitude_input`] says the
/// algorithm takes: the combined magnitude, or every echo's, stacked with the echo slowest.
/// Returns the raw susceptibility map in ppm.
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
    let magnitude = magnitude.map(|m| crop_stack(m, mask.len(), &cb));
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

/// The TGV step's parameters. α are in the reference's units, radians at the echo time the input
/// is phase at (the one `te` picks for a field, a wrapped echo's own) and the field strength.
pub fn tgv_params(config: &PipelineConfig, meta: &RunMetadata) -> serde_json::Value {
    let tgv = &config.inversion.tgv;
    let wrapped = tgv_source(meta) == TgvSource::WrappedPhase;
    serde_json::json!({
        "iterations": tgv.iterations,
        "alphas_rad": [tgv.alpha1, tgv.alpha0],
        "erosions": tgv.erosions,
        "step_size": tgv.step_size,
        "input": if wrapped { "wrapped_phase" } else { "field_ppm" },
        "te": tgv.te,
        "te_ms": tgv_te(config, meta).unwrap_or(f64::NAN) * 1000.0,
        "field_strength": meta.field_strength,
    })
}

/// The echo time TGV's input is phase at, in seconds: for a field, the one `te` picks from the
/// scan's echo times; for a single echo's wrapped phase, that echo's.
pub fn tgv_te(config: &PipelineConfig, meta: &RunMetadata) -> Result<f64, String> {
    match tgv_source(meta) {
        TgvSource::FieldPpm => qsm_core::EchoTime::from(config.inversion.tgv.te).resolve(&meta.echo_times),
        TgvSource::WrappedPhase => Ok(meta.echo_times[0]),
    }
}

/// The echo time the configured inversion converts the field from ppm to radians at, in seconds,
/// for a method that takes a `te` choice: the choice resolved against the scan's echo times.
/// `None` for a method without one, or when the choice cannot be resolved (requirements refuse
/// that run).
pub fn resolved_te(config: &PipelineConfig, meta: &RunMetadata) -> Option<f64> {
    let te = echo_time_choice(config, config.inversion.algorithm)?;
    qsm_core::EchoTime::from(te).resolve(&meta.echo_times).ok()
}

/// The echo time choice of inversion `alg`, for a method that converts the field from ppm to
/// radians at one of the scan's echo times.
pub fn echo_time_choice(config: &PipelineConfig, alg: QsmAlgorithm) -> Option<crate::EchoTimeChoice> {
    let inv = &config.inversion;
    Some(match alg {
        QsmAlgorithm::Nltv => inv.nltv.te,
        QsmAlgorithm::Medi => inv.medi.te,
        QsmAlgorithm::Tfi => inv.tfi.te,
        QsmAlgorithm::Ndi => inv.ndi.te,
        QsmAlgorithm::Fansi | QsmAlgorithm::FansiTgv => inv.fansi.te,
        QsmAlgorithm::L1qsm => inv.l1qsm.te,
        QsmAlgorithm::Whqsm => inv.whqsm.te,
        QsmAlgorithm::Heidi => inv.heidi.te,
        QsmAlgorithm::Tgv => inv.tgv.te,
        _ => return None,
    })
}

/// qsm-core's TGV parameters from QSMxT's TGV config.
pub fn tgv_params_from_config(c: &TgvConfig) -> qsm_core::inversion::TgvParams {
    qsm_core::inversion::TgvParams {
        alpha0: c.alpha0 as f32,
        alpha1: c.alpha1 as f32,
        iterations: c.iterations,
        erosions: c.erosions,
        step_size: c.step_size as f32,
        te: c.te.into(),
    }
}

/// TGV-QSM inside the brain `mask`, from `data`: what [`tgv_source`] says for this run (the total
/// field in ppm, which TGV expresses as phase at the echo time `te` picks, or echo 1's wrapped
/// phase).
/// Returns the raw susceptibility map in ppm.
pub fn tgv(
    data: &[f64], mask: &[u8], meta: &RunMetadata, config: &PipelineConfig,
) -> Result<Vec<f64>, EngineError> {
    let tgv = &config.inversion.tgv;
    let te1_s = meta.echo_times[0];
    let b0_t = meta.field_strength;
    let iterations = tgv.iterations.map_or_else(|| "from the voxel size".to_string(), |n| n.to_string());
    let te_s = tgv_te(config, meta).map_err(|e| EngineError::Config(format!("TGV: {e}")))?;
    if !(te_s.is_finite() && te_s > 0.0 && b0_t.is_finite() && b0_t > 0.0) {
        return Err(EngineError::Config(format!(
            "TGV needs a positive echo time and field strength (TE = {te_s} s, B0 = {b0_t} T)"
        )));
    }
    let input = match tgv_source(meta) {
        TgvSource::FieldPpm => {
            log::info!(
                "TGV-QSM on the B0 field map (iterations {iterations}, alphas [{}, {}] rad at TE = {:.3}ms, erosions {})",
                tgv.alpha1, tgv.alpha0, te_s * 1000.0, tgv.erosions,
            );
            qsm_core::inversion::TgvInput::FieldPpm { field: data, echo_times: &meta.echo_times, b0_t }
        }
        TgvSource::WrappedPhase => {
            log::info!(
                "TGV-QSM on wrapped echo-1 phase (iterations {iterations}, alphas [{}, {}] rad, erosions {}, TE={:.3}ms, B0={:.1}T)",
                tgv.alpha1, tgv.alpha0, tgv.erosions, te1_s * 1000.0, b0_t,
            );
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
        qsm_core::pipeline::run_iqsm_plus(phases, magnitudes, mask, &scan_meta, none, config.dl_tile())
    } else {
        qsm_core::pipeline::run_iqsm(phases, magnitudes, mask, &scan_meta, none, config.dl_tile())
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

    /// The echo time a method converts at is in its key, as chosen and as the scan resolves it
    /// (which `run` adds); TGV's key carries the resolved time too.
    #[test]
    fn the_echo_time_is_in_the_inversion_keys() {
        let key = |c: &PipelineConfig| crate::cache::step_params_hash(
            Some(&format!("{}", c.inversion.algorithm)), &params(c));
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Fansi;
        let base = key(&c);
        c.inversion.fansi.te = crate::EchoTimeChoice::First;
        assert_ne!(key(&c), base);
        let mut meta = crate::stages::magnitude::tests::meta_4x4x4();
        (meta.n_echoes, meta.echo_times) = (3, vec![0.004, 0.008, 0.012]);
        assert_eq!(resolved_te(&c, &meta), Some(0.004));
        c.inversion.fansi.te = crate::EchoTimeChoice::Spacing;
        assert!((resolved_te(&c, &meta).unwrap() - 0.004).abs() < 1e-12);
        c.inversion.algorithm = QsmAlgorithm::Ilsqr;
        assert_eq!(resolved_te(&c, &meta), None, "iLSQR does not convert the field");
        c.inversion.algorithm = QsmAlgorithm::Tgv;
        c.inversion.tgv.te = crate::EchoTimeChoice::Last;
        assert!((tgv_params(&c, &meta)["te_ms"].as_f64().unwrap() - 12.0).abs() < 1e-9);
        assert_eq!(tgv_params(&c, &meta)["te"], "last");
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
        // α are in radians at TE1 and B0 either way, so both are in the key.
        assert!((tgv_params(&config, &meta)["te_ms"].as_f64().unwrap() - 4.0).abs() < 1e-9);
        assert_eq!(tgv_params(&config, &meta)["input"], "field_ppm");

        // An echo time or field strength TGV cannot use is refused, not divided by.
        meta.n_echoes = 1;
        meta.echo_times = vec![0.0];
        let err = tgv(&[0.0; 64], &[1; 64], &meta, &config).unwrap_err();
        assert!(format!("{err}").contains("TGV needs a positive echo time"), "{err}");
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
