//! Bridge between qsmxt-config types and qsm-core pipeline types.
//!
//! Converts from the serde-enabled config types in this crate to the
//! pure algorithm config types in qsm-core's pipeline module.

// Re-exported types from qsm_core::pipeline (via pub use config::*)
// Use renamed imports to avoid conflicts with this crate's own types.
use qsm_core::pipeline::{
    FieldMappingConfig as PFieldMapping,
    BgRemovalConfig as PBgRemoval,
    InversionConfig as PInversion,
    ScanMetadata as PScanMetadata,
    UnwrappingAlgorithm as PUnwrap,
    B0EstimationMethod as PB0Method,
    BgRemovalAlgorithm as PBgAlg,
    InversionAlgorithm as PInvAlg,
    QsmReference as PRef,
    MaskSection as PMaskSection,
    MaskingInput as PMaskingInput,
    MaskOp as PMaskOp,
    MaskThresholdMethod as PMaskThresholdMethod,
};

use crate::config::PipelineConfig;
use crate::enums::*;

/// Map a qsmxt-config dipole inversion algorithm to its qsm-core equivalent.
fn map_alg(alg: QsmAlgorithm) -> PInvAlg {
    match alg {
        QsmAlgorithm::Rts => PInvAlg::Rts,
        QsmAlgorithm::Tv => PInvAlg::Tv,
        QsmAlgorithm::Tkd => PInvAlg::Tkd,
        QsmAlgorithm::Tsvd => PInvAlg::Tsvd,
        QsmAlgorithm::Tgv => PInvAlg::Tgv,
        QsmAlgorithm::Tikhonov => PInvAlg::Tikhonov,
        QsmAlgorithm::Nltv => PInvAlg::Nltv,
        QsmAlgorithm::Medi => PInvAlg::Medi,
        QsmAlgorithm::Ilsqr => PInvAlg::Ilsqr,
        QsmAlgorithm::Qsmart => PInvAlg::Qsmart,
    }
}

