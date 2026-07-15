use log::info;
use super::common::{load_nifti, load_mask, save_nifti, save_mask};
use crate::cli::{BgremoveCommand, BgremoveCommonArgs};

/// Convert a HARPERELLA tissue-phase field (radians at echo `te` s) to a ppm field, in place.
/// `ppm = phase · 1e6 / (2·π·γ·B0·TE)`, γ = 42.576e6 Hz/T (i.e. Hz = phase/(2π·TE), then
/// ppm = Hz·1e6/(γ·B0)). HARPERELLA works in the phase domain, so this turns its output into the
/// same ppm local field the other bgremove methods produce.
fn tissue_phase_to_ppm(phase: &mut [f64], te: f64, b0: f64) {
    const GAMMA: f64 = 42.576e6; // Hz/T
    let scale = 1e6 / (2.0 * std::f64::consts::PI * GAMMA * b0 * te);
    for v in phase.iter_mut() {
        *v *= scale;
    }
}

#[cfg(test)]
mod tests {
    use super::tissue_phase_to_ppm;

    #[test]
    fn test_tissue_phase_to_ppm_scale() {
        // 2π rad at TE=0.004 s, B0=7 T → Hz = 1/TE = 250; ppm = 250·1e6/(42.576e6·7) ≈ 0.8388.
        let mut v = [2.0 * std::f64::consts::PI];
        tissue_phase_to_ppm(&mut v, 0.004, 7.0);
        assert!((v[0] - 0.8388).abs() < 1e-3, "got {}", v[0]);
    }
}

