//! Inhomogeneity (bias field) correction, and the combined magnitude.
//!
//! The correction is one step (`inhomog`), run for what its consumers ask of it ([`targets`]):
//! the echoes masking reads (echo 1 for a first-echo recipe, every echo for the RSS or phase
//! quality), every echo for the combined magnitude, and QSMART's vessel magnitude (the echo mean,
//! corrected with N4 as QSMART does). Nothing asks, nothing runs. The combined magnitude is the
//! root sum of squares of the corrected echoes, the same image masking's RSS sections see.

use serde::Serialize;

use crate::config::PipelineConfig;
use crate::enums::{InhomogeneityMethod, N4Geometry};
use crate::error::EngineError;
use crate::masking::{MaskOp, MaskSection, MaskingInput};
use crate::scan::RunMetadata;

/// What the `inhomog` step corrects in a run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Targets {
    /// The echoes (1-based, ascending) whose magnitude is corrected with the configured method
    /// (`masking.inhomogeneity_method`).
    pub echoes: Vec<usize>,
    /// QSMART's vessel magnitude: the mean of every echo's magnitude, corrected with N4 on
    /// make_nii's geometry with N4's default settings, as QSMART does
    /// ([`qsm_core::utils::qsmart_vessel_magnitude`]).
    pub vessel: bool,
}

impl Targets {
    /// Whether nothing asks for a correction: the step is not planned.
    pub fn is_empty(&self) -> bool {
        self.echoes.is_empty() && !self.vessel
    }

    /// Add `echoes` to those corrected.
    pub fn add(&mut self, echoes: impl IntoIterator<Item = usize>) {
        self.echoes.extend(echoes);
        self.echoes.sort_unstable();
        self.echoes.dedup();
    }
}

/// What the planned steps (`has`) ask the correction for, on a scan `facts` describes:
/// - with the correction on, the echoes the brain mask reads, unless it is supplied (and the
///   reliable-phase mask's), and every echo for the combined magnitude;
/// - QSMART's vessel magnitude (`mean-n4`), unless the echoes are motion-corrected after masking:
///   QSMART then makes it from those itself.
pub fn targets(
    config: &PipelineConfig, facts: &crate::scan::ScanFacts, has: impl Fn(crate::plan::Stage) -> bool,
    motion_corrected: bool, supplied_mask: bool,
) -> Targets {
    use crate::plan::{Pass, Stage};
    let mut t = Targets::default();
    if !facts.has_magnitude {
        return t;
    }
    let (m, n) = (&config.masking, facts.n_echoes);
    if m.inhomogeneity_correction {
        if has(Stage::Mask(Pass::Main)) && !supplied_mask {
            t.add(mask_echoes(&m.sections, &m.refinements, n));
        }
        if has(Stage::Mask(Pass::Reliable)) {
            t.add(mask_echoes(&m.resolved_two_pass_sections(), &m.refinements, n));
        }
        if has(Stage::Magnitude) {
            t.add(1..=n);
        }
    }
    t.vessel = !motion_corrected
        && config.inversion.qsmart.vessel_magnitude == crate::enums::QsmartVesselMagnitude::MeanN4
        && (has(Stage::Qsmart(Pass::Main)) || has(Stage::Qsmart(Pass::Reliable)));
    t
}

/// Whether the refinements after combining read the RSS magnitude: any but the purely
/// morphological ones (erosion, dilation, closing, the majority filter, hole filling, smoothing),
/// which read only the mask.
pub fn refinements_read_magnitude(refinements: &[MaskOp]) -> bool {
    refinements.iter().any(|op| !matches!(op,
        MaskOp::Erode { .. } | MaskOp::Dilate { .. } | MaskOp::Close { .. } | MaskOp::Majority
        | MaskOp::FillHoles { .. } | MaskOp::GaussianSmooth { .. }))
}

/// The echoes (1-based, ascending) a mask of `sections`, then `refinements`, reads the magnitude
/// of, on a scan of `n` echoes: echo 1 for a first-echo section, the last for a last-echo one,
/// every echo for an RSS or phase-quality section (qsm-core's `section_magnitude`) and for
/// refinements that read the magnitude ([`refinements_read_magnitude`]).
pub fn mask_echoes(sections: &[MaskSection], refinements: &[MaskOp], n: usize) -> Vec<usize> {
    let mut echoes = Vec::new();
    let all = || (1..=n).collect::<Vec<_>>();
    for s in sections {
        match s.input {
            MaskingInput::MagnitudeFirst => echoes.push(1),
            MaskingInput::MagnitudeLast => echoes.push(n),
            MaskingInput::Magnitude | MaskingInput::PhaseQuality => echoes.extend(all()),
        }
    }
    if refinements_read_magnitude(refinements) {
        echoes.extend(all());
    }
    echoes.retain(|&e| e >= 1 && e <= n);
    echoes.sort_unstable();
    echoes.dedup();
    echoes
}