/// Convert a PipelineConfig to qsm-core pipeline stage configs.
pub fn to_pipeline_stages(cfg: &PipelineConfig) -> (
    PFieldMapping,
    PBgRemoval,
    PInversion,
    PRef,
) {
    let field_mapping = PFieldMapping {
        unwrapping_algorithm: match cfg.field_mapping.unwrapping_algorithm {
            UnwrappingAlgorithm::Romeo => PUnwrap::Romeo,
            UnwrappingAlgorithm::Laplacian => PUnwrap::Laplacian,
        },
        phase_offset_removal: cfg.field_mapping.phase_offset_removal,
        phase_offset_sigma: cfg.field_mapping.phase_offset_sigma,
        bipolar_correction: cfg.field_mapping.bipolar_correction,
        b0_estimation: match cfg.field_mapping.b0_estimation {
            B0Estimation::WeightedAvg => PB0Method::WeightedAvg,
            B0Estimation::LinearFit => PB0Method::LinearFit,
        },
        b0_weight_type: match cfg.field_mapping.b0_weight_type {
            B0WeightType::PhaseSNR => qsm_core::utils::B0WeightType::PhaseSNR,
            B0WeightType::PhaseVar => qsm_core::utils::B0WeightType::PhaseVar,
            B0WeightType::Average => qsm_core::utils::B0WeightType::Average,
            B0WeightType::TEs => qsm_core::utils::B0WeightType::TEs,
            B0WeightType::Mag => qsm_core::utils::B0WeightType::Mag,
        },
        romeo_params: qsm_core::unwrap::RomeoParams {
            individual: cfg.field_mapping.romeo.individual,
            correct_global: cfg.field_mapping.romeo.correct_global,
            template: cfg.field_mapping.romeo.template,
            phase_coherence: cfg.field_mapping.romeo.phase_coherence,
            phase_gradient_coherence: cfg.field_mapping.romeo.phase_gradient_coherence,
            phase_linearity: cfg.field_mapping.romeo.phase_linearity,
            mag_coherence: cfg.field_mapping.romeo.mag_coherence,
            mag_weight: cfg.field_mapping.romeo.mag_weight,
            mag_weight2: cfg.field_mapping.romeo.mag_weight2,
            ..Default::default()
        },
        linear_fit_params: qsm_core::utils::LinearFitParams {
            estimate_offset: cfg.field_mapping.linear_fit.estimate_offset,
            reliability_threshold_percentile: cfg.field_mapping.linear_fit.reliability_threshold_percentile,
        },
    };

    let bg_removal = PBgRemoval {
        algorithm: match cfg.bg_removal.algorithm {
            BfAlgorithm::Vsharp => PBgAlg::Vsharp,
            BfAlgorithm::Pdf => PBgAlg::Pdf,
            BfAlgorithm::Lbv => PBgAlg::Lbv,
            BfAlgorithm::Ismv => PBgAlg::Ismv,
            BfAlgorithm::Sharp => PBgAlg::Sharp,
            BfAlgorithm::Resharp => PBgAlg::Resharp,
            BfAlgorithm::Harperella => PBgAlg::Harperella,
            BfAlgorithm::Iharperella => PBgAlg::Iharperella,
        },
        vsharp: qsm_core::bgremove::VsharpParams {
            threshold: cfg.bg_removal.vsharp.threshold,
            max_radius_factor: cfg.bg_removal.vsharp.max_radius_factor,
            min_radius_factor: cfg.bg_removal.vsharp.min_radius_factor,
        },
        pdf: qsm_core::bgremove::PdfParams { tol: cfg.bg_removal.pdf.tol, max_iter: None },
        lbv: qsm_core::bgremove::LbvParams { tol: cfg.bg_removal.lbv.tol, max_iter: None },
        ismv: qsm_core::bgremove::IsmvParams {
            tol: cfg.bg_removal.ismv.tol,
            max_iter: cfg.bg_removal.ismv.max_iter,
            radius_factor: cfg.bg_removal.ismv.radius_factor,
        },
        sharp: qsm_core::bgremove::SharpParams {
            threshold: cfg.bg_removal.sharp.threshold,
            radius_factor: cfg.bg_removal.sharp.radius_factor,
        },
        resharp: qsm_core::bgremove::ResharpParams {
            radius: cfg.bg_removal.resharp.radius,
            tik_reg: cfg.bg_removal.resharp.tik_reg,
            tol: cfg.bg_removal.resharp.tol,
            max_iter: cfg.bg_removal.resharp.max_iter,
        },
        harperella: qsm_core::bgremove::HarperellaParams {
            radius: cfg.bg_removal.harperella.radius,
            max_iter: cfg.bg_removal.harperella.max_iter,
            tol: cfg.bg_removal.harperella.tol,
        },
        sdf: qsm_core::bgremove::SdfParams::default(),
    };

    let inversion = PInversion {
        algorithm: map_alg(cfg.inversion.algorithm),
        tkd: qsm_core::inversion::TkdParams { threshold: cfg.inversion.tkd.threshold },
        tsvd: qsm_core::inversion::TkdParams { threshold: cfg.inversion.tsvd.threshold },
        tikhonov: qsm_core::inversion::TikhonovParams {
            lambda: cfg.inversion.tikhonov.lambda,
            reg: match cfg.inversion.tikhonov.reg {
                crate::config::TikhonovReg::Identity => qsm_core::inversion::Regularization::Identity,
                crate::config::TikhonovReg::Gradient => qsm_core::inversion::Regularization::Gradient,
                crate::config::TikhonovReg::Laplacian => qsm_core::inversion::Regularization::Laplacian,
            },
        },
        tv: qsm_core::inversion::TvParams {
            lambda: cfg.inversion.tv.lambda, rho: cfg.inversion.tv.rho,
            tol: cfg.inversion.tv.tol, max_iter: cfg.inversion.tv.max_iter,
        },
        rts: qsm_core::inversion::RtsParams {
            delta: cfg.inversion.rts.delta, mu: cfg.inversion.rts.mu,
            rho: cfg.inversion.rts.rho, tol: cfg.inversion.rts.tol,
            max_iter: cfg.inversion.rts.max_iter, lsmr_iter: cfg.inversion.rts.lsmr_iter,
        },
        nltv: qsm_core::inversion::NltvParams {
            lambda: cfg.inversion.nltv.lambda, mu: cfg.inversion.nltv.mu,
            tol: cfg.inversion.nltv.tol, max_iter: cfg.inversion.nltv.max_iter,
            newton_iter: cfg.inversion.nltv.newton_iter,
        },
        medi: qsm_core::inversion::MediParams {
            lambda: cfg.inversion.medi.lambda,
            merit: cfg.inversion.medi.merit,
            smv: cfg.inversion.medi.smv,
            smv_radius: cfg.inversion.medi.smv_radius,
            data_weighting: cfg.inversion.medi.data_weighting,
            percentage: cfg.inversion.medi.percentage,
            cg_tol: cfg.inversion.medi.cg_tol,
            cg_max_iter: cfg.inversion.medi.cg_max_iter,
            max_iter: cfg.inversion.medi.max_iter,
            tol: cfg.inversion.medi.tol,
        },
        ilsqr: qsm_core::inversion::IlsqrParams {
            tol: cfg.inversion.ilsqr.tol, max_iter: cfg.inversion.ilsqr.max_iter,
        },
        tgv: qsm_core::inversion::TgvParams {
            iterations: cfg.inversion.tgv.iterations,
            erosions: cfg.inversion.tgv.erosions,
            alpha0: cfg.inversion.tgv.alpha0 as f32,
            alpha1: cfg.inversion.tgv.alpha1 as f32,
            step_size: cfg.inversion.tgv.step_size as f32,
            tol: cfg.inversion.tgv.tol as f32,
            ..Default::default()
        },
        qsmart: qsm_core::utils::QsmartParams {
            ilsqr_tol: cfg.inversion.qsmart.ilsqr_tol,
            ilsqr_max_iter: cfg.inversion.qsmart.ilsqr_max_iter,
            // NOTE: vasc_sphere_radius and frangi scales are in mm here; the qsmxt
            // runner converts them to voxels using the dataset voxel size.
            vasc_sphere_radius: cfg.inversion.qsmart.vasc_sphere_radius,
            sdf_spatial_radius: cfg.inversion.qsmart.sdf_spatial_radius,
            inversion: map_alg(cfg.inversion.qsmart.inversion),
            sdf_sigma1_stage1: cfg.inversion.qsmart.sdf_sigma1_stage1,
            sdf_sigma2_stage1: cfg.inversion.qsmart.sdf_sigma2_stage1,
            sdf_sigma1_stage2: cfg.inversion.qsmart.sdf_sigma1_stage2,
            sdf_sigma2_stage2: cfg.inversion.qsmart.sdf_sigma2_stage2,
            sdf_lower_lim: cfg.inversion.qsmart.sdf_lower_lim,
            sdf_curv_constant: cfg.inversion.qsmart.sdf_curv_constant,
            frangi_scale_range: [cfg.inversion.qsmart.frangi_scale_min, cfg.inversion.qsmart.frangi_scale_max],
            frangi_scale_ratio: cfg.inversion.qsmart.frangi_scale_ratio,
            frangi_c: cfg.inversion.qsmart.frangi_c,
            // ppm and b0_dir keep qsm-core defaults
            ..Default::default()
        },
    };

    let reference = match cfg.qsm.reference {
        QsmReference::Mean => PRef::Mean,
        QsmReference::None => PRef::None,
    };

    (field_mapping, bg_removal, inversion, reference)
}

