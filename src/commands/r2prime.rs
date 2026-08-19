use log::info;
use super::common::{load_nifti, load_mask, save_nifti};
use crate::cli::R2primeArgs;
use crate::error::QsmxtError;

pub fn execute(args: R2primeArgs) -> crate::Result<()> {
    let r2star = load_nifti(&args.r2star)?;
    let r2 = load_nifti(&args.r2)?;
    let (mask, _) = load_mask(&args.mask)?;
    if r2star.data.len() != r2.data.len() || r2star.data.len() != mask.len() {
        return Err(QsmxtError::Config(format!(
            "R2* ({}), R2 ({}) and mask ({}) must have the same number of voxels",
            r2star.data.len(), r2.data.len(), mask.len())));
    }
    info!("Computing R2' = R2* - R2");
    // r2prime clamps negatives to 0 (R2* ≥ R2 physically).
    let r2p = qsm_core::relaxometry::r2prime(&r2star.data, &r2.data, &mask);
    save_nifti(&args.output, &r2p, &r2star)?;
    info!("R2' map saved to {}", args.output.display());
    Ok(())
}
