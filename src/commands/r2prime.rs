use log::{info, warn};
use super::common::{load_mask, load_nifti, save_mask, save_nifti};
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

    // Where the R2 acquisition reached, if the caller knows. A spin-echo companion to a 3D GRE is
    // routinely a slab, and only the person who acquired it can say where it stopped: a zero in
    // the R2 map reads the same whether nothing was acquired or the fit returned nothing.
    let coverage_in = match &args.r2_coverage {
        Some(path) => {
            let (cov, _) = load_mask(path)?;
            if cov.len() != mask.len() {
                return Err(QsmxtError::Config(format!(
                    "R2 coverage mask ({}) must have the same number of voxels as the R2* map ({})",
                    cov.len(), r2star.data.len())));
            }
            Some(cov)
        }
        None => None,
    };

    info!("Computing R2' = R2* - R2 where R2 was measured");
    // r2prime clamps negatives to 0 (R2* >= R2 physically) and reports R2' only over the
    // region R2 covers, so a slab MESE does not turn into R2' = R2* - 0 outside the slab.
    let out = qsm_core::relaxometry::r2prime(
        &r2star.data, &r2.data, &mask, coverage_in.as_deref(),
    );

    let in_mask = mask.iter().filter(|&&m| m > 0).count();
    let covered = out.coverage.iter().filter(|&&c| c > 0).count();
    if in_mask > 0 && covered < in_mask {
        warn!(
            "R2 covers {:.0}% of the mask; R2' is left at zero over the remaining {} voxels \
             rather than reporting R2* - 0 = R2* there. Pass --output-coverage to write out \
             which voxels were measured.",
            100.0 * covered as f64 / in_mask as f64, in_mask - covered,
        );
        if coverage_in.is_none() {
            warn!(
                "The covered region was inferred from R2 > 0. That is exact for an R2 fitted on \
                 this grid, but an R2 map resampled from another one leaves small non-zero values \
                 just outside its field of view; pass --r2-coverage to say where it really reached."
            );
        }
    }

    save_nifti(&args.output, &out.r2prime, &r2star)?;
    info!("R2' map saved to {}", args.output.display());
    if let Some(path) = &args.output_coverage {
        save_mask(path, &out.coverage, &r2star)?;
        info!("R2' coverage mask saved to {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    const DIMS: (usize, usize, usize) = (4, 4, 4);
    const N: usize = 4 * 4 * 4;
    const AFFINE: [f64; 16] = [
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];

    fn write(path: &Path, data: &[f64]) {
        qsm_core::io::save_nifti_to_file(path, data, DIMS, (1.0, 1.0, 1.0), &AFFINE).unwrap();
    }

    fn read(path: &Path) -> Vec<f64> {
        qsm_core::io::read_nifti_file(path).unwrap().data
    }

    /// Voxel index of the lower half of the volume along z, which stands in for a MESE slab.
    fn in_slab(i: usize) -> bool {
        i < N / 2
    }

    struct Case {
        _dir: tempfile::TempDir,
        r2star: PathBuf,
        r2: PathBuf,
        mask: PathBuf,
        coverage: PathBuf,
        out: PathBuf,
        out_coverage: PathBuf,
    }

    /// R2* = 30 Hz everywhere in the brain; R2 = 10 Hz inside the slab. Outside it, `bleed` --
    /// what a resampled R2 map leaves just past its own field of view.
    fn case(bleed: f64) -> Case {
        let dir = tempfile::tempdir().unwrap();
        let p = |n: &str| dir.path().join(n);
        write(&p("r2star.nii"), &vec![30.0; N]);
        write(&p("r2.nii"), &(0..N).map(|i| if in_slab(i) { 10.0 } else { bleed }).collect::<Vec<_>>());
        write(&p("mask.nii"), &vec![1.0; N]);
        write(&p("coverage.nii"), &(0..N).map(|i| in_slab(i) as u8 as f64).collect::<Vec<_>>());
        Case {
            r2star: p("r2star.nii"), r2: p("r2.nii"), mask: p("mask.nii"),
            coverage: p("coverage.nii"), out: p("r2prime.nii"),
            out_coverage: p("r2prime_coverage.nii"),
            _dir: dir,
        }
    }

    fn args(c: &Case, r2_coverage: Option<PathBuf>, output_coverage: Option<PathBuf>) -> R2primeArgs {
        R2primeArgs {
            r2star: c.r2star.clone(), r2: c.r2.clone(), mask: c.mask.clone(),
            r2_coverage, output: c.out.clone(), output_coverage,
        }
    }

    /// The headline case: R2 measured over half the volume, R2' reported over that half only.
    ///
    /// Outside the slab R2 is zero because nothing was acquired, and R2' = R2* - 0 = 30 Hz there
    /// would put the whole of R2* forward as reversible dephasing.
    #[test]
    fn slab_r2_does_not_become_r2prime_equals_r2star() {
        let c = case(0.0);
        execute(args(&c, None, Some(c.out_coverage.clone()))).unwrap();

        let r2p = read(&c.out);
        let cov = read(&c.out_coverage);
        for i in 0..N {
            if in_slab(i) {
                assert!((r2p[i] - 20.0).abs() < 1e-9, "voxel {i}: expected 30 - 10 = 20, got {}", r2p[i]);
                assert_eq!(cov[i], 1.0, "voxel {i} was measured");
            } else {
                assert_eq!(r2p[i], 0.0, "voxel {i}: 30 Hz here is R2*, not a measured R2'");
                assert_eq!(cov[i], 0.0, "voxel {i} was not measured");
            }
        }
    }

    /// With a resampled R2 map the values outside the slab are small but non-zero, so nothing in
    /// the map itself gives the boundary away. `--r2-coverage` is the only thing that can.
    #[test]
    fn interpolation_bleed_needs_the_coverage_mask() {
        let c = case(0.5);

        // Without it, the bleed passes for tissue and R2' is reported across the whole volume.
        execute(args(&c, None, Some(c.out_coverage.clone()))).unwrap();
        let bled = read(&c.out);
        assert!((bled[N - 1] - 29.5).abs() < 1e-9,
                "without the mask the bleed is subtracted as if measured, got {}", bled[N - 1]);
        assert_eq!(read(&c.out_coverage).iter().sum::<f64>(), N as f64);

        // With it, the same inputs give the same answer as a hard-edged slab.
        execute(args(&c, Some(c.coverage.clone()), Some(c.out_coverage.clone()))).unwrap();
        let masked = read(&c.out);
        let cov = read(&c.out_coverage);
        for i in 0..N {
            let (want_r2p, want_cov) = if in_slab(i) { (20.0, 1.0) } else { (0.0, 0.0) };
            assert!((masked[i] - want_r2p).abs() < 1e-9, "voxel {i}: got {}", masked[i]);
            assert_eq!(cov[i], want_cov);
        }
    }

    /// A coverage mask on a different grid is a mistake worth naming, not something to broadcast.
    #[test]
    fn mismatched_coverage_mask_is_rejected() {
        let c = case(0.0);
        let wrong = c.mask.with_file_name("wrong.nii");
        qsm_core::io::save_nifti_to_file(&wrong, &[1.0; 8], (2, 2, 2), (1.0, 1.0, 1.0), &AFFINE)
            .unwrap();
        let err = execute(args(&c, Some(wrong), None)).unwrap_err();
        assert!(format!("{err}").contains("coverage"), "the error should name the coverage mask: {err}");
    }

    /// Writing the coverage out is opt-in; without it only the R2' map appears.
    #[test]
    fn coverage_output_is_optional() {
        let c = case(0.0);
        execute(args(&c, None, None)).unwrap();
        assert!(c.out.exists());
        assert!(!c.out_coverage.exists());
    }
}
