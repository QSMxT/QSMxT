//! Multi-echo B0 field mapping: wrapped phase per echo → B0 field map in ppm.
//!
//! QSMxT's entry point for the field-mapping stage, shared by `qsmxt run` and `qsmxt fieldmap`.
//! It is qsm-core's [`run_field_mapping`](qsm_core::pipeline::run_field_mapping) behind QSMxT's
//! stricter input checks. Everything this module used to do itself, qsm-core now does on its
//! own path (QSM.rs#161/#162/#166), and the tests hold it to the old implementation ([`legacy`]):
//!
//! * **Laplacian unwrapping** unwraps each echo on its own (phase zeroed outside the mask, as STI
//!   Suite's `MRPhaseUnwrap` is called by UK Biobank), aligns the constant a Laplacian unwrap
//!   leaves undetermined across echoes, and combines the echoes with the configured estimator,
//!   so `--b0-estimation` and `--b0-weight-type` mean the same as with ROMEO.
//! * **`assumed-decay` weighting** is qsm-core's `B0WeightType::AssumedDecay`.
//! * The **FFT Laplacian solver** is qsm-core's `LaplacianSolver::Fft`.
//!
//! * A **single Laplacian echo** is masked and demeaned, and **`linear-fit`** aligns the
//!   Laplacian per-echo constants first (QSM.rs#167).
//!
//! The UK Biobank recipe (per-echo Laplacian unwrapping as STI Suite's `MRPhaseUnwrap`, then a
//! mean weighted by `TE·exp(−TE/T2*)` with an assumed T2* of 40 ms) is
//! `--unwrapping-algorithm laplacian --laplacian-solver fft --b0-weight-type assumed-decay`.

use qsm_core::pipeline::config::{
    B0EstimationMethod, FieldMappingConfig, PipelineError, ScanMetadata,
};
pub use qsm_core::pipeline::FieldMappingResult;
use qsm_core::utils::B0WeightType;

/// QSMxT's input checks, stricter than qsm-core's: a magnitude per echo, every volume and the
/// mask the size of the grid, and positive echo times.
fn validate(
    phases: &[&[f64]], mags: Option<&[&[f64]]>, mask: &[u8], meta: &ScanMetadata,
    config: &FieldMappingConfig,
) -> Result<(), PipelineError> {
    let n = meta.dims.0 * meta.dims.1 * meta.dims.2;
    if phases.is_empty() {
        return Err(PipelineError::InvalidInput("no phase echoes provided".into()));
    }
    if meta.echo_times.len() != phases.len() {
        return Err(PipelineError::DimensionMismatch { expected: phases.len(), got: meta.echo_times.len() });
    }
    if let Some(m) = mags {
        if m.len() != phases.len() {
            return Err(PipelineError::DimensionMismatch { expected: phases.len(), got: m.len() });
        }
    }
    let lens = phases.iter().chain(mags.unwrap_or(&[])).map(|v| v.len()).chain(std::iter::once(mask.len()));
    if let Some(got) = lens.into_iter().find(|&l| l != n) {
        return Err(PipelineError::DimensionMismatch { expected: n, got });
    }
    if meta.echo_times.iter().any(|&te| !te.is_finite() || te <= 0.0) {
        return Err(PipelineError::InvalidInput("echo times must be positive".into()));
    }
    if let B0EstimationMethod::WeightedAvg { weighting: B0WeightType::AssumedDecay { t2star_s } } = config.b0_estimation {
        if !(t2star_s.is_finite() && t2star_s > 0.0) {
            return Err(PipelineError::InvalidConfig(format!(
                "assumed T2* for the assumed-decay B0 weighting must be positive, got {t2star_s} s")));
        }
    }
    Ok(())
}

/// Run field mapping: per-echo wrapped phase (rad, in [−π, π]) → B0 field in ppm, zero outside
/// `mask`, with everything (unwrapping, Laplacian solver, B0 estimation and its weighting)
/// taken from `config`. `magnitudes` = `None` weights every echo and voxel equally.
pub fn run_field_mapping(
    phases: &[&[f64]],
    magnitudes: Option<&[&[f64]]>,
    mask: &[u8],
    meta: &ScanMetadata,
    config: &FieldMappingConfig,
) -> Result<FieldMappingResult, PipelineError> {
    validate(phases, magnitudes, mask, meta, config)?;
    qsm_core::pipeline::run_field_mapping(phases, magnitudes, mask, meta, config, &mut |_, _| {})
}

/// Log what the motion-robust fit found: one line per echo, and a warning naming flagged echoes.
pub fn log_robust_fit_report(report: &qsm_core::pipeline::RobustFitReport) {
    let q = &report.quality;
    log::info!("Robust echo fit over {} voxels (echo: down-weighted / rejected fraction, median weight, \
                median |residual| rad, outlier score):", q.voxels_examined);
    for e in 0..q.median_weight.len() {
        log::info!("  echo {}: {:.3} / {:.3}, {:.3}, {:.4}, {:.4}", e + 1,
            q.downweighted_fraction[e], q.rejected_fraction[e], q.median_weight[e],
            q.median_abs_residual_rad[e], q.outlier_score[e]);
    }
    if !q.flagged.is_empty() {
        let names: Vec<String> = q.flagged.iter().map(|e| (e + 1).to_string()).collect();
        log::warn!("Robust echo fit flagged echo(es) {} as inconsistent with the rest (motion or \
                    signal loss?); they were down-weighted where they departed, not dropped",
                   names.join(", "));
    }
}

