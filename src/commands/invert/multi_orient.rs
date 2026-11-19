use log::{info, warn};
use super::common::{load_nifti, load_mask, save_nifti};
use crate::error::QsmxtError;

// ─── Multi-orientation reconstructions (COSMOS, STI) ───

/// Read a whitespace-separated N x 3 direction table (`--b0-dirs`).
///
/// Deliberately forgiving about the separator — people produce these with `awk`, a
/// spreadsheet export, or by hand — but strict about the shape, because a row that is not
/// three numbers means the file is not what the user thinks it is.
fn read_b0_dirs_file(path: &std::path::Path) -> crate::Result<Vec<(f64, f64, f64)>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| QsmxtError::Config(format!("{}: {}", path.display(), e)))?;
    let mut dirs = Vec::new();
    for (lineno, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let vals: Vec<&str> = line.split(|c: char| c.is_whitespace() || c == ',')
            .filter(|t| !t.is_empty())
            .collect();
        if vals.len() != 3 {
            return Err(QsmxtError::Config(format!(
                "{}:{}: expected 3 numbers per line, found {}",
                path.display(), lineno + 1, vals.len()
            )));
        }
        let mut xyz = [0.0f64; 3];
        for (i, v) in vals.iter().enumerate() {
            xyz[i] = v.parse().map_err(|_| QsmxtError::Config(format!(
                "{}:{}: '{}' is not a number", path.display(), lineno + 1, v
            )))?;
        }
        dirs.push((xyz[0], xyz[1], xyz[2]));
    }
    Ok(dirs)
}

/// Everything a multi-orientation reconstruction needs, loaded and validated.
struct MultiOrientInputs {
    fields: Vec<Vec<f64>>,
    mask: Vec<u8>,
    grid: qsm_core::Grid,
    reference: qsm_core::io::NiftiData,
    orientations: Vec<crate::multiorient::Orientation>,
}

/// Load N field maps, resolve their B0 directions, and refuse a set that cannot support the
/// requested reconstruction.
///
/// The grid check is not a formality. COSMOS and STI combine orientations voxel-for-voxel in
/// one k-space, so volumes that merely have the same dimensions but a different affine are
/// being silently treated as aligned when they are not.
fn load_multiorient(
    c: crate::cli::MultiOrientCommonArgs,
    kind: crate::multiorient::MultiOrientKind,
) -> crate::Result<MultiOrientInputs> {
    use crate::multiorient::{check_directions, direction_table, resolve_directions, DirectionSource};

    let n = c.inputs.len();

    // Explicit directions, from either spelling, before anything is loaded — a typo here
    // should not cost the user a minute of I/O.
    let explicit: Option<Vec<(f64, f64, f64)>> = if let Some(ref f) = c.b0_dirs {
        Some(read_b0_dirs_file(f)?)
    } else if !c.b0_direction.is_empty() {
        // clap flattens repeated `--b0-direction x y z` into one 3N-long vector.
        Some(c.b0_direction.chunks(3).map(|d| (d[0], d[1], d[2])).collect())
    } else {
        None
    };
    if let Some(ref dirs) = explicit {
        if dirs.len() != n {
            return Err(QsmxtError::Config(format!(
                "got {} B0 direction(s) for {} input(s) — supply one per --input, in the same order",
                dirs.len(), n
            )));
        }
    }

    let (mask, mask_nifti) = load_mask(&c.mask)?;
    let mut fields = Vec::with_capacity(n);
    let mut affines = Vec::with_capacity(n);
    let mut labels = Vec::with_capacity(n);
    let mut reference: Option<qsm_core::io::NiftiData> = None;

    for path in &c.inputs {
        let nifti = load_nifti(path)?;
        match &reference {
            // Geometry only — the first volume's samples are kept in `fields` like every
            // other orientation's, not duplicated here.
            None => reference = Some(qsm_core::io::NiftiData {
                data: Vec::new(),
                dims: nifti.dims,
                voxel_size: nifti.voxel_size,
                affine: nifti.affine,
                scl_slope: 1.0,
                scl_inter: 0.0,
            }),
            Some(r) => {
                if nifti.dims != r.dims {
                    return Err(QsmxtError::DimensionMismatch(format!(
                        "{} is {:?} but {:?} was expected — every orientation must sit on one common grid",
                        path.display(), nifti.dims, r.dims
                    )));
                }
                // Same dims, different affine: the volumes occupy different physical space,
                // so combining them voxel-for-voxel is meaningless.
                let drift = nifti.affine.iter().zip(r.affine.iter())
                    .map(|(a, b)| (a - b).abs()).fold(0.0f64, f64::max);
                if drift > 1e-3 {
                    return Err(QsmxtError::DimensionMismatch(format!(
                        "{} has a different affine from the first input (max element difference \
                         {drift:.3}) — the orientations are not on a common grid. Co-register \
                         them first; QSMxT does not do that yet",
                        path.display()
                    )));
                }
            }
        }
        affines.push(nifti.affine);
        labels.push(
            path.file_name().map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string()),
        );
        fields.push(nifti.data);
    }
    let reference = reference.expect("clap requires at least one --input");

    if mask_nifti.dims != reference.dims {
        return Err(QsmxtError::DimensionMismatch(format!(
            "mask is {:?} but the orientations are {:?}", mask_nifti.dims, reference.dims
        )));
    }

    let declared: Vec<Option<(f64, f64, f64)>> = match &explicit {
        Some(dirs) => dirs.iter().map(|&d| Some(d)).collect(),
        None => vec![None; n],
    };
    let orientations =
        resolve_directions(&labels, &declared, &affines, DirectionSource::Explicit);

    info!("{} over {} orientation(s):", kind, n);
    for row in direction_table(&orientations) {
        info!("  {row}");
    }

    let check = check_directions(&orientations, kind);
    match (&check.verdict, c.force) {
        (Ok(()), _) => info!("  {}", check.summary()),
        (Err(why), false) => return Err(QsmxtError::Config(why.to_string())),
        (Err(why), true) => warn!("--force: reconstructing anyway, but {why}"),
    }

    let grid = super::common::nifti_grid(&reference);
    Ok(MultiOrientInputs { fields, mask, grid, reference, orientations })
}

