//! Standalone resampling of an oblique volume onto a cardinal-aligned grid.
//!
//! Continuous data (magnitude, an unwrapped field map, χ) can be resampled on its own with the
//! positional argument. **Wrapped phase cannot**: halfway between +3.0 and −3.0 rad a linear
//! interpolator returns 0.0 where the answer is near ±π, so every wrap becomes a band of wrong
//! values. Pass `--phase` with its `--magnitude` and the pair is interpolated as `mag·e^{iφ}`,
//! which is continuous across wraps.

use log::info;
use qsm_core::geometry::{
    b0_angle_from_affine, obliquity_axes_from_affine, obliquity_from_affine,
    resample_complex_to_axial, resample_to_axial, AxialResampleParams,
};

use super::common::{load_nifti, save_nifti};
use crate::cli::ResampleArgs;
use crate::error::QsmxtError;

/// `<stem>_axial.nii` next to the input, used when no explicit output is given.
fn default_out(input: &std::path::Path) -> std::path::PathBuf {
    let s = input.to_string_lossy();
    let stem = s
        .strip_suffix(".nii.gz")
        .or_else(|| s.strip_suffix(".nii"))
        .unwrap_or(&s);
    std::path::PathBuf::from(format!("{stem}_axial.nii"))
}

fn report(label: &str, affine: &[f64; 16], dims: (usize, usize, usize)) {
    let axes = obliquity_axes_from_affine(affine);
    info!(
        "{label}: {}x{}x{}, obliquity {:.1}° (per-axis {:.1}/{:.1}/{:.1}°), B0 {:.1}° off the slice normal",
        dims.0, dims.1, dims.2,
        obliquity_from_affine(affine), axes[0], axes[1], axes[2], b0_angle_from_affine(affine),
    );
}

pub fn execute(args: ResampleArgs) -> crate::Result<()> {
    let params = AxialResampleParams {
        noise_fill_fraction: if args.no_noise_fill { None } else { AxialResampleParams::default().noise_fill_fraction },
    };

    if let (Some(phase_path), Some(mag_path)) = (args.phase.as_ref(), args.magnitude.as_ref()) {
        let phase = load_nifti(phase_path)?;
        let mag = load_nifti(mag_path)?;
        if phase.dims != mag.dims {
            return Err(QsmxtError::DimensionMismatch(format!(
                "phase {:?} and magnitude {:?} differ", phase.dims, mag.dims
            )));
        }
        let (nx, ny, nz) = phase.dims;
        report("Input", &phase.affine, phase.dims);

        let mut phase_data = phase.data.clone();
        crate::pipeline::phase::scale_phase_to_pi(&mut phase_data);
        let out = resample_complex_to_axial(&mag.data, &phase_data, nx, ny, nz, &phase.affine, &params);
        report("Output", &out.affine, out.dims);

        let reference = qsm_core::io::NiftiData {
            data: vec![], dims: out.dims, voxel_size: out.voxel_size, affine: out.affine,
            scl_slope: 1.0, scl_inter: 0.0,
        };
        let p_out = args.phase_out.unwrap_or_else(|| default_out(phase_path));
        let m_out = args.magnitude_out.unwrap_or_else(|| default_out(mag_path));
        save_nifti(&p_out, &out.phase, &reference)?;
        save_nifti(&m_out, &out.magnitude, &reference)?;
        info!("Wrote {} and {}", p_out.display(), m_out.display());
        return Ok(());
    }

    let input = args.input.ok_or_else(|| QsmxtError::Config(
        "nothing to resample: pass a NIfTI file, or --phase with --magnitude for wrapped phase".into(),
    ))?;
    let output = args.output.unwrap_or_else(|| default_out(&input));
    let nifti = load_nifti(&input)?;
    let (nx, ny, nz) = nifti.dims;
    report("Input", &nifti.affine, nifti.dims);

    let resampled = resample_to_axial(&nifti.data, nx, ny, nz, &nifti.affine);
    report("Output", &resampled.affine, resampled.dims);

    let reference = qsm_core::io::NiftiData {
        data: vec![], dims: resampled.dims, voxel_size: resampled.voxel_size,
        affine: resampled.affine, scl_slope: 1.0, scl_inter: 0.0,
    };
    save_nifti(&output, &resampled.data, &reference)?;
    info!("Resampled volume saved to {}", output.display());
    Ok(())
}