/// The robust fit's per-echo report as JSON (echoes 1-based), for the field map's sidecar.
pub fn robust_fit_report_json(report: &qsm_core::pipeline::RobustFitReport) -> serde_json::Value {
    let q = &report.quality;
    serde_json::json!({
        "voxels_examined": q.voxels_examined,
        "downweighted_fraction": q.downweighted_fraction,
        "rejected_fraction": q.rejected_fraction,
        "median_weight": q.median_weight,
        "median_abs_residual_rad": q.median_abs_residual_rad,
        "outlier_score": q.outlier_score,
        "flagged_echoes": q.flagged.iter().map(|e| e + 1).collect::<Vec<_>>(),
    })
}

/// The field mapping this module implemented itself before qsm-core could (QSMxT#252): per-echo
/// Laplacian unwrapping of the masked phase, per-echo constant alignment, the configured echo
/// combination including assumed-decay weighting, and qsm-core's offset-removal ROMEO path with
/// that weighting. Kept, test-only, as the reference [`super::run_field_mapping`] must reproduce.
#[cfg(test)]
#[allow(dead_code)]
pub(crate) mod legacy {
    use qsm_core::pipeline::config::{
        B0EstimationMethod, FieldMappingConfig, PipelineError, ScanMetadata, UnwrappingAlgorithm,
    };
    use qsm_core::Grid;