/// The magnitudes masking reads, for `sections` and the configured refinements: each echo of
/// `magnitudes` that the mask reads ([`mask_echoes`]) corrected for inhomogeneity when that is on,
/// the others as they are (no section reads them). What a mask preview builds from the echoes;
/// the run's mask step reads the same images from the `inhomog` step.
pub fn masking_magnitudes(
    sections: &[MaskSection], config: &PipelineConfig, magnitudes: Vec<Vec<f64>>, meta: &RunMetadata,
) -> Result<Vec<Vec<f64>>, EngineError> {
    if !config.masking.inhomogeneity_correction || magnitudes.is_empty() {
        return Ok(magnitudes);
    }
    let wanted = mask_echoes(sections, &config.masking.refinements, magnitudes.len());
    magnitudes.into_iter().enumerate().map(|(i, m)| {
        if wanted.contains(&(i + 1)) {
            homogeneity_correct(&m, meta, config, &format!("echo {}'s magnitude", i + 1))
        } else {
            Ok(m)
        }
    }).collect()
}

/// The `inhomog` step's parameters: the correction's method and settings, and what it corrects.
pub fn step_params(config: &PipelineConfig, targets: &Targets) -> serde_json::Value {
    serde_json::json!({
        "correction": params(config),
        "echoes": targets.echoes,
        // QSMART's own N4 (fixed settings, make_nii geometry); versioned with it.
        "vessel": targets.vessel.then_some("qsmart-mean-n4"),
    })
}

/// The correction's method and that method's settings.
pub fn params(config: &PipelineConfig) -> serde_json::Value {
    match config.masking.inhomogeneity_method {
        InhomogeneityMethod::Makehomogeneous => serde_json::json!({
            "inhomogeneity_correction": config.masking.inhomogeneity_correction,
            "homogeneity_sigma_mm": config.homogeneity.sigma_mm,
            "homogeneity_nbox": config.homogeneity.nbox,
        }),
        InhomogeneityMethod::N4 => serde_json::json!({
            "inhomogeneity_correction": config.masking.inhomogeneity_correction,
            "inhomogeneity_method": "n4",
            "n4": config.n4,
            "n4_geometry": config.masking.n4_geometry,
        }),
    }
}

/// The run's combined magnitude: the root sum of squares of `echoes`, every echo's magnitude as
/// the `inhomog` step left it (corrected, when the correction is on).
pub fn combined_magnitude(echoes: &[&[f64]]) -> Result<Vec<f64>, EngineError> {
    Ok(qsm_core::pipeline::rss_combine(echoes)?)
}

/// QSMART's vessel magnitude from every echo's magnitude
/// ([`qsm_core::utils::qsmart_vessel_magnitude`]: their mean, corrected with N4 as QSMART does).
pub fn vessel_magnitude(echoes: &[&[f64]], meta: &RunMetadata) -> Result<Vec<f64>, EngineError> {
    let grid = crate::scan::scan_metadata(meta).grid();
    qsm_core::utils::qsmart_vessel_magnitude(echoes, &grid).map_err(|e| EngineError::Config(format!("qsmart: {e}")))
}

