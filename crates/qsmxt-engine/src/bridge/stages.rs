//! The QSM stages' configs as QSM.rs takes them, from QSMxT's config.

use qsm_core::pipeline::{
    FieldMappingConfig as PFieldMapping,
    BgRemovalConfig as PBgRemoval,
    InversionConfig as PInversion,
    UnwrappingAlgorithm as PUnwrap,
    B0EstimationMethod as PB0Method,
    QsmReference as PRef,
};

use super::*;

pub fn to_pipeline_stages(cfg: &PipelineConfig) -> (
    PFieldMapping,
    PBgRemoval,
    PInversion,
    PRef,
) {
    let field_mapping = field_mapping_stage(cfg);
    let bg_removal = bg_removal_stage(cfg);
    let inversion = inversion_stage(cfg);

    let reference = match cfg.qsm.reference {
        QsmReference::Mean => PRef::Mean,
        // A region reference is subtracted by QSMxT, which has the parcellation; the core stage
        // only masks, or it would mean-reference the map a second time.
        QsmReference::None | QsmReference::Region => PRef::None,
    };

    (field_mapping, bg_removal, inversion, reference)
}

/// The field-mapping stage's config.
fn field_mapping_stage(cfg: &PipelineConfig) -> PFieldMapping {
    PFieldMapping {
        unwrapping_algorithm: match cfg.field_mapping.unwrapping_algorithm {
            UnwrappingAlgorithm::Romeo => PUnwrap::Romeo,
            UnwrappingAlgorithm::Laplacian => PUnwrap::Laplacian,
            UnwrappingAlgorithm::Prelude => PUnwrap::Prelude,
            UnwrappingAlgorithm::MediRegionGrowing => PUnwrap::MediRegionGrowing,
        },
        phase_offset_removal: cfg.field_mapping.phase_offset_removal,
        phase_offset_sigma_mm: cfg.field_mapping.phase_offset_sigma_mm,
        // Bipolar correction is part of the phase offset removal, before any unwrapper; with the
        // complex fit (which removes no offsets) it is MEDI's bipolar echo fit.
        bipolar_correction: cfg.field_mapping.bipolar_correction
            && (cfg.field_mapping.phase_offset_removal || cfg.field_mapping.b0_estimation == B0Estimation::ComplexFit),
        b0_estimation: match cfg.field_mapping.b0_estimation {
            B0Estimation::WeightedAvg => PB0Method::WeightedAvg {
                weighting: map_b0_weight_type(&cfg.field_mapping),
            },
            B0Estimation::LinearFit => PB0Method::LinearFit(qsm_core::utils::LinearFitParams {
                estimate_offset: cfg.field_mapping.linear_fit.estimate_offset,
            }),
            B0Estimation::RobustFit => {
                let r = &cfg.field_mapping.robust_fit;
                PB0Method::RobustFit(qsm_core::fieldmap::RobustFitParams {
                    loss: match r.loss {
                        RobustLoss::Tukey => qsm_core::fieldmap::RobustLoss::Tukey,
                        RobustLoss::Huber => qsm_core::fieldmap::RobustLoss::Huber,
                    },
                    tuning: r.tuning,
                    iterations: r.iterations,
                    downweight_level: r.downweight_level,
                    flag_ratio: r.flag_ratio,
                    estimate_offset: cfg.field_mapping.linear_fit.estimate_offset,
                })
            }
            B0Estimation::QsmartEchofit => PB0Method::QsmartEchofit {
                fit_threshold: cfg.field_mapping.qsmart_fit_threshold(),
            },
            B0Estimation::ComplexFit => PB0Method::ComplexFit(qsm_core::fieldmap::FitPpmComplexParams {
                reltol: cfg.field_mapping.complex_fit.reltol,
                max_iter: cfg.field_mapping.complex_fit.max_iter,
            }),
        },
        laplacian_solver: match cfg.field_mapping.laplacian_solver {
            LaplacianSolver::Dct => qsm_core::unwrap::LaplacianSolver::Dct,
            LaplacianSolver::Fft => qsm_core::unwrap::LaplacianSolver::Fft {
                pad: cfg.field_mapping.laplacian_fft_pad,
            },
            LaplacianSolver::Mirror => qsm_core::unwrap::LaplacianSolver::Mirror,
            LaplacianSolver::Mgpcg => qsm_core::unwrap::LaplacianSolver::Mgpcg,
        },
        laplacian_unmasked: cfg.field_mapping.laplacian_unmasked,
        laplacian_keep_outside: cfg.field_mapping.laplacian_keep_outside,
        romeo_params: (&cfg.field_mapping.romeo).into(),
        prelude_params: (&cfg.field_mapping.prelude).into(),
    }
}