/// Convert RunMetadata-like info to qsm-core ScanMetadata.
pub fn to_scan_metadata(
    dims: (usize, usize, usize),
    voxel_size: (f64, f64, f64),
    echo_times: &[f64],
    field_strength: f64,
    b0_direction: (f64, f64, f64),
) -> PScanMetadata {
    PScanMetadata {
        dims,
        voxel_size,
        echo_times: echo_times.to_vec(),
        field_strength,
        b0_direction,
    }
}

/// Convert qsmxt-config MaskSection to qsm-core MaskSection.
pub fn to_mask_sections(sections: &[crate::masking::MaskSection]) -> Vec<PMaskSection> {
    sections.iter().map(|s| PMaskSection {
        input: match s.input {
            crate::masking::MaskingInput::MagnitudeFirst => PMaskingInput::MagnitudeFirst,
            crate::masking::MaskingInput::Magnitude => PMaskingInput::Magnitude,
            crate::masking::MaskingInput::MagnitudeLast => PMaskingInput::MagnitudeLast,
            crate::masking::MaskingInput::PhaseQuality => PMaskingInput::PhaseQuality,
        },
        generator: convert_mask_op(&s.generator),
        refinements: s.refinements.iter().map(convert_mask_op).collect(),
    }).collect()
}

fn convert_mask_op(op: &crate::masking::MaskOp) -> PMaskOp {
    match op {
        crate::masking::MaskOp::Threshold { method, value } => PMaskOp::Threshold {
            method: match method {
                crate::masking::MaskThresholdMethod::Otsu => PMaskThresholdMethod::Otsu,
                crate::masking::MaskThresholdMethod::Fixed => PMaskThresholdMethod::Fixed,
                crate::masking::MaskThresholdMethod::Percentile => PMaskThresholdMethod::Percentile,
            },
            value: *value,
        },
        crate::masking::MaskOp::Bet { fractional_intensity } => PMaskOp::Bet { fractional_intensity: *fractional_intensity },
        crate::masking::MaskOp::Erode { iterations } => PMaskOp::Erode { iterations: *iterations },
        crate::masking::MaskOp::Dilate { iterations } => PMaskOp::Dilate { iterations: *iterations },
        crate::masking::MaskOp::Close { radius } => PMaskOp::Close { radius: *radius },
        crate::masking::MaskOp::FillHoles { max_size } => PMaskOp::FillHoles { max_size: *max_size },
        crate::masking::MaskOp::GaussianSmooth { sigma_mm } => PMaskOp::GaussianSmooth { sigma_mm: *sigma_mm },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qsmart_config_propagates_to_core_params() {
        // Previously the bridge punted with QsmartParams::default(), dropping config.
        let mut cfg = PipelineConfig::default();
        cfg.inversion.qsmart.ilsqr_tol = 0.005;
        cfg.inversion.qsmart.ilsqr_max_iter = 99;
        cfg.inversion.qsmart.vasc_sphere_radius = 5;
        cfg.inversion.qsmart.sdf_spatial_radius = 6;
        cfg.inversion.qsmart.inversion = QsmAlgorithm::Tkd;

        let (_, _, inv, _) = to_pipeline_stages(&cfg);
        assert_eq!(inv.qsmart.ilsqr_tol, 0.005);
        assert_eq!(inv.qsmart.ilsqr_max_iter, 99);
        assert_eq!(inv.qsmart.vasc_sphere_radius, 5);
        assert_eq!(inv.qsmart.sdf_spatial_radius, 6);
        assert_eq!(inv.qsmart.inversion, PInvAlg::Tkd);
    }

    #[test]
    fn qsmart_inversion_defaults_to_ilsqr() {
        let (_, _, inv, _) = to_pipeline_stages(&PipelineConfig::default());
        assert_eq!(inv.qsmart.inversion, PInvAlg::Ilsqr);
    }
}
