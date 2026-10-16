//! R2 from a matched multi-echo spin-echo (MESE) acquisition, and R2′: measured as R2* − R2, or
//! predicted from R2* by R2PRIMEnet when there is no R2 to subtract.

use crate::config::{PipelineConfig, R2primenetConfig};
use crate::error::EngineError;
use crate::scan::RunMetadata;
use crate::store::{Mese, Supplied};

/// The `r2_r2prime` step's parameters.
pub fn params(config: &PipelineConfig, meta: &RunMetadata, mese: Option<&Mese>) -> serde_json::Value {
    let s = &config.separation;
    serde_json::json!({
        "do_r2map": config.pipeline.do_r2map, "do_r2primemap": config.pipeline.do_r2primemap,
        "custom_r2": s.custom_r2_tool,
        "custom_r2prime": s.custom_r2prime_tool,
        "echo_times": meta.echo_times,
        "has_mese": mese.is_some(),
        // The MESE is reconciled onto this run's grid, so both grids belong in the cache key:
        // a MESE swapped for one on another grid produces a different R2 map from the same config.
        "mese_grid": mese.map(|m| (m.dims, m.affine)),
        "grid": (meta.dims, meta.affine),
        "r2prime_strategy": format!("{}", s.r2prime_strategy),
        "r2primenet": s.r2primenet,
    })
}

/// Whether two grids are the same one, to within the precision a NIfTI header stores.
///
/// Affines come back from 32-bit header fields, so bitwise equality is the wrong test: a volume
/// round-tripped through a header can differ in the last few bits and is still the same grid.
/// A millimetre is far below any real difference in FOV, position or orientation and far above
/// that noise.
pub fn same_grid(
    a_dims: (usize, usize, usize), a_affine: &[f64; 16],
    b_dims: (usize, usize, usize), b_affine: &[f64; 16],
) -> bool {
    a_dims == b_dims && a_affine.iter().zip(b_affine.iter()).all(|(x, y)| (x - y).abs() < 1e-3)
}

/// A MESE's echoes on the run's grid.
#[derive(Debug)]
pub struct MeseOnGrid {
    /// Magnitude, voxel-major `(n_voxels, n_se)`.
    pub echoes: Vec<f64>,
    /// Which voxels the MESE's FOV reaches.
    pub coverage: Vec<u8>,
}

