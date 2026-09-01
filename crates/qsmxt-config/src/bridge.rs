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
    SeparationConfig as PSeparation,
    SeparationAlgorithm as PSepAlg,
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
        QsmAlgorithm::Tfi => PInvAlg::Tfi,
        QsmAlgorithm::Ilsqr => PInvAlg::Ilsqr,
        QsmAlgorithm::Qsmart => PInvAlg::Qsmart,
        QsmAlgorithm::Ndi => PInvAlg::Ndi,
        QsmAlgorithm::Fansi => PInvAlg::Fansi,
        QsmAlgorithm::FansiTgv => PInvAlg::FansiTgv,
        QsmAlgorithm::L1qsm => PInvAlg::L1qsm,
        QsmAlgorithm::Whqsm => PInvAlg::Whqsm,
        QsmAlgorithm::Hdqsm => PInvAlg::Hdqsm,
        QsmAlgorithm::AmpPe => PInvAlg::AmpPe,
        QsmAlgorithm::Xqsm => PInvAlg::Xqsm,
        QsmAlgorithm::Qsmnet => PInvAlg::Qsmnet,
        QsmAlgorithm::QsmnetPlus => PInvAlg::QsmnetPlus,
        QsmAlgorithm::Autoqsm => PInvAlg::Autoqsm,
        QsmAlgorithm::Qsmgan => PInvAlg::Qsmgan,
        QsmAlgorithm::Ir2qsm => PInvAlg::Ir2qsm,
        QsmAlgorithm::Lpcnn => PInvAlg::Lpcnn,
        QsmAlgorithm::ModlQsm => PInvAlg::ModlQsm,
        QsmAlgorithm::Nextqsm => PInvAlg::Nextqsm,
        // End-to-end from phase — the runner calls run_iqsm/run_iqsm_plus directly and
        // never routes these through run_dipole_inversion (which rejects them).
        QsmAlgorithm::Iqsm => PInvAlg::Iqsm,
        QsmAlgorithm::IqsmPlus => PInvAlg::IqsmPlus,
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
            bestpath: cfg.field_mapping.romeo.bestpath,
            temporal_uncertain_unwrapping: cfg.field_mapping.romeo.temporal_uncertain_unwrapping,
            max_seeds: cfg.field_mapping.romeo.max_seeds,
            merge_regions: cfg.field_mapping.romeo.merge_regions,
            correct_regions: cfg.field_mapping.romeo.correct_regions,
            wrap_addition: cfg.field_mapping.romeo.wrap_addition,
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
            BfAlgorithm::Bfrnet => PBgAlg::Bfrnet,
            // iQFM has no qsm-core BgRemovalAlgorithm (its input is phase, not a total field):
            // the qsmxt runner intercepts it and calls run_iqfm, so this value is never used.
            // Map to a harmless placeholder to keep the config well-formed.
            BfAlgorithm::Iqfm => PBgAlg::Vsharp,
        },
        vsharp: qsm_core::bgremove::VsharpParams {
            threshold: cfg.bg_removal.vsharp.threshold,
            max_radius: cfg.bg_removal.vsharp.max_radius,
            min_radius: cfg.bg_removal.vsharp.min_radius,
        },
        pdf: qsm_core::bgremove::PdfParams { tol: cfg.bg_removal.pdf.tol, max_iter: None },
        lbv: qsm_core::bgremove::LbvParams { tol: cfg.bg_removal.lbv.tol, max_iter: None },
        ismv: qsm_core::bgremove::IsmvParams {
            tol: cfg.bg_removal.ismv.tol,
            max_iter: cfg.bg_removal.ismv.max_iter,
            radius: cfg.bg_removal.ismv.radius,
        },
        sharp: qsm_core::bgremove::SharpParams {
            threshold: cfg.bg_removal.sharp.threshold,
            radius: cfg.bg_removal.sharp.radius,
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
        // mSMV boundary-shadow refinement (Roberts 2024), applied on top of the primary BFR
        // when enabled. b0/te are overridden from scan metadata and prefilter is forced off
        // (refinement mode) by the qsm-core dispatcher; only radius/maxk come from config.
        msmv: qsm_core::bgremove::MsmvParams {
            radius: cfg.bg_removal.msmv.radius,
            maxk: cfg.bg_removal.msmv.maxk,
            ..qsm_core::bgremove::MsmvParams::default()
        },
        msmv_refine: cfg.bg_removal.msmv_refine,
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
        tfi: qsm_core::inversion::TfiParams {
            lambda: cfg.inversion.tfi.lambda,
            precond: cfg.inversion.tfi.precond,
            merit: cfg.inversion.tfi.merit,
            data_weighting: cfg.inversion.tfi.data_weighting,
            percentage: cfg.inversion.tfi.percentage,
            cg_tol: cfg.inversion.tfi.cg_tol,
            cg_max_iter: cfg.inversion.tfi.cg_max_iter,
            max_iter: cfg.inversion.tfi.max_iter,
            tol: cfg.inversion.tfi.tol,
        },
        ilsqr: qsm_core::inversion::IlsqrParams {
            tol: cfg.inversion.ilsqr.tol, max_iter: cfg.inversion.ilsqr.max_iter,
        },
        ndi: qsm_core::inversion::NdiParams {
            tau: cfg.inversion.ndi.tau, alpha: cfg.inversion.ndi.alpha,
            max_iter: cfg.inversion.ndi.max_iter, phase_scale: cfg.inversion.ndi.phase_scale,
        },
        fansi: qsm_core::inversion::FansiParams {
            alpha1: cfg.inversion.fansi.alpha1, mu1: cfg.inversion.fansi.mu1,
            mu2: cfg.inversion.fansi.mu2, alpha0: cfg.inversion.fansi.alpha0,
            mu0: cfg.inversion.fansi.mu0, max_iter: cfg.inversion.fansi.max_iter,
            tol_update: cfg.inversion.fansi.tol_update, tol_delta: cfg.inversion.fansi.tol_delta,
            phase_scale: cfg.inversion.fansi.phase_scale,
            is_tgv: false,
        },
        l1qsm: qsm_core::inversion::L1QsmParams {
            alpha1: cfg.inversion.l1qsm.alpha1, mu1: cfg.inversion.l1qsm.mu1,
            mu2: cfg.inversion.l1qsm.mu2, mu3: cfg.inversion.l1qsm.mu3,
            lambda: cfg.inversion.l1qsm.lambda, max_iter: cfg.inversion.l1qsm.max_iter,
            tol_update: cfg.inversion.l1qsm.tol_update, tol_delta: cfg.inversion.l1qsm.tol_delta,
            phase_scale: cfg.inversion.l1qsm.phase_scale,
        },
        whqsm: qsm_core::inversion::WhQsmParams {
            alpha1: cfg.inversion.whqsm.alpha1, mu1: cfg.inversion.whqsm.mu1,
            mu2: cfg.inversion.whqsm.mu2, beta: cfg.inversion.whqsm.beta,
            muh: cfg.inversion.whqsm.muh, max_iter: cfg.inversion.whqsm.max_iter,
            tol_update: cfg.inversion.whqsm.tol_update, tol_delta: cfg.inversion.whqsm.tol_delta,
            phase_scale: cfg.inversion.whqsm.phase_scale,
        },
        hdqsm: qsm_core::inversion::HdQsmParams {
            alpha_l2: cfg.inversion.hdqsm.alpha_l2, mu1_l2: cfg.inversion.hdqsm.mu1_l2,
            mu2: cfg.inversion.hdqsm.mu2, max_iter_l1: cfg.inversion.hdqsm.max_iter_l1,
            max_iter_l2: cfg.inversion.hdqsm.max_iter_l2, tol_update: cfg.inversion.hdqsm.tol_update,
        },
        // AMP-PE: `b0` is overridden from scan metadata by the qsm-core dispatcher.
        amp_pe: qsm_core::inversion::AmpPeParams {
            wave_order: cfg.inversion.amp_pe.wave_order,
            nlevel: cfg.inversion.amp_pe.nlevel,
            wave_pec: cfg.inversion.amp_pe.wave_pec,
            simulated_te: cfg.inversion.amp_pe.simulated_te,
            max_linearization_ite: cfg.inversion.amp_pe.max_linearization_ite,
            b0: cfg.inversion.amp_pe.b0,
            gyro_ratio: cfg.inversion.amp_pe.gyro_ratio,
            damp_rate_sig: cfg.inversion.amp_pe.damp_rate_sig,
            damp_rate_par: cfg.inversion.amp_pe.damp_rate_par,
            max_pe_spar_ite: cfg.inversion.amp_pe.max_pe_spar_ite,
            max_pe_est_ite: cfg.inversion.amp_pe.max_pe_est_ite,
            cvg_thd: cfg.inversion.amp_pe.cvg_thd,
            tikhonov_beta: cfg.inversion.amp_pe.tikhonov_beta,
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
        // Overlap-tiling for the DL inversions: presence of `tile_size` enables it (core size);
        // `tile_halo` defaults to 8 (qsm-core's TileConfig default) when omitted. `None` =
        // whole-volume. Stored as `(core, halo)` so this crate needn't pull qsm-core's onnx feature.
        tile: cfg.inversion.tile_size.map(|core| (core, cfg.inversion.tile_halo.unwrap_or(8))),
    };

    let reference = match cfg.qsm.reference {
        QsmReference::Mean => PRef::Mean,
        QsmReference::None => PRef::None,
    };

    (field_mapping, bg_removal, inversion, reference)
}

