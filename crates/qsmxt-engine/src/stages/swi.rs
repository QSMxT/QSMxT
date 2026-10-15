//! Susceptibility-weighted imaging: CLEAR-SWI from the echoes, SMWI from the susceptibility map,
//! and the minimum-intensity projection of each.

use crate::config::PipelineConfig;
use crate::error::EngineError;
use crate::scan::RunMetadata;
use crate::store::{Contrast, Geometry};

/// An image and its minimum-intensity projection over slabs of slices.
pub struct Projected {
    /// The image, on the run's grid.
    pub image: Vec<f64>,
    /// Its projection, on [`Self::mip_geometry`]: shorter along the slice axis, centred on each
    /// slab.
    pub mip: Vec<f64>,
    pub mip_geometry: Geometry,
}

fn grid(meta: &RunMetadata) -> qsm_core::Grid {
    let ((nx, ny, nz), (vx, vy, vz)) = (meta.dims, meta.voxel_size);
    qsm_core::Grid::new(nx, ny, nz, vx, vy, vz)
}

/// The dimensions of a `window`-slice projection of the run's grid, or why that window cannot
/// be used. Checked before any work, so an impossible window fails early.
fn mip_dims(meta: &RunMetadata, window: usize) -> Result<[usize; 3], EngineError> {
    let (g, _) = qsm_core::swi::mip_geometry(&grid(meta), &meta.affine, window).map_err(EngineError::Config)?;
    Ok([g.nx(), g.ny(), g.nz()])
}

/// `image`'s minimum-intensity projection over `window` slices (on `meta`'s grid, or `affine`).
pub fn project(image: &[f64], meta: &RunMetadata, window: usize) -> Result<(Vec<f64>, Geometry), EngineError> {
    let g = grid(meta);
    project_on(image, &g, &meta.affine, window)
}

/// [`project`] on any grid: the acquired one, for an output returned to it.
pub fn project_on(
    image: &[f64], grid: &qsm_core::Grid, affine: &[f64; 16], window: usize,
) -> Result<(Vec<f64>, Geometry), EngineError> {
    let mip = qsm_core::swi::create_mip(image, grid, affine, window).map_err(EngineError::Config)?;
    let geometry = Geometry { dims: mip.grid.dims, voxel_size: mip.grid.voxel_size, affine: mip.affine };
    Ok((mip.data, geometry))
}

/// The `swi` step's parameters.
pub fn swi_params(config: &PipelineConfig, meta: &RunMetadata) -> Result<serde_json::Value, EngineError> {
    let s = &config.swi;
    let mip_dims = mip_dims(meta, s.mip_window)?;
    Ok(serde_json::json!({
        "unwrap": s.unwrap,
        "scaling": s.scaling,
        "strength": s.strength,
        "hp_sigma": s.hp_sigma,
        "mip_window": s.mip_window,
        // Every echo contributes, so their number and times are part of what was computed. Their
        // absence also retires SWIs cached when the stage used the first echo only.
        "echoes": meta.n_echoes,
        "echo_times": meta.echo_times,
        // Part of the cache key so a minIP written with full-volume dimensions before this was
        // fixed (issue #211) is rebuilt rather than kept.
        "mip_dims": mip_dims,
    }))
}

/// CLEAR-SWI over every echo, and its minIP.
///
/// The phase is filtered within CLEAR-SWI's own mask (from the first echo's magnitude) rather
/// than the brain mask, as CLEAR-SWI does: the SWI covers the head, not just the brain.
pub fn swi(
    phases: &[Vec<f64>], magnitudes: &[Vec<f64>], meta: &RunMetadata, config: &PipelineConfig,
) -> Result<Projected, EngineError> {
    let image = qsm_core::swi::calculate_swi(
        phases, magnitudes, &meta.echo_times, None, &grid(meta), &config.swi.to_params(),
    )?.swi;
    // The projection carries its own geometry: shorter than the SWI along the slice axis, and
    // centred on each slab. Writing it under the run's dimensions is issue #211.
    let (mip, mip_geometry) = project(&image, meta, config.swi.mip_window)?;
    Ok(Projected { image, mip, mip_geometry })
}

/// The `smwi` step's parameters.
pub fn smwi_params(config: &PipelineConfig, meta: &RunMetadata) -> Result<serde_json::Value, EngineError> {
    let s = &config.smwi;
    let mip_dims = mip_dims(meta, s.mip_window)?;
    Ok(serde_json::json!({
        "threshold_ppm": s.threshold_ppm,
        "power": s.power,
        "mip_window": s.mip_window,
        "mip_dims": mip_dims,
    }))
}

/// Susceptibility map-weighted imaging: the combined `magnitude` weighted by a mask built from
/// the referenced map `chi` inside `mask`, for each sign of χ, with each image's minIP.
///
/// Unlike SWI, the weighting comes from the susceptibility map rather than from high-pass
/// filtered phase, so the contrast sits where the source is instead of spreading with the dipole
/// field. The paramagnetic image suppresses positive χ (iron, haemorrhage), the diamagnetic one
/// negative χ (calcification, myelin).
pub fn smwi(
    magnitude: &[f64], chi: &[f64], mask: &[u8], meta: &RunMetadata, config: &PipelineConfig,
) -> Result<Vec<(Contrast, Projected)>, EngineError> {
    let g = grid(meta);
    let s = &config.smwi;
    let core = qsm_core::swi::SmwiParams { threshold_ppm: s.threshold_ppm, power: s.power, mip_window: s.mip_window };
    let (para, dia) = qsm_core::swi::calculate_smwi(magnitude, chi, Some(mask), &g, &core);
    [(Contrast::Paramagnetic, para), (Contrast::Diamagnetic, dia)].into_iter().map(|(contrast, data)| {
        // The combined magnitude is one volume here, but calculate_smwi also accepts echoes
        // stacked along the slowest axis; collapse before projecting either way.
        let image = qsm_core::swi::average_echoes(&data, &g);
        let (mip, mip_geometry) = project(&image, meta, s.mip_window)?;
        Ok((contrast, Projected { image, mip, mip_geometry }))
    }).collect()
}
