//! Standalone MCPC-3D-S coil combination (per-coil NIfTI in, combined echoes out).
//!
//! Exposes `qsm_core::utils::mcpc3ds_combine` on loose files, mirroring what the pipeline
//! does for `rec-uncombined_coil-NN` BIDS runs. Useful to inspect the combined phase before a
//! full run, or to build a combined BIDS dataset for other tools.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use log::info;

use super::common::{load_nifti, save_mask, save_nifti};
use crate::bids::entities;
use crate::cli::{CombineCommand, CombineMcpc3dsArgs, UnwrapAlgorithmArg};
use crate::error::QsmxtError;
use crate::pipeline::phase;

/// Group files into `[coil][echo]` order using BIDS `coil-NN` / `echo-N` entities when every
/// file has them, otherwise coil-major in the given order with `num_echoes` per coil.
fn group_by_coil(files: &[PathBuf], num_echoes: Option<usize>, what: &str) -> crate::Result<Vec<Vec<PathBuf>>> {
    let parsed: Vec<Option<(u32, u32)>> = files.iter().map(|f| {
        let name = f.file_name()?.to_str()?;
        let e = entities::parse_entities(name)?;
        Some((e.coil?, e.echo.unwrap_or(1)))
    }).collect();

    if parsed.iter().all(|p| p.is_some()) {
        let mut by_coil: BTreeMap<u32, BTreeMap<u32, PathBuf>> = BTreeMap::new();
        for (f, p) in files.iter().zip(&parsed) {
            let (coil, echo) = p.unwrap();
            if by_coil.entry(coil).or_default().insert(echo, f.clone()).is_some() {
                return Err(QsmxtError::Config(format!("{}: duplicate coil-{:02} echo-{}", what, coil, echo)));
            }
        }
        let n_echoes = by_coil.values().next().map_or(0, |m| m.len());
        let mut out = Vec::with_capacity(by_coil.len());
        for (coil, echoes) in by_coil {
            if echoes.len() != n_echoes {
                return Err(QsmxtError::Config(format!(
                    "{}: coil-{:02} has {} echo(es) but the first coil has {}", what, coil, echoes.len(), n_echoes)));
            }
            out.push(echoes.into_values().collect());
        }
        Ok(out)
    } else {
        let n_echoes = num_echoes.ok_or_else(|| QsmxtError::Config(format!(
            "{}: file names carry no coil-NN/echo-N entities — pass --num-echoes and list the files coil-major", what)))?;
        if n_echoes == 0 || !files.len().is_multiple_of(n_echoes) {
            return Err(QsmxtError::Config(format!(
                "{}: {} files is not a multiple of --num-echoes {}", what, files.len(), n_echoes)));
        }
        Ok(files.chunks(n_echoes).map(|c| c.to_vec()).collect())
    }
}

fn echo_times_from_sidecars(phase_files: &[PathBuf]) -> crate::Result<Vec<f64>> {
    let mut tes = Vec::with_capacity(phase_files.len());
    for f in phase_files {
        let js = entities::sidecar_path(f).filter(|p| p.exists()).ok_or_else(|| QsmxtError::Config(format!(
            "no echo times: pass --tes or provide a JSON sidecar next to {}", f.display())))?;
        tes.push(crate::bids::sidecar::read_sidecar(&js)?.echo_time);
    }
    Ok(tes)
}

fn output_path(prefix: &Path, tail: &str) -> PathBuf {
    let base = prefix.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    prefix.with_file_name(format!("{}_{}.nii", base, tail))
}

pub fn execute(cmd: CombineCommand) -> crate::Result<()> {
    match cmd {
        CombineCommand::Mcpc3ds(args) => mcpc3ds(args),
    }
}