/// The background-removal stage's config.
fn bg_removal_stage(cfg: &PipelineConfig) -> PBgRemoval {
    PBgRemoval {
        algorithm: map_bf_alg(cfg.bg_removal.algorithm),
        vsharp: qsm_core::bgremove::VsharpParams {
            threshold: cfg.bg_removal.vsharp.threshold,
            max_radius: cfg.bg_removal.vsharp.max_radius,
            min_radius: cfg.bg_removal.vsharp.min_radius,
            radius_step: cfg.bg_removal.vsharp.radius_step,
            deconvolve: cfg.bg_removal.vsharp.deconvolve,
            // qsm-core's SMV ball (STI Suite's); not a setting: it only reproduces one
            // implementation or the other (`qsmxt bgremove vsharp --ball` offers both, for parity).
            ball: qsm_core::bgremove::VsharpBall::default(),
            grid: cfg.bg_removal.vsharp.core_grid(),
        },
        pdf: qsm_core::bgremove::PdfParams {
            tol: cfg.bg_removal.pdf.tol, n_cg: cfg.bg_removal.pdf.n_cg,
            space: cfg.bg_removal.pdf.space.into(), n_pad: cfg.bg_removal.pdf.n_pad,
        },
        lbv: cfg.bg_removal.lbv.params(),
        ismv: qsm_core::bgremove::IsmvParams {
            variant: cfg.bg_removal.ismv.variant.into(),
            tol: cfg.bg_removal.ismv.tol,
            max_iter: cfg.bg_removal.ismv.max_iter,
            radius: cfg.bg_removal.ismv.radius,
        },
        sharp: qsm_core::bgremove::SharpParams {
            threshold: cfg.bg_removal.sharp.threshold,
            radius: cfg.bg_removal.sharp.radius,
            // qsm-core's SHARP (SEPIA's); the QSM.jl variant is for parity only
            // (`qsmxt bgremove sharp --variant`).
            variant: qsm_core::bgremove::SharpVariant::default(),
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
            pad: cfg.bg_removal.harperella.pad,
            rim: cfg.bg_removal.harperella.rim.into(),
        },
        iharperella: qsm_core::bgremove::IharperellaParams {
            max_iter: cfg.bg_removal.iharperella.max_iter,
            tol: cfg.bg_removal.iharperella.tol,
            pad: cfg.bg_removal.iharperella.pad,
            rim: cfg.bg_removal.iharperella.rim.into(),
        },
        sdf: qsm_core::bgremove::SdfParams::default(),
        tile: cfg.dl_tile(),
        erode_voxels: cfg.bg_removal.erode_voxels,
        polish_mask: cfg.bg_removal.polish_mask,
        ukb_mask_trim: cfg.bg_removal.ukb_mask_trim,
    }
}