pub fn execute(cmd: BgremoveCommand) -> crate::Result<()> {
    let (common, local_field, eroded_mask) = match cmd {
        BgremoveCommand::Vsharp(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let (nx, ny, nz) = field_nifti.dims;
            let (vsx, vsy, vsz) = field_nifti.voxel_size;
            let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
            info!("Background removal (V-SHARP, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::bgremove::VsharpParams::default();
            let params = qsm_core::bgremove::VsharpParams {
                threshold: args.threshold.unwrap_or(d.threshold),
                max_radius: args.max_radius.unwrap_or(d.max_radius),
                min_radius: args.min_radius.unwrap_or(d.min_radius),
            };
            let (lf, em) = qsm_core::bgremove::vsharp(
                &field_nifti.data, &mask, &grid, &params, |_, _| {},
            );
            (c, (lf, field_nifti), em)
        }
        BgremoveCommand::Pdf(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let (nx, ny, nz) = field_nifti.dims;
            let (vsx, vsy, vsz) = field_nifti.voxel_size;
            let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
            let bdir = (c.b0_direction[0], c.b0_direction[1], c.b0_direction[2]);
            info!("Background removal (PDF, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::bgremove::PdfParams::default();
            let params = qsm_core::bgremove::PdfParams {
                tol: args.tol.unwrap_or(d.tol),
                max_iter: None,
            };
            let lf = qsm_core::bgremove::pdf(
                &field_nifti.data, &mask, &grid, bdir, &params, |_, _| {},
            );
            (c, (lf, field_nifti), mask)
        }
        BgremoveCommand::Lbv(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let (nx, ny, nz) = field_nifti.dims;
            let (vsx, vsy, vsz) = field_nifti.voxel_size;
            let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
            info!("Background removal (LBV, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::bgremove::LbvParams::default();
            let params = qsm_core::bgremove::LbvParams {
                tol: args.tol.unwrap_or(d.tol),
                max_iter: None,
            };
            let (lf, em) = qsm_core::bgremove::lbv(
                &field_nifti.data, &mask, &grid, &params, |_, _| {},
            );
            (c, (lf, field_nifti), em)
        }
        BgremoveCommand::Ismv(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let (nx, ny, nz) = field_nifti.dims;
            let (vsx, vsy, vsz) = field_nifti.voxel_size;
            let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
            info!("Background removal (iSMV, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::bgremove::IsmvParams::default();
            let params = qsm_core::bgremove::IsmvParams {
                tol: args.tol.unwrap_or(d.tol),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                radius: args.radius.unwrap_or(d.radius),
            };
            let (lf, em) = qsm_core::bgremove::ismv(
                &field_nifti.data, &mask, &grid, &params, |_, _| {},
            );
            (c, (lf, field_nifti), em)
        }
        BgremoveCommand::Sharp(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let (nx, ny, nz) = field_nifti.dims;
            let (vsx, vsy, vsz) = field_nifti.voxel_size;
            let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
            info!("Background removal (SHARP, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::bgremove::SharpParams::default();
            let params = qsm_core::bgremove::SharpParams {
                threshold: args.threshold.unwrap_or(d.threshold),
                radius: args.radius.unwrap_or(d.radius),
            };
            let (lf, em) = qsm_core::bgremove::sharp(
                &field_nifti.data, &mask, &grid, &params,
            );
            (c, (lf, field_nifti), em)
        }
        BgremoveCommand::Resharp(args) => {
            let c = args.common;
            let field_nifti = load_nifti(&c.input)?;
            let (mask, _) = load_mask(&c.mask)?;
            let (nx, ny, nz) = field_nifti.dims;
            let (vsx, vsy, vsz) = field_nifti.voxel_size;
            let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
            info!("Background removal (RESHARP, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::bgremove::ResharpParams::default();
            let params = qsm_core::bgremove::ResharpParams {
                radius: args.radius.unwrap_or(d.radius),
                tik_reg: args.tik_reg.unwrap_or(d.tik_reg),
                tol: args.tol.unwrap_or(d.tol),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
            };
            let (lf, em) = qsm_core::bgremove::resharp(
                &field_nifti.data, &mask, &grid, &params, |_, _| {},
            );
            (c, (lf, field_nifti), em)
        }
        BgremoveCommand::Harperella(args) => {
            let phase_nifti = load_nifti(&args.input)?;
            let (mask, _) = load_mask(&args.mask)?;
            let (nx, ny, nz) = phase_nifti.dims;
            let (vsx, vsy, vsz) = phase_nifti.voxel_size;
            let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
            info!("Background removal (HARPERELLA, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::bgremove::HarperellaParams::default();
            let params = qsm_core::bgremove::HarperellaParams {
                radius: args.radius.unwrap_or(d.radius),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                tol: args.tol.unwrap_or(d.tol),
            };
            let (mut lf, em) = qsm_core::bgremove::harperella(
                &phase_nifti.data, &mask, &grid, &params, |_, _| {},
            );
            if let (Some(te), Some(b0)) = (args.te, args.field_strength) {
                tissue_phase_to_ppm(&mut lf, te, b0);
            }

            let common = BgremoveCommonArgs {
                input: args.input, mask: args.mask, output: args.output,
                b0_direction: vec![0.0, 0.0, 1.0], output_mask: args.output_mask,
            };
            (common, (lf, phase_nifti), em)
        }
        BgremoveCommand::Iharperella(args) => {
            let phase_nifti = load_nifti(&args.input)?;
            let (mask, _) = load_mask(&args.mask)?;
            let (nx, ny, nz) = phase_nifti.dims;
            let (vsx, vsy, vsz) = phase_nifti.voxel_size;
            let grid = qsm_core::Grid::new(nx, ny, nz, vsx, vsy, vsz);
            info!("Background removal (iHARPERELLA, {}x{}x{})", grid.nx(), grid.ny(), grid.nz());

            let d = qsm_core::bgremove::HarperellaParams::default();
            let params = qsm_core::bgremove::HarperellaParams {
                radius: args.radius.unwrap_or(d.radius),
                max_iter: args.max_iter.unwrap_or(d.max_iter),
                tol: args.tol.unwrap_or(d.tol),
            };
            let (mut lf, em) = qsm_core::bgremove::iharperella(
                &phase_nifti.data, &mask, &grid, &params, |_, _| {},
            );
            if let (Some(te), Some(b0)) = (args.te, args.field_strength) {
                tissue_phase_to_ppm(&mut lf, te, b0);
            }

            let common = BgremoveCommonArgs {
                input: args.input, mask: args.mask, output: args.output,
                b0_direction: vec![0.0, 0.0, 1.0], output_mask: args.output_mask,
            };
            (common, (lf, phase_nifti), em)
        }
    };

    let (local_field_data, field_nifti) = local_field;
    save_nifti(&common.output, &local_field_data, &field_nifti)?;
    info!("Local field saved to {}", common.output.display());

    if let Some(ref mask_out) = common.output_mask {
        save_mask(mask_out, &eroded_mask, &field_nifti)?;
        info!("Eroded mask saved to {}", mask_out.display());
    }

    Ok(())
}