fn mcpc3ds(args: CombineMcpc3dsArgs) -> crate::Result<()> {
    let phase_files = group_by_coil(&args.phase, args.num_echoes, "--phase")?;
    let mag_files = group_by_coil(&args.magnitude, args.num_echoes, "--magnitude")?;
    let n_coils = phase_files.len();
    let n_echoes = phase_files.first().map_or(0, |c| c.len());
    if n_coils < 2 {
        return Err(QsmxtError::Config(format!("MCPC-3D-S coil combination needs at least 2 coils, got {}", n_coils)));
    }
    if n_echoes < 2 {
        return Err(QsmxtError::Config(format!("MCPC-3D-S needs at least 2 echoes, got {}", n_echoes)));
    }
    if mag_files.len() != n_coils || mag_files.iter().any(|c| c.len() != n_echoes) {
        return Err(QsmxtError::Config(format!(
            "--magnitude layout ({} coils) does not match --phase ({} coils x {} echoes)", mag_files.len(), n_coils, n_echoes)));
    }
    let tes = match &args.tes {
        Some(t) => t.clone(),
        None => echo_times_from_sidecars(&phase_files[0])?,
    };
    if tes.len() != n_echoes {
        return Err(QsmxtError::Config(format!("{} echo times given for {} echoes", tes.len(), n_echoes)));
    }

    info!("MCPC-3D-S coil combination: {} coils x {} echoes, TEs={:?}s", n_coils, n_echoes, tes);
    let reference = load_nifti(&phase_files[0][0])?;
    let grid = super::common::nifti_grid(&reference);
    let n = reference.data.len();

    let mut phases: Vec<Vec<Vec<f64>>> = Vec::with_capacity(n_coils);
    let mut mags: Vec<Vec<Vec<f64>>> = Vec::with_capacity(n_coils);
    for c in 0..n_coils {
        let mut cp = Vec::with_capacity(n_echoes);
        let mut cm = Vec::with_capacity(n_echoes);
        for e in 0..n_echoes {
            let mut p = load_nifti(&phase_files[c][e])?.data;
            let m = load_nifti(&mag_files[c][e])?.data;
            if p.len() != n || m.len() != n {
                return Err(QsmxtError::DimensionMismatch(format!(
                    "coil {} echo {}: {} / {} voxels, expected {} (all inputs must share one grid)",
                    c + 1, e + 1, p.len(), m.len(), n)));
            }
            phase::scale_phase_to_pi(&mut p);
            cp.push(p);
            cm.push(m);
        }
        phases.push(cp);
        mags.push(cm);
    }

    let sigma = match &args.sigma {
        Some(s) => [s[0], s[1], s[2]],
        None => qsmxt_config::PipelineConfig::default().field_mapping.coil_combination_sigma,
    };
    let unwrap = match args.hip_unwrapping {
        UnwrapAlgorithmArg::Laplacian => qsm_core::unwrap::UnwrapMethod::Laplacian,
        _ => qsm_core::unwrap::UnwrapMethod::Romeo,
    };
    let combined = qsm_core::utils::mcpc3ds_combine(&phases, &mags, &tes, sigma, [0, 1], unwrap, &grid);
    drop(phases);
    drop(mags);

    if let Some(parent) = args.output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    for e in 0..n_echoes {
        let p = output_path(&args.output, &format!("echo-{}_part-phase", e + 1));
        save_nifti(&p, &combined.phases[e], &reference)?;
        let m = output_path(&args.output, &format!("echo-{}_part-mag", e + 1));
        save_nifti(&m, &combined.magnitudes[e], &reference)?;
        info!("Wrote {} and {}", p.display(), m.display());
    }
    let mask_path = output_path(&args.output, "desc-mcpc3ds_mask");
    save_mask(&mask_path, &combined.mask, &reference)?;
    info!("Wrote {}", mask_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_bids_named_files_by_coil_then_echo() {
        let files: Vec<PathBuf> = [
            "sub-1_rec-uncombined_coil-02_echo-2_part-phase_MEGRE.nii",
            "sub-1_rec-uncombined_coil-01_echo-1_part-phase_MEGRE.nii",
            "sub-1_rec-uncombined_coil-02_echo-1_part-phase_MEGRE.nii",
            "sub-1_rec-uncombined_coil-01_echo-2_part-phase_MEGRE.nii",
        ].iter().map(PathBuf::from).collect();
        let g = group_by_coil(&files, None, "--phase").unwrap();
        assert_eq!(g.len(), 2);
        assert!(g[0][0].to_string_lossy().contains("coil-01_echo-1"));
        assert!(g[0][1].to_string_lossy().contains("coil-01_echo-2"));
        assert!(g[1][1].to_string_lossy().contains("coil-02_echo-2"));
    }

    #[test]
    fn groups_plain_files_coil_major() {
        let files: Vec<PathBuf> = ["a.nii", "b.nii", "c.nii", "d.nii"].iter().map(PathBuf::from).collect();
        assert!(group_by_coil(&files, None, "--phase").is_err());
        let g = group_by_coil(&files, Some(2), "--phase").unwrap();
        assert_eq!(g.len(), 2);
        assert_eq!(g[1][0], PathBuf::from("c.nii"));
    }
}