/// The dipole-inversion stage's config.
fn inversion_stage(cfg: &PipelineConfig) -> PInversion {
    PInversion {
        algorithm: map_alg(cfg.inversion.algorithm),
        tkd: qsm_core::inversion::TkdParams { threshold: cfg.inversion.tkd.threshold, pad: cfg.inversion.tkd.pad },
        tsvd: qsm_core::inversion::TsvdParams { threshold: cfg.inversion.tsvd.threshold, pad: cfg.inversion.tsvd.pad },
        tikhonov: qsm_core::inversion::TikhonovParams {
            lambda: cfg.inversion.tikhonov.lambda,
            pad: cfg.inversion.tikhonov.pad,
            reg: match cfg.inversion.tikhonov.reg {
                crate::config::TikhonovReg::Identity => qsm_core::inversion::Regularization::Identity,
                crate::config::TikhonovReg::Gradient => qsm_core::inversion::Regularization::Gradient,
                crate::config::TikhonovReg::Laplacian => qsm_core::inversion::Regularization::Laplacian,
            },
        },
        tv: qsm_core::inversion::TvParams {
            lambda: cfg.inversion.tv.lambda, rho: cfg.inversion.tv.rho,
            tol: cfg.inversion.tv.tol, max_iter: cfg.inversion.tv.max_iter, pad: cfg.inversion.tv.pad,
        },
        rts: qsm_core::inversion::RtsParams {
            delta: cfg.inversion.rts.delta, mu: cfg.inversion.rts.mu,
            rho: cfg.inversion.rts.rho, tol: cfg.inversion.rts.tol,
            max_iter: cfg.inversion.rts.max_iter, lsmr_iter: cfg.inversion.rts.lsmr_iter,
            tau: cfg.inversion.rts.tau, gamma: cfg.inversion.rts.gamma, pad: cfg.inversion.rts.pad,
            // QSM.jl's iteration-1 convergence check: a QSM.rs parity option, not a setting.
            ..Default::default()
        },
        nltv: qsm_core::inversion::NltvParams {
            lambda: cfg.inversion.nltv.lambda, rho: cfg.inversion.nltv.rho, mu: cfg.inversion.nltv.mu,
            tol: cfg.inversion.nltv.tol, max_iter: cfg.inversion.nltv.max_iter,
            tol_newton: cfg.inversion.nltv.tol_newton,
            max_iter_newton: cfg.inversion.nltv.max_iter_newton, te: cfg.inversion.nltv.te.into(),
            pad: cfg.inversion.nltv.pad,
            // Field strength and echo times are scan parameters; the pipeline fills them in.
            ..Default::default()
        },
        medi: medi_params(cfg),
        tfi: tfi_params(cfg),
        ilsqr: qsm_core::inversion::IlsqrParams {
            tol: cfg.inversion.ilsqr.tol, max_iter: cfg.inversion.ilsqr.max_iter,
            pad_mm: cfg.inversion.ilsqr.pad_mm, precision: cfg.inversion.ilsqr.precision.into(),
        },
        star: qsm_core::inversion::StarParams {
            pad: cfg.inversion.star.pad, te: cfg.inversion.star.te.into(),
            // Field strength and echo times are scan parameters; the pipeline fills them in.
            ..Default::default()
        },
        ndi: qsm_core::inversion::NdiParams {
            tau: cfg.inversion.ndi.tau, alpha: cfg.inversion.ndi.alpha,
            max_iter: cfg.inversion.ndi.max_iter, auto_stop: cfg.inversion.ndi.auto_stop,
            te: cfg.inversion.ndi.te.into(), weight_norm: cfg.inversion.ndi.weight_norm.into(),
            // Field strength and echo times are scan parameters; the pipeline fills them in.
            ..Default::default()
        },
        fansi: qsm_core::inversion::FansiParams {
            alpha1: cfg.inversion.fansi.alpha1, mu1: cfg.inversion.fansi.mu1,
            mu2: cfg.inversion.fansi.mu2, alpha0: cfg.inversion.fansi.alpha0,
            mu0: cfg.inversion.fansi.mu0, max_iter: cfg.inversion.fansi.max_iter,
            tol_update: cfg.inversion.fansi.tol_update, tol_delta: cfg.inversion.fansi.tol_delta,
            te: cfg.inversion.fansi.te.into(),
            // The dispatcher sets is_tgv; field strength and echo times are scan parameters.
            ..Default::default()
        },
        l1qsm: qsm_core::inversion::L1QsmParams {
            alpha1: cfg.inversion.l1qsm.alpha1, mu1: cfg.inversion.l1qsm.mu1,
            mu2: cfg.inversion.l1qsm.mu2, mu3: cfg.inversion.l1qsm.mu3,
            lambda: cfg.inversion.l1qsm.lambda, max_iter: cfg.inversion.l1qsm.max_iter,
            tol_update: cfg.inversion.l1qsm.tol_update, tol_delta: cfg.inversion.l1qsm.tol_delta,
            te: cfg.inversion.l1qsm.te.into(),
            // Field strength and echo times are scan parameters; the pipeline fills them in.
            ..Default::default()
        },
        whqsm: qsm_core::inversion::WhQsmParams {
            alpha1: cfg.inversion.whqsm.alpha1, mu1: cfg.inversion.whqsm.mu1,
            mu2: cfg.inversion.whqsm.mu2, beta: cfg.inversion.whqsm.beta,
            muh: cfg.inversion.whqsm.muh, max_iter: cfg.inversion.whqsm.max_iter,
            tol_update: cfg.inversion.whqsm.tol_update, tol_delta: cfg.inversion.whqsm.tol_delta,
            weight: cfg.inversion.whqsm.weight.into(), te: cfg.inversion.whqsm.te.into(),
            // Field strength and echo times are scan parameters; the pipeline fills them in.
            ..Default::default()
        },
        hdqsm: qsm_core::inversion::HdQsmParams {
            alpha_l2: cfg.inversion.hdqsm.alpha_l2, mu1_l2: cfg.inversion.hdqsm.mu1_l2,
            mu2: cfg.inversion.hdqsm.mu2, max_iter_l1: cfg.inversion.hdqsm.max_iter_l1,
            max_iter_l2: cfg.inversion.hdqsm.max_iter_l2, tol_update: cfg.inversion.hdqsm.tol_update,
            // The dipole kernel is qsm-core's default (FANSI's discrete kernel).
            ..Default::default()
        },
        // AMP-PE: `b0` is overridden from scan metadata by the qsm-core dispatcher.
        amp_pe: amp_pe_params(cfg),
        tgv: tgv_params(cfg),
        qsmart: qsmart_params(cfg),
        // Overlap-tiling for the DL inversions: presence of `tile_size` enables it (core size);
        // `tile_halo` defaults to 8 (qsm-core's TileConfig default) when omitted. `None` =
        // whole-volume. Stored as `(core, halo)` so this crate needn't pull qsm-core's onnx feature.
        tile: cfg.dl_tile(),
        // `b0` and `mask_output` are the dispatcher's: it sets b0 from scan metadata and forces
        // mask_output off when HEIDI consumes the solution, so neither is in LsqrConfig.
        lsqr: qsm_core::inversion::LsqrQsmParams {
            residual_weighting: cfg.inversion.lsqr.residual_weighting,
            fit_global_offset: cfg.inversion.lsqr.fit_global_offset,
            tol: cfg.inversion.lsqr.tol,
            max_iter: cfg.inversion.lsqr.max_iter,
            ..Default::default()
        },
        apply_heidi: cfg.inversion.heidi.enabled,
        heidi: heidi_params(cfg),
    }
}

