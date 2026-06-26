use log::info;
use super::common::{compute_r2star, save_nifti};
use crate::cli::R2starArgs;

pub fn execute(args: R2starArgs) -> crate::Result<()> {
    info!("Computing R2* from {} echoes", args.inputs.len());
    let (r2star_map, reference) = compute_r2star(&args.inputs, &args.mask, &args.echo_times)?;
    save_nifti(&args.output, &r2star_map, &reference)?;
    info!("R2* map saved to {}", args.output.display());
    Ok(())
}
