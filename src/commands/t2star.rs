use log::info;
use super::common::{compute_r2star, save_nifti};
use crate::cli::T2starArgs;

pub fn execute(args: T2starArgs) -> crate::Result<()> {
    info!("Computing T2* from {} echoes", args.inputs.len());
    let (r2star_map, reference) = compute_r2star(&args.inputs, &args.mask, &args.echo_times)?;

    let t2star_map: Vec<f64> = r2star_map.iter()
        .map(|&r2| if r2 > 0.0 { 1.0 / r2 } else { 0.0 })
        .collect();

    save_nifti(&args.output, &t2star_map, &reference)?;
    info!("T2* map saved to {}", args.output.display());
    Ok(())
}