/// Apply inhomogeneity (bias field) correction on the run's working grid, with the configured
/// method (`masking.inhomogeneity_method`). `source` names the volume in the error.
///
/// qsm-core indexes the volume as `x + y*nx + z*nx*ny` over the grid dimensions, so a volume
/// with fewer voxels than the grid panics with an out-of-bounds index inside the box filter
/// rather than reporting which input is wrong (issue #184). `qsmxt` checks every input's size at
/// load time for a fresh run; this guard also covers runs resumed from a cached state.
pub fn homogeneity_correct(
    data: &[f64],
    meta: &RunMetadata,
    config: &PipelineConfig,
    source: &str,
) -> Result<Vec<f64>, EngineError> {
    let (nx, ny, nz) = meta.dims;
    let (vsx, vsy, vsz) = meta.voxel_size;
    if data.len() != nx * ny * nz {
        return Err(EngineError::DimensionMismatch(format!(
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
    match config.masking.inhomogeneity_method {
        InhomogeneityMethod::Makehomogeneous => {
            let h = &config.homogeneity;
            Ok(qsm_core::utils::makehomogeneous(data, &grid, h.sigma_mm, h.nbox)?)
        }
        InhomogeneityMethod::N4 => {
            // The plan refuses settings and grids N4 cannot use up front; this covers runs
            // resumed from a cached state. No mask: the corrected magnitude is what masking is
            // made from.
            let geometry = n4_geometry(meta, config)
                .map_err(|e| EngineError::Config(format!("inhomogeneity correction: {e}")))?;
            qsm_core::utils::n4_bias_correction(data, None, &grid, &geometry, &config.n4.core(), |_, _| {})
                .map(|out| out.corrected)
                .map_err(|e| EngineError::Config(format!("inhomogeneity correction: {e}")))
        }
    }
}

/// The image geometry N4 is given (`masking.n4_geometry`): the scan's header as ITK reads it, or
/// the axis-aligned one MATLAB's `make_nii` writes for the run's voxel size (QSMART's).
pub fn n4_geometry(
    meta: &RunMetadata, config: &PipelineConfig,
) -> Result<qsm_core::utils::ItkGeometry, qsm_core::error::InputError> {
    match config.masking.n4_geometry {
        N4Geometry::Scanner => crate::scan::itk_geometry(meta),
        N4Geometry::MakeNii => Ok(qsm_core::utils::ItkGeometry::make_nii(meta.voxel_size)),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn meta_4x4x4() -> RunMetadata {
        RunMetadata {
            dims: (4, 4, 4), voxel_size: (1.0, 1.0, 1.0),
            affine: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            n_echoes: 1, echo_times: vec![0.004], b0_direction: (0.0, 0.0, 1.0),
            field_strength: 3.0, has_magnitude: true,
            source_geometry: None, slice_geometry: None, orientation: None,
        }
    }

    #[test]
    fn test_homogeneity_correct_rejects_size_mismatch() {
        // The guard must report the mismatch rather than let qsm-core index past the volume.
        let cfg = PipelineConfig::default();
        let short = vec![1.0f64; 4 * 4 * 3];
        let err = homogeneity_correct(&short, &meta_4x4x4(), &cfg, "the magnitude").unwrap_err();
        assert!(matches!(err, EngineError::DimensionMismatch(_)));
        let msg = format!("{}", err);
        assert!(msg.contains("48 voxels"), "got: {}", msg);
        assert!(msg.contains("4x4x4"), "got: {}", msg);
        assert!(msg.contains("--no-inhomogeneity-correction"), "got: {}", msg);
    }

    #[test]
    fn test_homogeneity_correct_runs_on_matching_volume() {
        let cfg = PipelineConfig::default();
        let data: Vec<f64> = (0..4 * 4 * 4).map(|i| 100.0 + i as f64).collect();
        let out = homogeneity_correct(&data, &meta_4x4x4(), &cfg, "the magnitude").unwrap();
        assert_eq!(out.len(), data.len());
        assert!(out.iter().all(|v| v.is_finite()));
    }

    /// The echoes a mask reads: echo 1 or the last for a first- or last-echo section, every echo
    /// for the RSS, phase quality, or refinements that read the magnitude; morphology alone reads
    /// none.
    #[test]
    fn a_mask_reads_the_echoes_its_sections_use() {
        use crate::masking::MaskThresholdMethod;
        let section = |input| MaskSection {
            input, generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None }, refinements: vec![],
        };
        let erode = MaskOp::Erode { iterations: 1 };
        assert_eq!(mask_echoes(&[section(MaskingInput::MagnitudeFirst)], &[erode.clone(), MaskOp::Majority], 5), vec![1]);
        assert_eq!(mask_echoes(&[section(MaskingInput::MagnitudeLast)], &[], 5), vec![5]);
        assert_eq!(mask_echoes(&[section(MaskingInput::MagnitudeFirst), section(MaskingInput::MagnitudeLast)], &[], 5), vec![1, 5]);
        for input in [MaskingInput::Magnitude, MaskingInput::PhaseQuality] {
            assert_eq!(mask_echoes(&[section(input)], &[], 3), vec![1, 2, 3]);
        }
        assert_eq!(mask_echoes(&[section(MaskingInput::MagnitudeFirst)], &[MaskOp::signal_erode_default()], 3), vec![1, 2, 3]);
        assert!(!refinements_read_magnitude(&[erode, MaskOp::FillHoles { max_size: 0 }]));
        assert!(refinements_read_magnitude(&[MaskOp::MinIntensity { value: 100.0 }]));
    }

    /// The echoes masking reads are corrected, the others left as they are; nothing is corrected
    /// with the correction off.
    #[test]
    fn masking_corrects_only_the_echoes_it_reads() {
        use crate::masking::MaskThresholdMethod;
        let mut meta = meta_4x4x4();
        meta.n_echoes = 2;
        let a: Vec<f64> = (0..64).map(|i| 100.0 + i as f64).collect();
        let b: Vec<f64> = (0..64).map(|i| 50.0 + i as f64).collect();
        let first = [MaskSection {
            input: MaskingInput::MagnitudeFirst,
            generator: MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None }, refinements: vec![],
        }];
        let mut config = PipelineConfig::default();
        config.masking.refinements.clear();
        let got = masking_magnitudes(&first, &config, vec![a.clone(), b.clone()], &meta).unwrap();
        assert_eq!(got, vec![homogeneity_correct(&a, &meta, &config, "x").unwrap(), b.clone()]);
        config.masking.inhomogeneity_correction = false;
        assert_eq!(masking_magnitudes(&first, &config, vec![a.clone(), b.clone()], &meta).unwrap(), vec![a, b]);
    }

    /// The method decides which correction runs, and N4's own settings reach it.
    #[test]
    fn the_method_chooses_the_correction() {
        let mut meta = meta_4x4x4();
        meta.dims = (8, 8, 8);
        let data: Vec<f64> = (0..512).map(|i| 100.0 + (i % 37) as f64).collect();
        let mut config = PipelineConfig::default();
        let made = homogeneity_correct(&data, &meta, &config, "x").unwrap();
        config.masking.inhomogeneity_method = InhomogeneityMethod::N4;
        config.n4.max_iterations = vec![2];
        let grid = qsm_core::Grid::new(8, 8, 8, 1.0, 1.0, 1.0);
        let geometry = crate::scan::itk_geometry(&meta).unwrap();
        let n4 = qsm_core::utils::n4_bias_correction(&data, None, &grid, &geometry, &config.n4.core(), |_, _| {}).unwrap().corrected;
        assert_eq!(homogeneity_correct(&data, &meta, &config, "x").unwrap(), n4);
        assert_ne!(n4, made);
        assert_eq!(params(&config)["inhomogeneity_method"], "n4");
        assert_eq!(params(&config)["n4"]["max_iterations"], serde_json::json!([2]));
    }

    /// `masking.n4_geometry` decides the geometry N4 is given: the scan's header, or make_nii's
    /// axis-aligned one for the run's voxel size (here with an off-diagonal term and an origin
    /// the scanner's header carries and make_nii's does not); it is in the step's parameters.
    #[test]
    fn n4_is_given_the_configured_geometry() {
        let mut meta = meta_4x4x4();
        meta.dims = (8, 8, 8);
        meta.voxel_size = (1.0, 1.0, 2.0);
        meta.affine = [1.0, 5e-12, 0.0, -4.0, 0.0, 1.0, 0.0, -4.0, 0.0, 0.0, 2.0, -8.0, 0.0, 0.0, 0.0, 1.0];
        let mut config = PipelineConfig::default();
        config.masking.inhomogeneity_method = InhomogeneityMethod::N4;
        config.n4.max_iterations = vec![2];
        let scanner = crate::scan::itk_geometry(&meta).unwrap();
        let make_nii = qsm_core::utils::ItkGeometry::make_nii((1.0, 1.0, 2.0));
        assert_ne!(scanner, make_nii);
        assert_eq!(n4_geometry(&meta, &config).unwrap(), scanner);
        let scanner_params = params(&config);
        config.masking.n4_geometry = N4Geometry::MakeNii;
        assert_eq!(n4_geometry(&meta, &config).unwrap(), make_nii);
        assert_ne!(params(&config), scanner_params);
        assert_eq!(params(&config)["n4_geometry"], "make-nii");

        let data: Vec<f64> = (0..512).map(|i| 100.0 + (i % 37) as f64).collect();
        let grid = qsm_core::Grid::new(8, 8, 8, 1.0, 1.0, 2.0);
        let expected = qsm_core::utils::n4_bias_correction(&data, None, &grid, &make_nii, &config.n4.core(), |_, _| {})
            .unwrap().corrected;
        assert_eq!(homogeneity_correct(&data, &meta, &config, "x").unwrap(), expected);
    }

    /// A grid N4 cannot shrink, and no fitting level, are errors rather than qsm-core panics.
    #[test]
    fn n4_refuses_what_qsm_core_would_panic_on() {
        let mut config = PipelineConfig::default();
        config.masking.inhomogeneity_method = InhomogeneityMethod::N4;
        let data = vec![1.0f64; 64];
        let err = homogeneity_correct(&data, &meta_4x4x4(), &config, "x").unwrap_err().to_string();
        assert!(err.contains("shrink_factor = 4") && err.contains("4x4x4"), "{err}");
        config.n4.shrink_factor = 2;
        assert!(homogeneity_correct(&data, &meta_4x4x4(), &config, "x").is_ok());
        config.n4.max_iterations.clear();
        let err = homogeneity_correct(&data, &meta_4x4x4(), &config, "x").unwrap_err().to_string();
        assert!(err.contains("N4 max_iterations = []"), "{err}");
    }
}