/// The MESE's magnitude, voxel-major `(n_voxels, n_se)`, on the grid `dst_dims`/`dst_affine`;
/// `echo(n)` reads echo `n` (1-based) as supplied.
///
/// The MESE is a separate acquisition, so its geometry is reconciled with the reconstruction grid
/// here instead of being assumed. Two things make them disagree, and both are ordinary rather
/// than exceptional:
///
/// - the MESE was acquired on its own grid (a 2D stack with thicker slices, a smaller matrix, a
///   tighter FOV), which is the usual shape of a spin-echo companion to a 3D GRE;
/// - the GRE was resampled to a cardinal grid, which moves it away from whatever the MESE shares.
///
/// Either way the echoes are resampled onto the destination grid — magnitude is continuous, so
/// trilinear interpolation is safe (wrapped phase would have to go through the complex domain
/// instead; see the `qsm_core::geometry` module docs).
///
/// Returns the buffer and a coverage mask marking which destination voxels the MESE's FOV
/// actually reaches: a MESE with fewer slices than the GRE leaves the rest of the grid empty, and
/// R2 must not be reported where nothing was measured. `None` (with a warning) when the MESE
/// cannot be used: an echo unreadable or on a grid of its own, an affine that cannot be inverted.
pub fn mese_on_grid(
    mese: &Mese,
    mut echo: impl FnMut(usize) -> Supplied,
    dst_dims: (usize, usize, usize),
    dst_affine: &[f64; 16],
) -> Result<Option<MeseOnGrid>, EngineError> {
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
            log::warn!("MESE acquisition {} has an affine that cannot be inverted; ignoring the MESE", mese.name);
            return Ok(None);
        };
        c.iter().map(|&v| (v > 0.5) as u8).collect()
    };
    let covered = count_set(&coverage);
    if covered == 0 {
        // Not a grid difference to interpolate across: the two acquisitions do not occupy the
        // same space at all, so there is no reading of this that produces an R2 map.
        return Err(EngineError::Config(format!(
            "The MESE acquisition {} shares no space with the grid this run reconstructs on \
             ({}x{}x{} vs {}x{}x{}), so R2 cannot be computed from it. Check that the MESE and \
             the GRE belong to the same subject and that their affines are not corrupt.",
            mese.name, mese.dims.0, mese.dims.1, mese.dims.2, dst_dims.0, dst_dims.1, dst_dims.2,
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
    for i in 0..n_se {
        let (origin, data, dims, affine) = match echo(i + 1) {
            Supplied::Found { origin, data, dims, affine } => (origin, data, dims, affine),
            Supplied::Unreadable { origin, error } => {
                log::warn!("MESE echo unreadable ({origin}): {error}; ignoring the MESE");
                return Ok(None);
            }
            Supplied::Missing { searched } => {
                log::warn!("MESE echo {} not found in {searched}; ignoring the MESE", i + 1);
                return Ok(None);
            }
        };
        // The acquisition's grid was taken from its first echo. An echo that disagrees with it is
        // a broken series, not a second geometry to reconcile.
        if !same_grid(dims, &affine, mese.dims, &mese.affine) {
            log::warn!(
                "MESE echo {} is on a different grid from the first echo of its own acquisition \
                 ({}x{}x{} vs {}x{}x{}); ignoring the MESE",
                origin, dims.0, dims.1, dims.2, mese.dims.0, mese.dims.1, mese.dims.2,
            );
            return Ok(None);
        }
        let values = if on_grid {
            data
        } else {
            match qsm_core::geometry::resample_onto(&data, mese.dims, &mese.affine, dst_dims, dst_affine) {
                Some(v) => v,
                None => {
                    log::warn!("Cannot resample MESE echo {origin}; ignoring the MESE");
                    return Ok(None);
                }
            }
        };
        for vox in 0..n_voxels { se[vox * n_se + i] = values[vox]; }
    }
    Ok(Some(MeseOnGrid { echoes: se, coverage }))
}

/// The mask a MESE-measured R2 is valid over: the brain mask, narrowed to where the MESE's FOV
/// reached.
///
/// Outside that FOV the resampled echoes are zero because nothing was acquired there, not because
/// the tissue is dark, so a fit there invents an R2 out of interpolated nothing.
pub fn mese_measured_mask(brain: &[u8], coverage: &[u8]) -> Vec<u8> {
    brain.iter().zip(coverage.iter()).map(|(&m, &c)| ((m > 0) && (c > 0)) as u8).collect()
}

pub fn count_set(mask: &[u8]) -> usize {
    mask.iter().filter(|&&m| m > 0).count()
}

/// R2 (Hz) by the EPG fit of the MESE's echoes (`se`, voxel-major) inside `mask`.
pub fn r2_epg(se: &[f64], mask: &[u8], mese: &Mese, meta: &RunMetadata) -> Result<Vec<f64>, EngineError> {
    let ((nx, ny, nz), (vx, vy, vz)) = (meta.dims, meta.voxel_size);
    let grid = qsm_core::Grid::new(nx, ny, nz, vx, vy, vz);
    let p = qsm_core::relaxometry::R2EpgParams::default();
    let (r2, _b1) = qsm_core::relaxometry::r2_epg(se, mask, &mese.echo_times, &grid, &p, None)?;
    Ok(r2)
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
pub fn measured_r2prime(
    r2star: &[f64], r2: &[f64], brain: &[u8], r2_mask: Option<&[u8]>,
) -> Result<(Vec<f64>, Vec<u8>), EngineError> {
    let out = qsm_core::relaxometry::r2prime(r2star, r2, brain, r2_mask)?;
    Ok((out.r2prime, out.coverage))
}

/// The tilt worth warning about before R2PRIMEnet runs, or `None` when there is nothing to say.
///
/// R2PRIMEnet assumes B0 along `+z` and has no way to be told otherwise, so an oblique run that
/// predicts R2′ is normally resampled to axial first. That is decided from the configuration and
/// the scan's facts, which judge a declared R2 source optimistically, so a run can reach R2PRIMEnet
/// having been promised a measured R2 that never materialised (a missing `--use-custom-r2`
/// derivative, an unreadable MESE). A run resampled to a cardinal grid has nothing wrong with it,
/// and a sub-tolerance tilt is the rounding every affine carries.
pub fn r2primenet_grid_tilt(was_resampled: bool, affine: &[f64; 16]) -> Option<f64> {
    if was_resampled {
        return None;
    }
    let tilt = qsm_core::geometry::b0_angle_from_affine(affine);
    (tilt > crate::scan::AXIAL_ONLY_TOLERANCE_DEG).then_some(tilt)
}

/// qsm-core's R2PRIMEnet parameters, or why the configured patch cannot be used.
#[cfg(feature = "onnx")]
pub fn r2primenet_params(c: &R2primenetConfig) -> Result<qsm_core::relaxometry::R2PrimeNetParams, EngineError> {
    c.validate().map_err(EngineError::Config)?;
    Ok(qsm_core::relaxometry::R2PrimeNetParams {
        patch: (c.patch[0], c.patch[1], c.patch[2]),
        ignore_field_mismatch: c.ignore_field_mismatch,
    })
}

/// R2′ (Hz) predicted from R2* inside `mask` by R2PRIMEnet, whose weights must be ready.
///
/// The network's weights are trained at 3 T and it takes no field strength as an input, so
/// `qsm_core` refuses anything far from 3 T unless `separation.r2primenet.ignore_field_mismatch`
/// is set. Whether to get this far at all off 3 T is the step's decision
/// ([`crate::requirements::r2primenet_field_policy`]).
#[cfg(feature = "onnx")]
pub fn r2primenet(
    r2star: &[f64], mask: &[u8], meta: &RunMetadata, config: &R2primenetConfig,
    observer: &mut dyn crate::observe::Observer,
) -> Result<Vec<f64>, EngineError> {
    let params = r2primenet_params(config)?;
    let weights = qsm_core::models::primary_weight("r2primenet")
        .map_err(|e| EngineError::Config(format!("r2primenet weights: {}", e)))?;
    let ((nx, ny, nz), (vx, vy, vz)) = (meta.dims, meta.voxel_size);
    let grid = qsm_core::Grid::new(nx, ny, nz, vx, vy, vz);
    qsm_core::relaxometry::r2primenet(
        r2star, mask, &grid, meta.field_strength, &weights,
        &qsm_core::relaxometry::R2PrimeNetNorm::default(), &params,
        |done, total| observer.progress(done, total),
    ).map_err(|e| EngineError::Config(format!("r2primenet: {}", e)))
}

/// Without the `onnx` feature there is no network to run.
#[cfg(not(feature = "onnx"))]
pub fn r2primenet(
    _r2star: &[f64], _mask: &[u8], _meta: &RunMetadata, _config: &R2primenetConfig,
    _observer: &mut dyn crate::observe::Observer,
) -> Result<Vec<f64>, EngineError> {
    Err(EngineError::Config(
        "estimating R2' needs a deep-learning build: R2PRIMEnet runs an ONNX network, which this \
         binary was compiled without (--no-default-features). Supply a custom R2' map, or a MESE \
         acquisition to measure it from".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diag_affine(vs: f64, origin_x: f64) -> [f64; 16] {
        [vs, 0.0, 0.0, origin_x, 0.0, vs, 0.0, 0.0, 0.0, 0.0, vs, 0.0, 0.0, 0.0, 0.0, 1.0]
    }

    /// A MESE of `n_echoes` on its own grid, and its echoes as `value(echo, i, j, k)` gives them.
    fn mese_with(
        dims: (usize, usize, usize), affine: [f64; 16], n_echoes: usize,
        value: impl Fn(usize, usize, usize, usize) -> f64,
    ) -> (Mese, Vec<Supplied>) {
        let echoes = (0..n_echoes).map(|e| {
            let mut data = vec![0.0f64; dims.0 * dims.1 * dims.2];
            for k in 0..dims.2 { for j in 0..dims.1 { for i in 0..dims.0 {
                data[i + j * dims.0 + k * dims.0 * dims.1] = value(e, i, j, k);
            }}}
            Supplied::Found { origin: format!("echo-{}", e + 1), data, dims, affine }
        }).collect();
        let echo_times = (0..n_echoes).map(|e| 0.01 * (e + 1) as f64).collect();
        (Mese { name: "sub-1_MESE".into(), dims, affine, echo_times }, echoes)
    }

    fn on_grid(
        (mese, echoes): &(Mese, Vec<Supplied>), dims: (usize, usize, usize), affine: [f64; 16],
    ) -> Result<Option<MeseOnGrid>, EngineError> {
        mese_on_grid(mese, |n| echoes[n - 1].clone(), dims, &affine)
    }

    /// The broad case: a 2D-style MESE on a coarser grid of its own. The whole acquisition used to
    /// be discarded here (the voxel counts disagree), so R2 and R2' were never computed.
    #[test]
    fn mese_on_a_different_grid_is_resampled_rather_than_dropped() {
        // 4x4x4 at 2 mm: voxel centres span 0..6 mm on every axis.
        let mese = mese_with((4, 4, 4), diag_affine(2.0, 0.0), 3, |e, _, _, _| 7.0 * (e + 1) as f64);
        // 7x7x7 at 1 mm over the same 0..6 mm box: every destination centre is inside the source.
        let dst_dims = (7, 7, 7);
        let MeseOnGrid { echoes: se, coverage } = on_grid(&mese, dst_dims, diag_affine(1.0, 0.0))
            .expect("a differently-gridded MESE is reconcilable, not an error")
            .expect("and must not be dropped");

        let n = dst_dims.0 * dst_dims.1 * dst_dims.2;
        assert_eq!(se.len(), n * 3, "one value per destination voxel per echo");
        assert!(coverage.iter().all(|&c| c > 0), "the destination box is inside the MESE's FOV");
        // Each echo is constant, so interpolation must reproduce it exactly — and land on the
        // right echo: the buffer is voxel-major, (n_voxels, n_se).
        for vox in 0..n {
            for e in 0..3 {
                let (got, want) = (se[vox * 3 + e], 7.0 * (e + 1) as f64);
                assert!((got - want).abs() < 1e-9, "voxel {vox} echo {e}: got {got}, want {want}");
            }
        }
    }

    /// The correctness case: the same voxel count, so a length check would wave it through, but a
    /// FOV shifted 2 mm along x. Copying it voxel-for-voxel would use it mis-registered.
    #[test]
    fn mese_with_a_matching_voxel_count_but_a_shifted_fov_is_registered_not_copied() {
        // Source value is the x index, so a mis-registration along x is visible in the values.
        let mese = mese_with((8, 8, 8), diag_affine(1.0, 0.0), 3, |_, i, _, _| i as f64);
        // The same voxel count, shifted 2 mm along x.
        let MeseOnGrid { echoes: se, coverage } = on_grid(&mese, (8, 8, 8), diag_affine(1.0, 2.0)).unwrap()
            .expect("a shifted FOV is reconcilable");
        // Along the row y = 0, z = 0, where the voxel index is just the x index.
        let at = |i: usize, e: usize| se[i * 3 + e];
        // Destination voxel i sits at world x = i + 2, which is source voxel i + 2.
        for i in 0..6 {
            assert!((at(i, 0) - (i + 2) as f64).abs() < 1e-9,
                    "dst x={i} should sample source x={}, got {}", i + 2, at(i, 0));
        }
        // The two rightmost columns fall outside the source entirely.
        let covered = |i: usize| coverage[i] > 0;
        assert!(covered(5) && !covered(6) && !covered(7),
                "the shift leaves the last two columns outside the MESE's FOV");
    }

    /// The genuinely undecidable case, which still refuses: the two acquisitions do not occupy the
    /// same space, so there is no interpolation that produces an R2 map.
    #[test]
    fn mese_sharing_no_space_with_the_run_is_refused() {
        let mese = mese_with((4, 4, 4), diag_affine(1.0, 0.0), 3, |_, _, _, _| 1.0);
        let err = on_grid(&mese, (4, 4, 4), diag_affine(1.0, 1000.0))
            .expect_err("a metre apart is not a grid difference to interpolate across");
        assert!(err.to_string().contains("shares no space"), "the error should say why: {err}");
    }

    /// Echoes of one acquisition must agree with each other. That is a broken series rather than a
    /// second geometry to reconcile, so the MESE is dropped (with a warning) and the run goes on.
    /// So is one whose echo cannot be read.
    #[test]
    fn mese_whose_echoes_disagree_or_cannot_be_read_is_dropped() {
        let mut mese = mese_with((4, 4, 4), diag_affine(1.0, 0.0), 3, |_, _, _, _| 1.0);
        mese.1[2] = Supplied::Found {
            origin: "echo-3".into(), data: vec![1.0; 8], dims: (2, 2, 2), affine: diag_affine(1.0, 0.0),
        };
        assert!(on_grid(&mese, (4, 4, 4), diag_affine(1.0, 0.0)).unwrap().is_none(),
                "a series whose echoes are on different grids cannot be fitted");
        mese.1[2] = Supplied::Unreadable { origin: "echo-3".into(), error: "truncated".into() };
        assert!(on_grid(&mese, (4, 4, 4), diag_affine(1.0, 0.0)).unwrap().is_none());
    }

    /// A MESE that does not reach all of the brain leaves R2 unmeasured there, and R2' must not
    /// quietly report R2* - 0 as if the whole of R2* were reversible.
    #[test]
    fn r2_outside_the_mese_fov_is_not_subtracted_as_a_measurement() {
        // Four voxels of brain; the MESE's FOV reaches the first two.
        let brain = [1u8, 1, 1, 1];
        let measured = mese_measured_mask(&brain, &[1u8, 1, 0, 0]);
        assert_eq!(measured, vec![1, 1, 0, 0]);
        assert_eq!(count_set(&measured), 2);

        let r2star = [30.0, 30.0, 30.0, 30.0];
        // R2 was fitted only inside the FOV; outside it the echoes were zero, so it is zero.
        let (r2p, cov) = measured_r2prime(&r2star, &[10.0, 10.0, 0.0, 0.0], &brain, Some(&measured)).unwrap();
        assert_eq!(&r2p[..2], &[20.0, 20.0], "measured where the MESE reached");
        assert_eq!(&r2p[2..], &[0.0, 0.0], "R2* - 0 = 30 here would be an estimate dressed as a measurement");
        assert_eq!(cov, vec![1, 1, 0, 0], "the coverage reported back is the FOV, not the brain");

        // The FOV mask has to do the work on its own when R2 is non-zero outside it: a resampled
        // MESE interpolates across its own boundary, which is why the FOV is found geometrically.
        let (r2p, cov) = measured_r2prime(&r2star, &[10.0, 10.0, 0.6, 0.2], &brain, Some(&measured)).unwrap();
        assert_eq!(&r2p[2..], &[0.0, 0.0], "29.4 and 29.8 here would be nearly the whole of R2* called reversible");
        assert_eq!(cov, vec![1, 1, 0, 0]);
    }

    /// With R2 from a custom derivative there is no FOV to narrow by, so the brain mask stands,
    /// bar any voxel whose R2 is zero: T2 = infinity is not a tissue property, so a zero there is
    /// a gap in the map rather than a measured value to subtract.
    #[test]
    fn a_custom_r2_map_is_subtracted_over_the_whole_brain() {
        let (r2p, cov) = measured_r2prime(
            &[30.0, 30.0, 30.0, 30.0], &[10.0, 10.0, 10.0, 0.0], &[1u8, 1, 0, 1], None,
        ).unwrap();
        assert_eq!(r2p, vec![20.0, 20.0, 0.0, 0.0]);
        assert_eq!(cov, vec![1, 1, 0, 0], "a zero R2 inside the brain mask is an absent measurement, not R2' = R2*");
    }

    /// The backstop for the residual case the plan judges optimistically: a declared R2 source
    /// that never materialises leaves R2PRIMEnet running on an unresampled grid, and this is the
    /// only thing that says so.
    #[test]
    fn r2primenet_warns_only_when_the_grid_was_left_oblique() {
        let (c, s) = (20f64.to_radians().cos(), 20f64.to_radians().sin());
        let oblique = [1.0, 0.0, 0.0, 0.0, 0.0, c, -s, 0.0, 0.0, s, c, 0.0, 0.0, 0.0, 0.0, 1.0];
        assert!(r2primenet_grid_tilt(false, &oblique).is_some_and(|t| t > 1.0),
                "an unresampled oblique grid is exactly the case worth warning about");
        assert!(r2primenet_grid_tilt(true, &oblique).is_none(),
                "a run resampled to a cardinal grid has nothing wrong with it");
        let axial = [0.8, 0.0, 0.0, 0.0, 0.0, 0.8, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        assert!(r2primenet_grid_tilt(false, &axial).is_none(), "an axial acquisition needs no warning");
    }

    #[test]
    fn same_grid_tolerates_header_precision_but_not_a_real_difference() {
        let a = diag_affine(1.0, 0.0);
        let mut rounded = a;
        rounded[3] = 1e-5; // what a 32-bit header field can cost
        assert!(same_grid((4, 4, 4), &a, (4, 4, 4), &rounded));
        assert!(!same_grid((4, 4, 4), &a, (4, 4, 4), &diag_affine(1.0, 0.5)),
                "half a millimetre of offset is a real difference");
        assert!(!same_grid((4, 4, 4), &a, (4, 4, 5), &a), "dims count too");
    }

    /// The engine writes R2PRIMEnet's defaults out (it builds without qsm-core's `onnx`); they
    /// must stay qsm-core's.
    #[cfg(feature = "onnx")]
    #[test]
    fn r2primenet_config_defaults_are_qsm_cores() {
        let ours = r2primenet_params(&Default::default()).unwrap();
        let core = qsm_core::relaxometry::R2PrimeNetParams::default();
        assert_eq!(ours.patch, core.patch);
        assert_eq!(ours.ignore_field_mismatch, core.ignore_field_mismatch);
        assert_eq!(crate::requirements::R2PRIMENET_TRAINED_B0_T, qsm_core::relaxometry::TRAINED_B0_T);
        assert_eq!(crate::requirements::R2PRIMENET_B0_TOLERANCE_T, qsm_core::relaxometry::B0_TOLERANCE_T);
    }
}