    /// Poisson solver for Laplacian unwrapping, with its parameters (`field_mapping.laplacian_*`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Solver {
        /// Unweighted least squares, Neumann/DCT solve (Ghiglia & Romero 1994; QSM.jl `:dct`).
        Dct,
        /// Schofield & Zhu (2003) sin/cos Laplacian, FFT solve on the volume zero-padded by `pad`
        /// voxels per side (x, y, z): STI Suite 3.0's `MRPhaseUnwrap` (UK Biobank: pad 64).
        Fft { pad: [usize; 3] },
    }

    impl std::fmt::Display for Solver {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Solver::Dct => write!(f, "dct"),
                Solver::Fft { pad: [x, y, z] } if x == y && y == z => write!(f, "fft (pad {x})"),
                Solver::Fft { pad: [x, y, z] } => write!(f, "fft (pad {x} {y} {z})"),
            }
        }
    }

    impl Solver {
        pub fn from_config(cfg: &qsmxt_config::FieldMappingConfig) -> Self {
            match cfg.laplacian_solver {
                qsmxt_config::LaplacianSolver::Dct => Solver::Dct,
                qsmxt_config::LaplacianSolver::Fft => Solver::Fft { pad: cfg.laplacian_fft_pad },
            }
        }

        /// qsm-core's form of this solver.
        pub fn to_core(self) -> qsm_core::unwrap::LaplacianSolver {
            match self {
                Solver::Dct => qsm_core::unwrap::LaplacianSolver::Dct,
                Solver::Fft { pad } => qsm_core::unwrap::LaplacianSolver::Fft { pad },
            }
        }
    }

    /// How echoes are weighted in a weighted-average B0 estimate.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum EchoWeighting {
        /// One of qsm-core's weight types (`config.b0_weight_type`).
        Core(qsm_core::utils::B0WeightType),
        /// Fixed per-echo weights from the echo times and an *assumed* T2* (seconds): UK Biobank's
        /// echo combination (Wang et al. 2022, Nat. Neurosci. 25:818; T2* = 40 ms for everyone).
        ///
        /// UKB averages the unwrapped *phases* with `Wₑ = TEₑ·exp(−TEₑ/T2*)` and divides by the
        /// equally weighted mean TE, i.e. `f = Σ Wₑ φₑ / Σ Wₑ TEₑ`. In the phase/TE form every other
        /// weight type uses (`f = Σ wₑ (φₑ/TEₑ) / Σ wₑ`) that is `wₑ = TEₑ² · exp(−TEₑ/T2*)`. No T2*
        /// map and no magnitude enter it, so every voxel gets the same echo weights. It is a
        /// heuristic, not the inverse-variance weighting for that decay; `PhaseSNR`, which uses the
        /// measured magnitude in each voxel, is generally preferable when magnitude is available.
        AssumedDecay { t2star_s: f64 },
    }

    impl EchoWeighting {
        /// The weighting `field_mapping.b0_weight_type` asks for.
        pub fn from_config(cfg: &qsmxt_config::FieldMappingConfig) -> Self {
            use qsm_core::utils::B0WeightType as W;
            use qsmxt_config::B0WeightType as C;
            match cfg.b0_weight_type {
                C::PhaseSNR => EchoWeighting::Core(W::PhaseSNR),
                C::PhaseVar => EchoWeighting::Core(W::PhaseVar),
                C::Average => EchoWeighting::Core(W::Average),
                C::TEs => EchoWeighting::Core(W::TEs),
                C::Mag => EchoWeighting::Core(W::Mag),
                C::AssumedDecay => EchoWeighting::AssumedDecay { t2star_s: cfg.b0_weight_assumed_t2star_ms * 1e-3 },
            }
        }

        /// Per-echo weight in the phase/TE form, `f = Σ w (φ/TE) / Σ w`.
        fn weight(self, te: f64, mag: f64) -> f64 {
            use qsm_core::utils::B0WeightType as W;
            match self {
                EchoWeighting::Core(W::PhaseSNR) => mag * te,
                EchoWeighting::Core(W::PhaseVar) => mag * mag * te * te,
                EchoWeighting::Core(W::Average) => 1.0,
                EchoWeighting::Core(W::TEs) => te,
                EchoWeighting::Core(W::Mag) => mag,
                EchoWeighting::Core(W::AssumedDecay { t2star_s }) |
                EchoWeighting::AssumedDecay { t2star_s } => te * te * (-te / t2star_s).exp(),
            }
        }
    }

    /// Unwrap one echo with the Laplacian method, for field mapping.
    ///
    /// The phase is zeroed outside `mask` first, as STI Suite's `MRPhaseUnwrap` is called by UK
    /// Biobank (`mask .* phase`): the Poisson solve is global, so noise outside the brain otherwise
    /// leaks into it. The result is defined up to a constant and is zero outside `mask`.
    ///
    /// This is the one place the field-mapping Laplacian unwrap is called.
    pub fn unwrap_echo_laplacian(phase: &[f64], mask: &[u8], grid: &Grid, solver: Solver) -> Vec<f64> {
        let masked: Vec<f64> = phase.iter().zip(mask).map(|(&p, &m)| if m != 0 { p } else { 0.0 }).collect();
        qsm_core::unwrap::laplacian_unwrap(&masked, mask, grid, solver.to_core())
    }

    /// Subtract the mean over `mask` from `values` inside `mask`.
    ///
    /// A Laplacian unwrap fixes each echo only up to its own constant. Unwrapped correctly, echo `e`
    /// inside the mask is `TEₑ·ω(r) + cₑ`; its masked mean is `TEₑ·ω̄ + cₑ`, so subtracting it leaves
    /// `TEₑ·(ω − ω̄)` for every echo — consistent across echoes, which is what any weighting whose
    /// relative echo weights vary over the brain (phase-snr, phase-var, mag) needs, since otherwise
    /// the arbitrary `cₑ` turn into a spatially varying (magnitude-shaped) field offset. The global
    /// ω̄ it removes is a constant, which background removal and referencing remove anyway.
    pub fn remove_masked_mean(values: &mut [f64], mask: &[u8]) {
        let (sum, n) = values.iter().zip(mask).filter(|(_, &m)| m != 0)
            .fold((0.0, 0usize), |(s, n), (&v, _)| (s + v, n + 1));
        if n == 0 {
            return;
        }
        let mean = sum / n as f64;
        for (v, &m) in values.iter_mut().zip(mask) {
            if m != 0 {
                *v -= mean;
            }
        }
    }

    /// Weighted average of `unwrapped/TE` across echoes, in Hz. Zero outside `mask`.
    pub fn weighted_b0_hz(
        unwrapped: &[Vec<f64>], mags: &[&[f64]], tes: &[f64], mask: &[u8], weighting: EchoWeighting,
    ) -> Vec<f64> {
        let n = mask.len();
        let mut b0 = vec![0.0; n];
        for i in 0..n {
            if mask[i] == 0 {
                continue;
            }
            let (mut num, mut den) = (0.0, 0.0);
            for (e, &te) in tes.iter().enumerate() {
                let w = weighting.weight(te, mags[e][i]);
                num += w * unwrapped[e][i] / te;
                den += w;
            }
            if den > 1e-10 {
                b0[i] = num / den / std::f64::consts::TAU;
            }
        }
        b0
    }

    /// Combine unwrapped echoes (rad) into a B0 field in Hz with the configured estimator.
    fn combine_echoes(
        unwrapped: &[Vec<f64>], mags: &[&[f64]], tes: &[f64], mask: &[u8],
        config: &FieldMappingConfig, weighting: EchoWeighting,
    ) -> Vec<f64> {
        if unwrapped.len() == 1 {
            // One echo: every estimator is φ/TE (a fit with an intercept would be degenerate).
            return weighted_b0_hz(unwrapped, mags, tes, mask, EchoWeighting::Core(qsm_core::utils::B0WeightType::Average));
        }
        match &config.b0_estimation {
            B0EstimationMethod::WeightedAvg { .. } => weighted_b0_hz(unwrapped, mags, tes, mask, weighting),
            B0EstimationMethod::LinearFit(p) => {
                let fit = qsm_core::utils::multi_echo_linear_fit(
                    unwrapped, mags, tes, mask, p.estimate_offset, p.reliability_threshold_percentile,
                );
                qsm_core::utils::field_to_hz(&fit.field)
            }
            B0EstimationMethod::RobustFit(_) => unreachable!("run_field_mapping sends robust-fit to qsm-core"),
        }
    }

    fn validate(
        phases: &[&[f64]], mags: Option<&[&[f64]]>, mask: &[u8], meta: &ScanMetadata, weighting: EchoWeighting,
    ) -> Result<(), PipelineError> {
        if let EchoWeighting::AssumedDecay { t2star_s } = weighting {
            if !(t2star_s.is_finite() && t2star_s > 0.0) {
                return Err(PipelineError::InvalidConfig(format!("assumed T2* for assumed-decay weighting must be positive, got {} s", t2star_s)));
            }
        }
        let n = meta.dims.0 * meta.dims.1 * meta.dims.2;
        if phases.is_empty() {
            return Err(PipelineError::InvalidInput("no phase echoes provided".into()));
        }
        if meta.echo_times.len() != phases.len() {
            return Err(PipelineError::DimensionMismatch { expected: phases.len(), got: meta.echo_times.len() });
        }
        if let Some(m) = mags {
            if m.len() != phases.len() {
                return Err(PipelineError::DimensionMismatch { expected: phases.len(), got: m.len() });
            }
        }
        let lens = phases.iter().chain(mags.unwrap_or(&[])).map(|v| v.len()).chain(std::iter::once(mask.len()));
        if let Some(got) = lens.into_iter().find(|&l| l != n) {
            return Err(PipelineError::DimensionMismatch { expected: n, got });
        }
        if meta.echo_times.iter().any(|&te| !te.is_finite() || te <= 0.0) {
            return Err(PipelineError::InvalidInput("echo times must be positive".into()));
        }
        Ok(())
    }

    /// Per-echo Laplacian unwrapping followed by the configured echo combination. Returns Hz.
    fn laplacian_field_hz(
        phases: &[&[f64]], mags: &[&[f64]], mask: &[u8], meta: &ScanMetadata,
        config: &FieldMappingConfig, weighting: EchoWeighting, solver: Solver,
    ) -> Vec<f64> {
        let grid = grid(meta);
        let unwrapped: Vec<Vec<f64>> = phases.iter().map(|p| {
            let mut u = unwrap_echo_laplacian(p, mask, &grid, solver);
            remove_masked_mean(&mut u, mask);
            u
        }).collect();
        combine_echoes(&unwrapped, mags, &meta.echo_times, mask, config, weighting)
    }

    /// qsm-core's offset-removal ROMEO path, with this module's weighted average. Returns Hz.
    fn romeo_offset_field_hz(
        phases: &[&[f64]], mags: &[&[f64]], mask: &[u8], meta: &ScanMetadata,
        config: &FieldMappingConfig, weighting: EchoWeighting,
    ) -> Vec<f64> {
        let grid = grid(meta);
        let tes = &meta.echo_times;
        let (mut corrected, _) = qsm_core::utils::phase_offset_removal(
            phases, mags, tes, mask, config.phase_offset_sigma, [0, 1],
            qsm_core::unwrap::UnwrapMethod::Romeo, &grid,
        );
        if config.bipolar_correction && phases.len() >= 3 {
            qsm_core::utils::bipolar_correction(&mut corrected, mags, tes, mask, config.phase_offset_sigma, &grid);
        }
        let unwrapped = qsm_core::unwrap::unwrap_romeo_multi_echo(&corrected, mags, tes, mask, &config.romeo_params, &grid);
        combine_echoes(&unwrapped, mags, tes, mask, config, weighting)
    }

    fn grid(meta: &ScanMetadata) -> Grid {
        let (nx, ny, nz) = meta.dims;
        let (vx, vy, vz) = meta.voxel_size;
        Grid::new(nx, ny, nz, vx, vy, vz)
    }

    /// Run field mapping: per-echo wrapped phase (rad, in [−π, π]) → B0 field in ppm, zero outside
    /// `mask`. `magnitudes` = `None` weights every echo and voxel equally.
    ///
    /// `weighting` replaces the weighting in `config.b0_estimation` (used only with
    /// `WeightedAvg`). `solver` is the Poisson
    /// solver for Laplacian unwrapping (ignored with ROMEO).
    pub fn run_field_mapping(
        phases: &[&[f64]],
        magnitudes: Option<&[&[f64]]>,
        mask: &[u8],
        meta: &ScanMetadata,
        config: &FieldMappingConfig,
        weighting: EchoWeighting,
        solver: Solver,
    ) -> Result<Vec<f64>, PipelineError> {
        validate(phases, magnitudes, mask, meta, weighting)?;
        if matches!(config.b0_estimation, B0EstimationMethod::RobustFit(_)) {
            let mut core = config.clone();
            core.laplacian_solver = solver.to_core();
            return qsm_core::pipeline::run_field_mapping(phases, magnitudes, mask, meta, &core, &mut |_, _| {})
                .map(|r| r.b0_field_ppm);
        }
        let n = mask.len();
        let ones = vec![1.0; n];
        let uniform: Vec<&[f64]> = phases.iter().map(|_| ones.as_slice()).collect();
        let mags = magnitudes.unwrap_or(&uniform);
        let n_echoes = phases.len();
        let weighted_avg = matches!(config.b0_estimation, B0EstimationMethod::WeightedAvg { .. });

        let hz = match (config.unwrapping_algorithm, weighting) {
            (UnwrappingAlgorithm::Laplacian, _) => laplacian_field_hz(phases, mags, mask, meta, config, weighting, solver),
            (UnwrappingAlgorithm::Romeo, EchoWeighting::AssumedDecay { .. })
                if n_echoes > 1 && weighted_avg && config.phase_offset_removal =>
            {
                romeo_offset_field_hz(phases, mags, mask, meta, config, weighting)
            }
            (UnwrappingAlgorithm::Romeo, w) => {
                // qsm-core's ROMEO path without offset removal honours the estimator and weighting
                // (QSM.rs#162), assumed-decay included.
                let mut core = config.clone();
                if let B0EstimationMethod::WeightedAvg { weighting } = &mut core.b0_estimation {
                    *weighting = match w {
                        EchoWeighting::Core(t) => t,
                        EchoWeighting::AssumedDecay { t2star_s } => qsm_core::utils::B0WeightType::AssumedDecay { t2star_s },
                    };
                }
                return qsm_core::pipeline::run_field_mapping(phases, magnitudes, mask, meta, &core, &mut |_, _| {})
                    .map(|r| r.b0_field_ppm);
            }
        };
        Ok(qsm_core::pipeline::hz_to_ppm(&hz, meta.field_strength))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::legacy::{remove_masked_mean, weighted_b0_hz, EchoWeighting};
    use qsm_core::pipeline::config::UnwrappingAlgorithm;
    use qsm_core::unwrap::LaplacianSolver;
    use qsm_core::utils::B0WeightType as W;
    use std::f64::consts::PI;

    const N: usize = 24;
    const DCT: LaplacianSolver = LaplacianSolver::Dct;
    const FFT: LaplacianSolver = LaplacianSolver::Fft { pad: [64; 3] };

    /// The module's entry point with the weighting and solver set on `cfg` (the weighting only
    /// matters to `WeightedAvg`), returning ppm.
    fn run(
        ph: &[&[f64]], mg: Option<&[&[f64]]>, mask: &[u8], meta: &ScanMetadata,
        cfg: &FieldMappingConfig, w: W, solver: LaplacianSolver,
    ) -> Result<Vec<f64>, PipelineError> {
        let mut cfg = cfg.clone();
        if let B0EstimationMethod::WeightedAvg { weighting } = &mut cfg.b0_estimation {
            *weighting = w;
        }
        cfg.laplacian_solver = solver;
        run_field_mapping(ph, mg, mask, meta, &cfg).map(|r| r.b0_field_ppm)
    }

    fn meta(tes: &[f64]) -> ScanMetadata {
        ScanMetadata {
            dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), echo_times: tes.to_vec(),
            field_strength: 3.0, b0_direction: (0.0, 0.0, 1.0), slice_geometry: None,
        }
    }

    /// A smooth frequency map (rad/s) that wraps several times at the later echoes: a sum of
    /// cosine (Neumann) modes, which the DCT Laplacian unwrap reproduces exactly on a full mask.
    fn omega(i: usize, j: usize, k: usize) -> f64 {
        let c = |x: usize, m: f64| (PI * m * (x as f64 + 0.5) / N as f64).cos();
        900.0 * c(i, 1.0) + 600.0 * c(j, 1.0) * c(k, 2.0)
    }

    /// A frequency map that wraps inside the sphere but falls smoothly to zero at its edge, so the
    /// masked phase has no jump there and a masked unwrap is exact too.
    fn omega_sphere(i: usize, j: usize, k: usize) -> f64 {
        let d = |x: usize| x as f64 + 0.5 - N as f64 / 2.0;
        let r = (d(i).powi(2) + d(j).powi(2) + d(k).powi(2)).sqrt() / (N as f64 / 2.0 - 2.0);
        if r < 1.0 { 600.0 * (PI * r / 2.0).cos().powi(2) } else { 0.0 }
    }

    fn wrap(x: f64) -> f64 { (x + PI).rem_euclid(2.0 * PI) - PI }

    fn sphere_mask() -> Vec<u8> {
        let r = N as f64 / 2.0 - 2.0;
        let mut m = vec![0u8; N * N * N];
        for k in 0..N { for j in 0..N { for i in 0..N {
            let d = |x: usize| x as f64 + 0.5 - N as f64 / 2.0;
            if d(i).powi(2) + d(j).powi(2) + d(k).powi(2) < r * r { m[i + j * N + k * N * N] = 1; }
        }}}
        m
    }

    /// Per-echo wrapped phase `TE·ω`, plus magnitude decaying with T2* = 30 ms.
    fn echoes(tes: &[f64]) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) { echoes_of(tes, omega) }

    fn echoes_of(tes: &[f64], omega: fn(usize, usize, usize) -> f64) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
        let n = N * N * N;
        let mut ph = vec![vec![0.0; n]; tes.len()];
        let mut mg = vec![vec![0.0; n]; tes.len()];
        for k in 0..N { for j in 0..N { for i in 0..N {
            let idx = i + j * N + k * N * N;
            for (e, &te) in tes.iter().enumerate() {
                ph[e][idx] = wrap(te * omega(i, j, k));
                mg[e][idx] = (1.0 + 0.5 * (i as f64 / N as f64)) * (-te / 0.030).exp();
            }
        }}}
        (ph, mg)
    }

    fn lap_config() -> FieldMappingConfig {
        FieldMappingConfig { unwrapping_algorithm: UnwrappingAlgorithm::Laplacian, ..Default::default() }
    }

    fn refs(v: &[Vec<f64>]) -> Vec<&[f64]> { v.iter().map(|x| x.as_slice()).collect() }

    fn demeaned(v: &[f64], mask: &[u8]) -> Vec<f64> {
        let mut v = v.to_vec();
        remove_masked_mean(&mut v, mask);
        v
    }

    fn max_abs_diff(a: &[f64], b: &[f64], mask: &[u8]) -> f64 {
        a.iter().zip(b).zip(mask).filter(|(_, &m)| m != 0).map(|((x, y), _)| (x - y).abs()).fold(0.0, f64::max)
    }

    fn truth_ppm(mask: &[u8]) -> Vec<f64> { truth_of(mask, omega) }

    fn truth_of(mask: &[u8], omega: fn(usize, usize, usize) -> f64) -> Vec<f64> {
        let mut t = vec![0.0; N * N * N];
        for k in 0..N { for j in 0..N { for i in 0..N {
            t[i + j * N + k * N * N] = omega(i, j, k) / (2.0 * PI);
        }}}
        let t = qsm_core::pipeline::hz_to_ppm(&t, 3.0);
        demeaned(&t, mask)
    }

    #[test]
    fn assumed_decay_weight_matches_ukb_formula() {
        // UKB: f = (W1 φ1 + W2 φ2) / (W1 TE1 + W2 TE2), W = TE·exp(−TE/T2*), in rad/s.
        let tes = [0.00942, 0.0197];
        let (p1, p2) = (0.7, 1.9);
        let w: Vec<f64> = tes.iter().map(|&t: &f64| t * (-t / 0.040).exp()).collect();
        let ukb = (w[0] * p1 + w[1] * p2) / (w[0] * tes[0] + w[1] * tes[1]) / (2.0 * PI);
        let uw = vec![vec![p1], vec![p2]];
        let ones = [1.0];
        let got = weighted_b0_hz(&uw, &[&ones, &ones], &tes, &[1], EchoWeighting::AssumedDecay { t2star_s: 0.040 });
        assert!((got[0] - ukb).abs() < 1e-12, "{} vs {}", got[0], ukb);
    }

    const ALL_WEIGHTINGS: [W; 6] = [
        W::PhaseSNR, W::PhaseVar, W::Average,
        W::TEs, W::Mag, W::AssumedDecay { t2star_s: 0.040 },
    ];

    #[test]
    fn laplacian_recovers_wrapped_field_with_every_weighting() {
        let tes = [0.004, 0.009, 0.014];
        for (mask, om) in [(vec![1u8; N * N * N], omega as fn(usize, usize, usize) -> f64), (sphere_mask(), omega_sphere)] {
            let (ph, mg) = echoes_of(&tes, om);
            let truth = truth_of(&mask, om);
            for w in ALL_WEIGHTINGS {
                let f = run(&refs(&ph), Some(&refs(&mg)), &mask, &meta(&tes), &lap_config(), w, DCT).unwrap();
                let err = max_abs_diff(&demeaned(&f, &mask), &truth, &mask);
                assert!(err < 1e-6, "{w:?}: max error {err} ppm");
                assert!(f.iter().zip(&mask).all(|(&v, &m)| m != 0 || v == 0.0), "{w:?}: nonzero outside mask");
            }
        }
    }

    #[test]
    fn laplacian_honours_b0_weight_type() {
        // The regression: on the Laplacian path every --b0-weight-type gave bit-identical output.
        let tes = [0.00942, 0.0197];
        let (mut ph, mg) = echoes(&tes);
        // Echo-dependent noise, so that the weighting has something to choose between.
        let mut s = 12345u64;
        for p in ph.iter_mut() {
            for v in p.iter_mut() {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                *v = wrap(*v + 0.3 * ((s >> 33) as f64 / (1u64 << 31) as f64 - 0.5));
            }
        }
        let mask = sphere_mask();
        let run = |w| run(&refs(&ph), Some(&refs(&mg)), &mask, &meta(&tes), &lap_config(), w, DCT).unwrap();
        let a = run(W::PhaseSNR);
        let b = run(W::TEs);
        let c = run(W::AssumedDecay { t2star_s: 0.040 });
        assert!(max_abs_diff(&a, &b, &mask) > 1e-4);
        assert!(max_abs_diff(&b, &c, &mask) > 1e-4);
    }

    #[test]
    fn assumed_decay_with_infinite_t2star_is_phase_var_without_magnitude() {
        // TE²·exp(−TE/∞) = TE² = mag²·TE² at unit magnitude: checks the weight on both paths,
        // and on the ROMEO path that the reproduction of qsm-core's offset-removal path matches it.
        let tes = [0.004, 0.009, 0.014];
        let (ph, _) = echoes(&tes);
        let mask = sphere_mask();
        let inf = W::AssumedDecay { t2star_s: 1e30 };
        for alg in [UnwrappingAlgorithm::Laplacian, UnwrappingAlgorithm::Romeo] {
            let cfg = FieldMappingConfig {
                unwrapping_algorithm: alg,
                b0_estimation: B0EstimationMethod::WeightedAvg { weighting: W::PhaseVar },
                ..Default::default()
            };
            let a = run(&refs(&ph), None, &mask, &meta(&tes), &cfg, inf, DCT).unwrap();
            let b = run(&refs(&ph), None, &mask, &meta(&tes), &cfg, W::PhaseVar, DCT).unwrap();
            assert!(max_abs_diff(&a, &b, &mask) < 1e-9, "{alg:?}");
        }
        let cfg = FieldMappingConfig {
            b0_estimation: B0EstimationMethod::WeightedAvg { weighting: W::PhaseVar },
            ..Default::default()
        };
        let ours = run(&refs(&ph), None, &mask, &meta(&tes), &cfg, inf, DCT).unwrap();
        let core = qsm_core::pipeline::run_field_mapping(&refs(&ph), None, &mask, &meta(&tes), &cfg, &mut |_, _| {}).unwrap();
        assert!(max_abs_diff(&ours, &core.b0_field_ppm, &mask) < 1e-9);
    }

    #[test]
    fn single_echo_laplacian_is_phase_over_te() {
        let tes = [0.008];
        let (ph, _) = echoes(&tes);
        let mask = vec![1u8; N * N * N];
        let mut cfg = lap_config();
        cfg.b0_estimation = B0EstimationMethod::LinearFit(Default::default()); // must not degenerate to zero
        let f = run(&refs(&ph), None, &mask, &meta(&tes), &cfg, W::PhaseSNR, DCT).unwrap();
        assert!(max_abs_diff(&demeaned(&f, &mask), &truth_ppm(&mask), &mask) < 1e-6);
    }

    #[test]
    fn masking_the_phase_keeps_outside_noise_out() {
        // Pure noise outside the mask must not change the field inside it.
        let tes = [0.004, 0.009];
        let (ph, _) = echoes(&tes);
        let mask = sphere_mask();
        let mut noisy = ph.clone();
        let mut s = 7u64;
        for p in noisy.iter_mut() {
            for (v, &m) in p.iter_mut().zip(&mask) {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                if m == 0 { *v = ((s >> 33) as f64 / (1u64 << 31) as f64 - 0.5) * 2.0 * PI; }
            }
        }
        let w = W::AssumedDecay { t2star_s: 0.040 };
        let a = run(&refs(&ph), None, &mask, &meta(&tes), &lap_config(), w, DCT).unwrap();
        let b = run(&refs(&noisy), None, &mask, &meta(&tes), &lap_config(), w, DCT).unwrap();
        assert!(max_abs_diff(&a, &b, &mask) < 1e-12);
    }

    #[test]
    fn fft_solver_recovers_wrapped_field() {
        // The masked phase falls smoothly to zero at the sphere edge, so the padded FFT solve
        // recovers it (up to a constant), approximately: the FFT solver's kernel is a continuous-k one.
        let tes = [0.004, 0.009, 0.014];
        let mask = sphere_mask();
        let (ph, _) = echoes_of(&tes, omega_sphere);
        let f = run(&refs(&ph), None, &mask, &meta(&tes), &lap_config(),
                                  W::AssumedDecay { t2star_s: 0.040 }, FFT).unwrap();
        let truth = truth_of(&mask, omega_sphere);
        let rms_truth = (truth.iter().zip(&mask).filter(|(_, &m)| m != 0).map(|(t, _)| t * t).sum::<f64>()
            / mask.iter().filter(|&&m| m != 0).count() as f64).sqrt();
        let err = max_abs_diff(&demeaned(&f, &mask), &truth, &mask);
        assert!(err < 0.2 * rms_truth, "max error {err} vs field rms {rms_truth}");
    }

    /// The gate for handing field mapping to qsm-core: on every path QSMxT used to compute
    /// itself (Laplacian with either solver, assumed-decay weighting on ROMEO's offset-removal
    /// path, single echoes) and the ones it already delegated, qsm-core's result is the old one.
    #[test]
    fn qsm_core_reproduces_the_old_implementation() {
        let romeo_direct = FieldMappingConfig { phase_offset_removal: false, ..Default::default() };
        let pad = [8, 8, 6];
        let paths: [(&str, FieldMappingConfig, LaplacianSolver, legacy::Solver); 4] = [
            ("laplacian dct", lap_config(), DCT, legacy::Solver::Dct),
            ("laplacian fft", lap_config(), LaplacianSolver::Fft { pad }, legacy::Solver::Fft { pad }),
            ("romeo + offset removal", FieldMappingConfig::default(), DCT, legacy::Solver::Dct),
            ("romeo direct", romeo_direct, DCT, legacy::Solver::Dct),
        ];
        let lf = |estimate_offset| B0EstimationMethod::LinearFit(qsm_core::utils::LinearFitParams {
            estimate_offset, ..Default::default()
        });
        let mut estimations: Vec<(B0EstimationMethod, EchoWeighting)> = ALL_WEIGHTINGS.iter().map(|&w| (
            B0EstimationMethod::WeightedAvg { weighting: w },
            match w { W::AssumedDecay { t2star_s } => EchoWeighting::AssumedDecay { t2star_s }, w => EchoWeighting::Core(w) },
        )).collect();
        estimations.push((lf(true), EchoWeighting::Core(W::PhaseSNR)));
        estimations.push((lf(false), EchoWeighting::Core(W::PhaseSNR)));

        let mut worst = 0.0f64;
        for tes in [&[0.008][..], &[0.004, 0.009][..], &[0.004, 0.009, 0.014][..]] {
            let (mut ph, mg) = echoes_of(tes, omega_sphere);
            let mut s = 99u64;
            for p in ph.iter_mut() {
                for v in p.iter_mut() {
                    s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    *v = wrap(*v + 0.2 * ((s >> 33) as f64 / (1u64 << 31) as f64 - 0.5));
                }
            }
            let (ph, mg) = (refs(&ph), refs(&mg));
            for mask in [vec![1u8; N * N * N], sphere_mask()] {
                for mags in [Some(&mg[..]), None] {
                    for (label, base, solver, old_solver) in &paths {
                        for (est, old_w) in &estimations {
                            // The old ROMEO path computed only assumed-decay itself (the rest
                            // went to qsm-core already), and magnitude-free runs need one estimator.
                            let assumed = matches!(old_w, EchoWeighting::AssumedDecay { .. });
                            if (label.starts_with("romeo") && !assumed)
                                || (mags.is_none() && !matches!(est, B0EstimationMethod::WeightedAvg { weighting: W::PhaseSNR })) {
                                continue;
                            }
                            let cfg = FieldMappingConfig { b0_estimation: est.clone(), laplacian_solver: *solver, ..base.clone() };
                            let new = run_field_mapping(&ph, mags, &mask, &meta(tes), &cfg).unwrap().b0_field_ppm;
                            let old = legacy::run_field_mapping(&ph, mags, &mask, &meta(tes), &cfg, *old_w, *old_solver).unwrap();
                            let d = max_abs_diff(&new, &old, &mask);
                            assert!(d < 1e-9, "{label}, {} echoes, {est:?}, mags {}: differs by {d} ppm",
                                    tes.len(), mags.is_some());
                            worst = worst.max(d);
                        }
                    }
                }
            }
        }
        eprintln!("largest difference from the old implementation: {worst:e} ppm");
    }

    /// `robust-fit` reaches qsm-core's robust fit, returns its report (which flags a corrupted
    /// echo), and refuses a series too short for it rather than quietly fitting a line.
    #[test]
    fn robust_fit_reports_and_flags_a_corrupted_echo() {
        let tes = [0.004, 0.008, 0.012, 0.016];
        let (mut ph, mut mg) = echoes_of(&tes, omega_sphere);
        let mask = sphere_mask();
        // Echo 3: a slab of signal loss with a phase error, as motion leaves.
        for idx in 0..N * N * N {
            if idx / (N * N) < N / 2 {
                ph[2][idx] = wrap(ph[2][idx] + 0.8);
                mg[2][idx] *= 0.3;
            }
        }
        let cfg = FieldMappingConfig {
            b0_estimation: B0EstimationMethod::RobustFit(Default::default()),
            phase_offset_removal: false,
            ..Default::default()
        };
        let r = run_field_mapping(&refs(&ph), Some(&refs(&mg)), &mask, &meta(&tes), &cfg).unwrap();
        let report = r.robust_fit.expect("robust fit reports");
        assert_eq!(report.quality.flagged, vec![2], "{:?}", report.quality);
        let json = robust_fit_report_json(&report);
        assert_eq!(json["flagged_echoes"], serde_json::json!([3]), "1-based in the sidecar");
        // Other estimators report nothing.
        let plain = run_field_mapping(&refs(&ph), Some(&refs(&mg)), &mask, &meta(&tes), &FieldMappingConfig::default()).unwrap();
        assert!(plain.robust_fit.is_none());
        // Three echoes with an offset term is too few.
        let three = run_field_mapping(&refs(&ph[..3]), Some(&refs(&mg[..3])), &mask, &meta(&tes[..3]), &cfg);
        assert!(matches!(three, Err(PipelineError::InvalidConfig(_))), "three echoes must be refused");
    }

    #[test]
    fn rejects_bad_inputs() {
        let n = N * N * N;
        let p = vec![0.0; n];
        let mask = vec![1u8; n];
        let w = W::PhaseSNR;
        assert!(run(&[], None, &mask, &meta(&[]), &lap_config(), w, DCT).is_err());
        assert!(run(&[&p], None, &mask, &meta(&[0.01, 0.02]), &lap_config(), w, DCT).is_err());
        assert!(run(&[&p[1..]], None, &mask, &meta(&[0.01]), &lap_config(), w, DCT).is_err());
        assert!(run(&[&p], None, &mask, &meta(&[0.0]), &lap_config(), w, DCT).is_err());
        let t0 = W::AssumedDecay { t2star_s: 0.0 };
        assert!(run(&[&p], None, &mask, &meta(&[0.01]), &lap_config(), t0, DCT).is_err());
    }
}