/// MEDI's parameters, as QSM.rs takes them.
fn medi_params(cfg: &PipelineConfig) -> qsm_core::inversion::MediParams {
    qsm_core::inversion::MediParams {
        lambda: cfg.inversion.medi.lambda,
        merit: cfg.inversion.medi.merit,
        smv: cfg.inversion.medi.smv,
        smv_radius: cfg.inversion.medi.smv_radius,
        smv_shrink_mask: cfg.inversion.medi.smv_shrink_mask,
        data_weighting: cfg.inversion.medi.data_weighting,
        percentage: cfg.inversion.medi.percentage,
        cg_tol: cfg.inversion.medi.cg_tol,
        csf: cfg.inversion.medi.csf, lambda_csf: cfg.inversion.medi.lambda_csf,
        csf_r2s_threshold: cfg.inversion.medi.csf_r2s_threshold, csf_erode: cfg.inversion.medi.csf_erode,
        zeropad: (cfg.inversion.medi.zeropad[0], cfg.inversion.medi.zeropad[1], cfg.inversion.medi.zeropad[2]),
        cg_max_iter: cfg.inversion.medi.cg_max_iter,
        max_iter: cfg.inversion.medi.max_iter,
        tol: cfg.inversion.medi.tol,
        msmv: cfg.inversion.medi.msmv_params(),
        te: cfg.inversion.medi.te.into(),
        // Field strength and echo times are scan parameters the pipeline fills in.
        ..Default::default()
    }
}

/// TFI's parameters, as QSM.rs takes them.
fn tfi_params(cfg: &PipelineConfig) -> qsm_core::inversion::TfiParams {
    qsm_core::inversion::TfiParams {
        lambda: cfg.inversion.tfi.lambda,
        precond: cfg.inversion.tfi.precond,
        merit: cfg.inversion.tfi.merit, linear: cfg.inversion.tfi.linear,
        data_weighting: cfg.inversion.tfi.data_weighting,
        percentage: cfg.inversion.tfi.percentage,
        cg_tol: cfg.inversion.tfi.cg_tol,
        cg_max_iter: cfg.inversion.tfi.cg_max_iter,
        max_iter: cfg.inversion.tfi.max_iter,
        tol: cfg.inversion.tfi.tol,
        te: cfg.inversion.tfi.te.into(),
        // Field strength and echo times are scan parameters the pipeline fills in.
        ..Default::default()
    }
}

/// AMP-PE's parameters, as QSM.rs takes them.
fn amp_pe_params(cfg: &PipelineConfig) -> qsm_core::inversion::AmpPeParams {
    qsm_core::inversion::AmpPeParams {
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
        // AMP-PE's solution does not depend on the weight's scale, so this stays off.
        normalize_weight: false,
    }
}

