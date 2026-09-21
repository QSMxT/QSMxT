use log::info;
use super::common::{load_nifti, load_mask, save_nifti, nifti_grid};
use crate::cli::SwiArgs;
use crate::nifti::write::write_volume;
use crate::pipeline::mip::mip_geometry;
use crate::pipeline::phase;

pub fn execute(args: SwiArgs) -> crate::Result<()> {
    let phase_nifti = load_nifti(&args.phase)?;
    let mag_nifti = load_nifti(&args.magnitude)?;
    let (mask, _) = load_mask(&args.mask)?;
    let grid = nifti_grid(&phase_nifti);

    let mut phase_data = phase_nifti.data.clone();
    phase::scale_phase_to_pi(&mut phase_data);
    let unwrapped = qsm_core::unwrap::laplacian_unwrap(&phase_data, &mask, &grid);

    info!("Computing SWI ({}x{}x{})", grid.nx(), grid.ny(), grid.nz());

    let d = qsm_core::swi::SwiParams::default();
    let hp_sigma = match args.swi_params.swi_hp_sigma {
        Some(ref s) if s.len() == 3 => [s[0], s[1], s[2]],
        _ => d.hp_sigma,
    };
    let scaling = match args.swi_params.swi_scaling.as_deref() {
        Some("negative-tanh" | "negative_tanh") => qsm_core::swi::PhaseScaling::NegativeTanh,
        Some("positive") => qsm_core::swi::PhaseScaling::Positive,
        Some("negative") => qsm_core::swi::PhaseScaling::Negative,
        Some("triangular") => qsm_core::swi::PhaseScaling::Triangular,
        Some("tanh") => qsm_core::swi::PhaseScaling::Tanh,
        _ => d.scaling,
    };
    let strength = args.swi_params.swi_strength.unwrap_or(d.strength);
    let params = qsm_core::swi::SwiParams { hp_sigma, scaling, strength, ..d };

    let swi = qsm_core::swi::calculate_swi(
        &unwrapped, &mag_nifti.data, &mask, &grid, &params,
    );

    save_nifti(&args.output, &swi, &phase_nifti)?;
    info!("SWI saved to {}", args.output.display());

    if args.mip {
        let mip_window = args.swi_params.swi_mip_window.unwrap_or(d.mip_window);
        // The projection is shorter than the SWI and centred on each slab, so it is written with
        // its own dimensions and origin rather than the input's (issue #211).
        let (mip_dims, mip_affine) = mip_geometry(phase_nifti.dims, &phase_nifti.affine, mip_window)?;
        let mip = qsm_core::swi::create_mip(&swi, &grid, mip_window);
        let mip_path = args.mip_output.unwrap_or_else(|| args.output.with_extension("mip.nii"));
        write_volume(&mip_path, &mip, mip_dims, phase_nifti.voxel_size, &mip_affine)?;
        info!("MIP saved to {}", mip_path.display());
    }

    Ok(())
}
