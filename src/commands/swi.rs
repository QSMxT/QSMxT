use log::info;
use super::common::{load_nifti, load_mask, save_nifti, nifti_grid};
use crate::cli::SwiArgs;
use crate::nifti::write::write_volume;
use crate::pipeline::phase;

/// The SWI parameters `qsmxt swi` runs with: qsm-core's defaults, overridden by the flags.
fn swi_params(p: &crate::cli::SwiParamArgs) -> qsm_core::swi::SwiParams {
    let d = qsm_core::swi::SwiParams::default();
    let hp_sigma = match p.swi_hp_sigma {
        Some(ref s) if s.len() == 3 => [s[0], s[1], s[2]],
        _ => d.hp_sigma,
    };
    let scaling = p.swi_scaling
        .map(|a| crate::pipeline::config::swi_scaling_arg_to_config(a).into())
        .unwrap_or(d.scaling);
    let strength = p.swi_strength.unwrap_or(d.strength);
    let mip_window = p.swi_mip_window.unwrap_or(d.mip_window);
    qsm_core::swi::SwiParams { hp_sigma, scaling, strength, mip_window }
}

pub fn execute(args: SwiArgs) -> crate::Result<()> {
    let phase_nifti = load_nifti(&args.phase)?;
    let mag_nifti = load_nifti(&args.magnitude)?;
    let (mask, _) = load_mask(&args.mask)?;
    let grid = nifti_grid(&phase_nifti);

    let mut phase_data = phase_nifti.data.clone();
    phase::scale_phase_to_pi(&mut phase_data);
    let unwrapped = qsm_core::unwrap::laplacian_unwrap(&phase_data, &mask, &grid, qsm_core::unwrap::LaplacianSolver::Dct);

    info!("Computing SWI ({}x{}x{})", grid.nx(), grid.ny(), grid.nz());

    let params = swi_params(&args.swi_params);

    let swi = qsm_core::swi::calculate_swi(
        &unwrapped, &mag_nifti.data, &mask, &grid, &params,
    );

    save_nifti(&args.output, &swi, &phase_nifti)?;
    info!("SWI saved to {}", args.output.display());

    if args.mip {
        let mip_window = params.mip_window;
        // The projection is shorter than the SWI and centred on each slab, so it is written with
        // the geometry it reports rather than the input's (issue #211).
        let mip = qsm_core::swi::create_mip(&swi, &grid, &phase_nifti.affine, mip_window)
            .map_err(crate::error::QsmxtError::Config)?;
        let mip_path = args.mip_output.unwrap_or_else(|| args.output.with_extension("mip.nii"));
        write_volume(&mip_path, &mip.data, mip.grid.dims, mip.grid.voxel_size, &mip.affine)?;
        info!("MIP saved to {}", mip_path.display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    /// Every scaling reaches qsm-core unchanged through both `qsmxt run` and `qsmxt swi`, under
    /// both spellings of negative-tanh; anything else does not parse (#266: `negative-tanh` ran
    /// plain tanh under `run`).
    #[test]
    fn every_swi_scaling_reaches_qsm_core_through_run_and_swi() {
        use qsm_core::swi::PhaseScaling as P;
        let cases = [
            ("tanh", P::Tanh), ("negative-tanh", P::NegativeTanh), ("negative_tanh", P::NegativeTanh),
            ("positive", P::Positive), ("negative", P::Negative), ("triangular", P::Triangular),
        ];
        for (name, expected) in cases {
            let cli = crate::cli::Cli::try_parse_from(["qsmxt", "run", "bids", "--do-swi", "--swi-scaling", name])
                .unwrap();
            let crate::cli::Command::Run(run) = cli.command else { panic!("expected run") };
            let mut config = crate::pipeline::config::PipelineConfig::default();
            crate::pipeline::config::apply_run_overrides(&mut config, &run.pipeline).unwrap();
            assert_eq!(P::from(config.swi.scaling), expected, "run --swi-scaling {name}");
            // ...and it survives the config file round trip.
            let back = crate::pipeline::config::PipelineConfig::from_toml(&config.to_toml().unwrap()).unwrap();
            assert_eq!(back.swi.scaling, config.swi.scaling, "{name} through the config file");

            let cli = crate::cli::Cli::try_parse_from([
                "qsmxt", "swi", "p.nii", "m.nii", "--mask", "k.nii", "--output", "o.nii", "--swi-scaling", name,
            ]).unwrap();
            let crate::cli::Command::Swi(swi) = cli.command else { panic!("expected swi") };
            assert_eq!(super::swi_params(&swi.swi_params).scaling, expected, "swi --swi-scaling {name}");
        }
        assert!(crate::cli::Cli::try_parse_from(["qsmxt", "run", "bids", "--swi-scaling", "tahn"]).is_err());
        // A config file with an unknown scaling is refused too.
        assert!(toml::from_str::<qsmxt_config::SwiConfig>("scaling = \"tahn\"").is_err());
        assert_eq!(toml::from_str::<qsmxt_config::SwiConfig>("scaling = \"negative_tanh\"").unwrap().scaling,
                   qsmxt_config::SwiScaling::NegativeTanh);
    }
}
