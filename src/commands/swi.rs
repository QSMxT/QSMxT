use log::info;
use super::common::{load_nifti, load_mask, save_nifti, nifti_grid};
use crate::cli::SwiArgs;
use crate::error::QsmxtError;
use crate::nifti::write::write_volume;
use crate::pipeline::phase;

/// The CLEAR-SWI parameters `qsmxt swi` runs with: the config defaults, overridden by the flags.
fn swi_params(p: &crate::cli::SwiParamArgs) -> qsm_core::swi::SwiParams {
    let mut c = qsmxt_engine::SwiConfig::default();
    if let Some(ref s) = p.swi_hp_sigma {
        if s.len() == 3 { c.hp_sigma = [s[0], s[1], s[2]]; }
    }
    if let Some(a) = p.swi_unwrapping { c.unwrap = crate::pipeline::config::swi_unwrap_arg_to_config(a); }
    if let Some(a) = p.swi_scaling { c.scaling = crate::pipeline::config::swi_scaling_arg_to_config(a); }
    if let Some(v) = p.swi_strength { c.strength = v; }
    if let Some(v) = p.swi_mip_window { c.mip_window = v; }
    c.to_params()
}

pub fn execute(args: SwiArgs) -> crate::Result<()> {
    let n_echoes = args.phase.len();
    if args.magnitude.len() != n_echoes {
        return Err(QsmxtError::Config(format!(
            "{} phase files but {} magnitude files: give one of each per echo", n_echoes, args.magnitude.len())));
    }
    let tes = match &args.tes {
        Some(t) => t.clone(),
        // One echo needs no echo time: nothing is combined.
        None if n_echoes == 1 => vec![1.0],
        None => super::combine::echo_times_from_sidecars(&args.phase)?,
    };
    if tes.len() != n_echoes {
        return Err(QsmxtError::Config(format!("{} echo times given for {} echoes", tes.len(), n_echoes)));
    }

    let mut phases = Vec::with_capacity(n_echoes);
    let mut mags = Vec::with_capacity(n_echoes);
    let reference = load_nifti(&args.magnitude[0])?;
    for (p, m) in args.phase.iter().zip(&args.magnitude) {
        let mut ph = load_nifti(p)?.data;
        phase::scale_phase_to_pi(&mut ph);
        phases.push(ph);
        mags.push(load_nifti(m)?.data);
    }
    let mask = match &args.mask {
        Some(path) => Some(load_mask(path)?.0),
        None => None,
    };
    let grid = nifti_grid(&reference);
    let params = swi_params(&args.swi_params);

    info!("Computing CLEAR-SWI ({} echo{}, {}x{}x{})",
          n_echoes, if n_echoes == 1 { "" } else { "es" }, grid.nx(), grid.ny(), grid.nz());
    let swi = qsm_core::swi::calculate_swi(&phases, &mags, &tes, mask.as_deref(), &grid, &params)?.swi;

    save_nifti(&args.output, &swi, &reference)?;
    info!("SWI saved to {}", args.output.display());

    if args.mip {
        // The projection is shorter than the SWI and centred on each slab, so it is written with
        // the geometry it reports rather than the input's (issue #211).
        let mip = qsm_core::swi::create_mip(&swi, &grid, &reference.affine, params.mip_window)
            .map_err(QsmxtError::Config)?;
        let mip_path = args.mip_output.unwrap_or_else(|| crate::bids::naming::minip_beside_swi(&args.output));
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
                "qsmxt", "swi", "--phase", "p.nii", "--magnitude", "m.nii", "--output", "o.nii", "--swi-scaling", name,
            ]).unwrap();
            let crate::cli::Command::Swi(swi) = cli.command else { panic!("expected swi") };
            assert_eq!(super::swi_params(&swi.swi_params).scaling, expected, "swi --swi-scaling {name}");
        }
        assert!(crate::cli::Cli::try_parse_from(["qsmxt", "run", "bids", "--swi-scaling", "tahn"]).is_err());
        // A config file with an unknown scaling is refused too.
        assert!(toml::from_str::<qsmxt_engine::SwiConfig>("scaling = \"tahn\"").is_err());
        assert_eq!(toml::from_str::<qsmxt_engine::SwiConfig>("scaling = \"negative_tanh\"").unwrap().scaling,
                   qsmxt_engine::SwiScaling::NegativeTanh);
    }
}