pub(super) fn run_cosmos(args: crate::cli::InvertCosmosArgs) -> crate::Result<()> {
    use crate::multiorient::MultiOrientKind;

    let out = args.output.clone();
    let magnitudes = args.magnitudes.clone();
    let inputs = load_multiorient(args.common, MultiOrientKind::Cosmos)?;
    let bdirs: Vec<_> = inputs.orientations.iter().map(|o| o.b0).collect();

    let d = qsm_core::inversion::CosmosParams::default();
    let params = qsm_core::inversion::CosmosParams {
        lambda: args.lambda.unwrap_or(d.lambda),
        tol: args.tol.unwrap_or(d.tol),
        max_iter: args.max_iter.unwrap_or(d.max_iter),
    };

    let chi = if magnitudes.is_empty() {
        qsm_core::inversion::cosmos(&inputs.fields, &bdirs, &inputs.mask, &inputs.grid, &params)
    } else {
        if magnitudes.len() != inputs.fields.len() {
            return Err(QsmxtError::Config(format!(
                "got {} --magnitude file(s) for {} orientation(s) — supply one per --input, or none",
                magnitudes.len(), inputs.fields.len()
            )));
        }
        let mut weights = Vec::with_capacity(magnitudes.len());
        for path in &magnitudes {
            let m = load_nifti(path)?;
            if m.dims != inputs.reference.dims {
                return Err(QsmxtError::DimensionMismatch(format!(
                    "magnitude {} is {:?} but the orientations are {:?}",
                    path.display(), m.dims, inputs.reference.dims
                )));
            }
            weights.push(m.data);
        }
        info!("Magnitude-weighted COSMOS (conjugate gradient, up to {} iterations)", params.max_iter);
        qsm_core::inversion::cosmos_weighted(
            &inputs.fields, &weights, &bdirs, &inputs.mask, &inputs.grid, &params,
        )
    };

    save_nifti(&out, &chi, &inputs.reference)?;
    info!("Susceptibility map saved to {}", out.display());
    Ok(())
}

pub(super) fn run_sti(args: crate::cli::InvertStiArgs) -> crate::Result<()> {
    use crate::multiorient::MultiOrientKind;

    let prefix = args.output.clone();
    let inputs = load_multiorient(args.common, MultiOrientKind::Sti)?;
    let bdirs: Vec<_> = inputs.orientations.iter().map(|o| o.b0).collect();

    let d = qsm_core::inversion::StiParams::default();
    let params = qsm_core::inversion::StiParams { lambda: args.lambda.unwrap_or(d.lambda) };

    let tensor = qsm_core::inversion::sti(
        &inputs.fields, &bdirs, &inputs.mask, &inputs.grid, &params,
    );
    let maps = qsm_core::inversion::tensor_maps(&tensor, &inputs.mask);

    // The pipeline's names and layout (`bids::naming`): one 4D tensor with its component order in
    // the sidecar, the scalar MMS and MSA maps, and one 4D eigenvector.
    use crate::bids::naming;
    let r = &inputs.reference;
    let write_4d = |tail: &naming::Tail, volumes: &[Vec<f64>], sidecar: serde_json::Value| -> crate::Result<()> {
        let path = naming::nifti_beside(&prefix, tail);
        let vols: Vec<&[f64]> = volumes.iter().map(|v| v.as_slice()).collect();
        crate::nifti::write::write_volumes_4d(&path, &vols, r.dims, r.voxel_size, &r.affine)?;
        if let Some(js) = crate::bids::entities::sidecar_path(&path) {
            std::fs::write(js, serde_json::to_string_pretty(&sidecar).unwrap_or_default())?;
        }
        Ok(())
    };
    write_4d(&naming::sti_tensor(), &tensor.components,
             serde_json::Value::Object(crate::pipeline::multiorient::tensor_sidecar_fields()))?;
    save_nifti(&naming::nifti_beside(&prefix, &naming::sti_mms()), &maps.mms, r)?;
    save_nifti(&naming::nifti_beside(&prefix, &naming::sti_msa()), &maps.msa, r)?;
    write_4d(&naming::sti_eigenvector(), &maps.pev, crate::pipeline::multiorient::eigenvector_sidecar())?;
    info!(
        "Tensor, MMS, MSA and principal eigenvector saved with prefix {}",
        prefix.display()
    );
    info!("MMS is the orientation-independent counterpart of a scalar QSM map; the eigenvector is in voxel coordinates of the common grid");
    Ok(())
}