/// TGV's parameters, as QSM.rs takes them.
fn tgv_params(cfg: &PipelineConfig) -> qsm_core::inversion::TgvParams {
    qsm_core::inversion::TgvParams {
        iterations: cfg.inversion.tgv.iterations,
        erosions: cfg.inversion.tgv.erosions,
        alpha0: cfg.inversion.tgv.alpha0 as f32,
        alpha1: cfg.inversion.tgv.alpha1 as f32,
        step_size: cfg.inversion.tgv.step_size as f32,
        te: cfg.inversion.tgv.te.into(),
        // qsm-core's defaults (exact adjoint, erosion from the array's edge too); not
        // settings: they only reproduce the reference package's quirks for parity
        // (`qsmxt invert tgv --reference-divergence --reference-edge-erosion
        // --reference-output-box`).
        ..Default::default()
    }
}

/// QSMART's parameters, as QSM.rs takes them.
fn qsmart_params(cfg: &PipelineConfig) -> qsm_core::utils::QsmartParams {
    qsm_core::utils::QsmartParams {
        adjust_offset_grid: match cfg.inversion.qsmart.offset_grid {
            crate::enums::QsmartOffsetGrid::Fft => qsm_core::utils::QsmartOffsetGrid::Fft,
            crate::enums::QsmartOffsetGrid::QsmartReference => qsm_core::utils::QsmartOffsetGrid::QsmartReference,
        },
        ilsqr_max_iter: cfg.inversion.qsmart.ilsqr_max_iter,
        ilsqr_pad_mm: cfg.inversion.qsmart.ilsqr_pad_mm,
        // NOTE: vasc_sphere_radius and frangi scales are in `vessel_size_units` here; the
        // QSMART stage converts mm to voxels with the dataset voxel size
        // (crate::stages::qsmart::inversion_config).
        vasc_sphere_radius: cfg.inversion.qsmart.vasc_sphere_radius,
        inversion: map_alg(cfg.inversion.qsmart.inversion),
        sdf_sigma1_stage1: cfg.inversion.qsmart.sdf_sigma1_stage1,
        sdf_sigma2_stage1: cfg.inversion.qsmart.sdf_sigma2_stage1,
        sdf_sigma1_stage2: cfg.inversion.qsmart.sdf_sigma1_stage2,
        sdf_sigma2_stage2: cfg.inversion.qsmart.sdf_sigma2_stage2,
        sdf_lower_lim: cfg.inversion.qsmart.sdf_lower_lim,
        sdf_curv_constant: cfg.inversion.qsmart.sdf_curv_constant,
        frangi_scale_range: [cfg.inversion.qsmart.frangi_scale_min, cfg.inversion.qsmart.frangi_scale_max],
        frangi_scale_ratio: cfg.inversion.qsmart.frangi_scale_ratio,
        frangi_c: cfg.inversion.qsmart.frangi_c(),
        // ppm and b0_dir keep qsm-core defaults
        ..Default::default()
    }
}

/// HEIDI's parameters, as QSM.rs takes them.
fn heidi_params(cfg: &PipelineConfig) -> qsm_core::inversion::HeidiParams {
    qsm_core::inversion::HeidiParams {
        cone_threshold: cfg.inversion.heidi.cone_threshold,
        gradient_threshold: cfg.inversion.heidi.gradient_threshold,
        apply_laplacian_correction: cfg.inversion.heidi.apply_laplacian_correction,
        laplacian_threshold: cfg.inversion.heidi.laplacian_threshold,
        gradient_mask_floor: cfg.inversion.heidi.gradient_mask_floor,
        continuation_steps: cfg.inversion.heidi.continuation_steps,
        inner_iterations: cfg.inversion.heidi.inner_iterations,
        mu_min: cfg.inversion.heidi.mu_min,
        tol: cfg.inversion.heidi.tol,
        // Flattened in the config (a switch plus three numbers) so it can be a checkbox and
        // three rows rather than a nested table.
        denoise: cfg.inversion.heidi.denoise.then_some(qsm_core::utils::AnisotropicDiffusionParams {
            iterations: cfg.inversion.heidi.denoise_iterations,
            time_step: cfg.inversion.heidi.denoise_time_step,
            conductance: cfg.inversion.heidi.denoise_conductance,
            reference_itk: cfg.inversion.heidi.denoise_reference_itk,
        }),
        te: cfg.inversion.heidi.te.into(),
        // Field strength and echo times are scan parameters the pipeline fills in.
        ..Default::default()
    }
}