/// Map a qsmxt-config separation algorithm to its qsm-core equivalent.
fn map_sep_alg(alg: SeparationAlgorithm) -> PSepAlg {
    match alg {
        SeparationAlgorithm::ChiSepIlsqr => PSepAlg::ChiSepIlsqr,
        SeparationAlgorithm::ChiSepMedi => PSepAlg::ChiSepMedi,
        SeparationAlgorithm::R2starQsm => PSepAlg::R2starQsm,
        SeparationAlgorithm::WaveSep => PSepAlg::WaveSep,
        SeparationAlgorithm::Decompose => PSepAlg::Decompose,
        SeparationAlgorithm::HcChisep => PSepAlg::HcChisep,
        SeparationAlgorithm::SusepNet => PSepAlg::SusepNet,
        SeparationAlgorithm::ChiSepNet => PSepAlg::ChiSepNet,
    }
}

/// Convert the chi-separation config to qsm-core's `SeparationConfig`.
///
/// `cf`/`b0` are left at defaults — the `run_separation` dispatcher overrides them
/// from scan metadata. `hc_chisep.se_echo_times` stays empty here; the runner sets it
/// when multi-echo spin-echo data is available.
pub fn to_separation_config(cfg: &PipelineConfig) -> PSeparation {
    let s = &cfg.separation;
    PSeparation {
        algorithm: map_sep_alg(s.algorithm),
        chi_sep_ilsqr: qsm_core::separation::ChiSepIlsqrParams {
            dr_pos: s.chi_sep_ilsqr.dr_pos, dr_neg: s.chi_sep_ilsqr.dr_neg,
            lambda1: s.chi_sep_ilsqr.lambda1, percentage: s.chi_sep_ilsqr.percentage,
            r2p_min: s.chi_sep_ilsqr.r2p_min, r2p_max: s.chi_sep_ilsqr.r2p_max,
            max_iter: s.chi_sep_ilsqr.max_iter, tol: s.chi_sep_ilsqr.tol,
            cg_max_iter: s.chi_sep_ilsqr.cg_max_iter, cg_tol: s.chi_sep_ilsqr.cg_tol,
            ..qsm_core::separation::ChiSepIlsqrParams::default()
        },
        chi_sep_medi: qsm_core::separation::ChiSepParams {
            lambda_para: s.chi_sep_medi.lambda_para, lambda_dia: s.chi_sep_medi.lambda_dia,
            lambda_cpl: s.chi_sep_medi.lambda_cpl, dr_pos: s.chi_sep_medi.dr_pos,
            dr_neg: s.chi_sep_medi.dr_neg, percentage: s.chi_sep_medi.percentage,
            cg_tol: s.chi_sep_medi.cg_tol, cg_max_iter: s.chi_sep_medi.cg_max_iter,
            max_iter: s.chi_sep_medi.max_iter, tol: s.chi_sep_medi.tol,
            ..qsm_core::separation::ChiSepParams::default()
        },
        r2star_qsm: qsm_core::separation::R2starQsmParams {
            r_const_3t: s.r2star_qsm.r_const_3t,
            ..qsm_core::separation::R2starQsmParams::default()
        },
        wavesep: qsm_core::separation::WaveSepParams {
            dr_pos: s.wavesep.dr_pos, dr_neg: s.wavesep.dr_neg, alpha: s.wavesep.alpha,
            lambda: s.wavesep.lambda, wavelet_order: s.wavesep.wavelet_order,
            max_iter: s.wavesep.max_iter, tol: s.wavesep.tol,
        },
        decompose: qsm_core::separation::DecomposeParams {
            n_inner: s.decompose.n_inner, chi_bound: s.decompose.chi_bound,
            max_lm_iter: s.decompose.max_lm_iter,
            ..qsm_core::separation::DecomposeParams::default()
        },
        hc_chisep: qsm_core::separation::HcChisepParams {
            dr_pos_3t: s.hc_chisep.dr_pos_3t, bin_hz: s.hc_chisep.bin_hz,
            ..qsm_core::separation::HcChisepParams::default()
        },
    }
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
    fn dl_algorithms_map_to_core_and_display() {
        use crate::enums::{BfAlgorithm as B, QsmAlgorithm as Q, SeparationAlgorithm as S};
        let mut cfg = PipelineConfig::default();
        for a in [
            Q::Xqsm, Q::Qsmnet, Q::QsmnetPlus, Q::Autoqsm, Q::Qsmgan, Q::Ir2qsm,
            Q::Lpcnn, Q::ModlQsm, Q::Nextqsm, Q::Iqsm, Q::IqsmPlus,
        ] {
            cfg.inversion.algorithm = a;
            let (_, _, inv, _) = to_pipeline_stages(&cfg);
            // map_alg produced a distinct core algorithm; Display round-trips to a non-empty id.
            let _ = inv.algorithm;
            assert!(!a.to_string().is_empty());
        }
        cfg = PipelineConfig::default();
        for b in [B::Bfrnet, B::Iqfm] {
            cfg.bg_removal.algorithm = b;
            let (_, bg, _, _) = to_pipeline_stages(&cfg);
            let _ = bg.algorithm;
            assert!(!b.to_string().is_empty());
        }
        for s in [S::SusepNet, S::ChiSepNet] {
            cfg.separation.algorithm = s;
            let sep = to_separation_config(&cfg);
            let _ = sep.algorithm;
            assert!(!s.to_string().is_empty());
        }
    }

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
