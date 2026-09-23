//! Combining an orientation group into one COSMOS map or one susceptibility tensor.
//!
//! # Where this sits in the pipeline
//!
//! The fan-in happens **after background removal and before inversion**. Background removal
//! wants each orientation in its own native space: the mask is native, there is no
//! interpolation blur yet, and PDF needs that acquisition's own B0 direction rather than the
//! reference frame's. So every member run goes through the ordinary pipeline, which leaves a
//! local field and a background-removal mask on disk, and this module picks those up.
//!
//! Members therefore also produce their own single-orientation χ maps. That is deliberate
//! rather than incidental: the first thing to check when a COSMOS map looks wrong is whether
//! the orientations were actually aligned, and per-orientation maps are how you see that.
//!
//! # What is assumed, and what is checked
//!
//! The orientations must already be co-registered onto one common grid. QSMxT has no
//! registration of its own, so this module verifies the assumption rather than satisfying it:
//! dimensions and affines must agree across the group, and the B0 direction set must be
//! non-degenerate ([`crate::multiorient::check_directions`]). Interpolating a local field
//! would be safe — it is continuous, unlike wrapped phase — but deciding *how* to interpolate
//! is the registration problem, and guessing it silently is the failure this whole feature is
//! built to avoid.

use std::path::PathBuf;

use log::{info, warn};

use crate::bids::derivatives::DerivativeOutputs;
use crate::bids::discovery::QsmRun;
use crate::bids::entities::AcquisitionKey;
use crate::bids::orientation::OrientationGroup;
use crate::error::QsmxtError;
use crate::multiorient::{
    check_directions, direction_table, DirectionSource, MultiOrientKind, Orientation,
};
use crate::pipeline::config::PipelineConfig;

/// The output key for a group: every entity the members agree on, with the ones that varied
/// between orientations dropped.
///
/// Dropping only what actually varied matters. A dataset whose orientations differ in `acq`
/// but share a meaningful `rec` should keep the `rec` in the output name; blanking every
/// entity that *could* have varied would throw that away.
pub fn group_key(members: &[&QsmRun]) -> AcquisitionKey {
    let first = members[0].key.clone();
    let agree = |f: fn(&AcquisitionKey) -> &Option<String>| {
        members.iter().all(|m| f(&m.key) == f(&first))
    };
    AcquisitionKey {
        subject: first.subject.clone(),
        session: first.session.clone(),
        acquisition: agree(|k| &k.acquisition).then(|| first.acquisition.clone()).flatten(),
        reconstruction: agree(|k| &k.reconstruction).then(|| first.reconstruction.clone()).flatten(),
        inversion: agree(|k| &k.inversion).then(|| first.inversion.clone()).flatten(),
        run: agree(|k| &k.run).then(|| first.run.clone()).flatten(),
        suffix: first.suffix.clone(),
    }
}

/// Resolve each member's B0 direction in the common frame.
///
/// A sidecar `B0_dir` wins over the affine, exactly as it does for a single-orientation run —
/// and here it is usually the *only* thing that can be right, because a group whose members
/// were resampled into a common frame has identical affines by construction.
fn orientations_for(members: &[&QsmRun], affines: &[[f64; 16]]) -> Vec<Orientation> {
    let labels: Vec<String> = members
        .iter()
        .map(|m| {
            // Show what distinguishes this member, not the whole key — the shared part is
            // already in the group label.
            m.key
                .acquisition
                .as_ref()
                .map(|a| format!("acq-{a}"))
                .or_else(|| m.key.run.as_ref().map(|r| format!("run-{r}")))
                .unwrap_or_else(|| m.key.to_string())
        })
        .collect();
    let declared: Vec<Option<(f64, f64, f64)>> = members.iter().map(|m| m.b0_dir).collect();
    crate::multiorient::resolve_directions(&labels, &declared, affines, DirectionSource::Sidecar)
}

/// Preview a group's direction table without running anything.
///
/// Reads each member's first phase header for its affine, which is cheap enough for `--dry`
/// and is the only way to show what the reconstruction would actually use. The TUI cannot do
/// this on every keystroke, so it previews structure and defers the physics to here.
pub fn preview_group(
    members: &[&QsmRun],
    kind: MultiOrientKind,
) -> (Vec<Orientation>, crate::multiorient::OrientationCheck) {
    let affines: Vec<[f64; 16]> = members
        .iter()
        .map(|m| {
            m.echoes
                .first()
                .and_then(|e| qsm_core::io::read_nifti_file(&e.phase_nifti).ok())
                .map(|n| n.affine)
                // No header to read: fall back to an identity affine, which yields (0,0,1) and
                // so shows up as the degenerate set it effectively is.
                .unwrap_or([
                    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
                ])
        })
        .collect();
    let orientations = orientations_for(members, &affines);
    let check = check_directions(&orientations, kind);
    (orientations, check)
}

/// Which reconstruction the config asks for.
pub fn kind_of(config: &PipelineConfig) -> MultiOrientKind {
    match config.multi_orientation.algorithm {
        crate::pipeline::config::MultiOrientAlgorithm::Cosmos => MultiOrientKind::Cosmos,
        crate::pipeline::config::MultiOrientAlgorithm::Sti => MultiOrientKind::Sti,
    }
}

/// The grid a group is reconstructed on: dimensions, voxel size, affine. Every member must
/// agree on all three, which is what "already co-registered" means in practice.
type Geometry = ((usize, usize, usize), (f64, f64, f64), [f64; 16]);

/// One member's contribution: its local field, the mask that field is defined on, and the
/// geometry both sit in.
struct MemberData {
    field: Vec<f64>,
    mask: Vec<u8>,
    geometry: Geometry,
}

/// Load one member's local field and background-removal mask.
fn load_member(output: &DerivativeOutputs, run: &QsmRun) -> crate::Result<MemberData> {
    let field_path = output.local_field_path(&run.key);
    if !field_path.exists() {
        return Err(QsmxtError::Config(format!(
            "no local field for {} at {} — the orientation's own pipeline must run before the \
             group is combined (and --clean-intermediates removes exactly this file, so leave \
             it off for multi-orientation runs)",
            run.key,
            field_path.display()
        )));
    }
    let field = qsm_core::io::read_nifti_file(&field_path)
        .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", field_path.display(), e)))?;

    // The background-removal mask is the one the local field is actually defined on; the
    // brain mask is larger wherever the BFR eroded.
    let mask_path = {
        let bg = output.bg_mask_path(&run.key);
        if bg.exists() { bg } else { output.mask_path(&run.key) }
    };
    let mask_nifti = qsm_core::io::read_nifti_file(&mask_path)
        .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", mask_path.display(), e)))?;
    let mask: Vec<u8> = mask_nifti.data.iter().map(|&v| (v > 0.5) as u8).collect();

    Ok(MemberData {
        field: field.data,
        mask,
        geometry: (field.dims, field.voxel_size, field.affine),
    })
}

/// Reconstruct one orientation group.
///
/// Returns the paths written. Errors are the caller's to report per group — one bad group
/// should not abandon the rest of the dataset.
pub fn reconstruct_group(
    group: &OrientationGroup,
    members: &[&QsmRun],
    config: &PipelineConfig,
    output: &DerivativeOutputs,
    progress: &dyn Fn(&str),
) -> crate::Result<Vec<PathBuf>> {
    let kind = kind_of(config);
    info!("{kind} for {} ({} orientations)", group.label, members.len());

    let mut fields = Vec::with_capacity(members.len());
    let mut masks = Vec::with_capacity(members.len());
    let mut affines = Vec::with_capacity(members.len());
    let mut geometry: Option<Geometry> = None;

    for run in members {
        let member = load_member(output, run)?;
        let (dims, voxel_size, affine) = member.geometry;
        match geometry {
            None => geometry = Some((dims, voxel_size, affine)),
            Some((d, _, a)) => {
                if dims != d {
                    return Err(QsmxtError::DimensionMismatch(format!(
                        "{}: {:?} but the group is {:?} — the orientations are not on a common grid",
                        run.key, dims, d
                    )));
                }
                let drift = affine.iter().zip(a.iter()).map(|(x, y)| (x - y).abs())
                    .fold(0.0f64, f64::max);
                if drift > 1e-3 {
                    return Err(QsmxtError::DimensionMismatch(format!(
                        "{} has a different affine from the rest of the group (max element \
                         difference {drift:.3}) — co-register the orientations onto one grid \
                         first; QSMxT cannot do that yet",
                        run.key
                    )));
                }
            }
        }
        affines.push(affine);
        fields.push(member.field);
        masks.push(member.mask);
    }
    let (dims, voxel_size, affine) = geometry.expect("a group has at least two members");

    // Every orientation must contribute at every voxel that is reconstructed, so the masks
    // intersect. A voxel one orientation eroded away has no field value there to combine.
    let mut mask = masks[0].clone();
    for m in &masks[1..] {
        for (a, b) in mask.iter_mut().zip(m.iter()) {
            *a &= *b;
        }
    }
    let kept = mask.iter().filter(|&&m| m == 1).count();
    let largest = masks.iter().map(|m| m.iter().filter(|&&v| v == 1).count()).max().unwrap_or(0);
    if largest > 0 && kept * 10 < largest * 7 {
        warn!(
            "the intersection of the orientation masks keeps {kept} of {largest} voxels \
             ({:.0}%) — that much loss usually means the orientations are not well aligned",
            100.0 * kept as f64 / largest as f64
        );
    }

    let orientations = orientations_for(members, &affines);
    info!("  directions:");
    for row in direction_table(&orientations) {
        info!("    {row}");
    }
    let check = check_directions(&orientations, kind);
    match (&check.verdict, config.multi_orientation.force) {
        (Ok(()), _) => info!("  {}", check.summary()),
        (Err(why), false) => return Err(QsmxtError::Config(why.clone())),
        (Err(why), true) => warn!("  force: reconstructing anyway, but {why}"),
    }

    let bdirs: Vec<_> = orientations.iter().map(|o| o.b0).collect();
    let grid = qsm_core::Grid::new(dims.0, dims.1, dims.2, voxel_size.0, voxel_size.1, voxel_size.2);
    let key = group_key(members);
    let mut written = Vec::new();

    let write = |path: &PathBuf, data: &[f64]| -> crate::Result<()> {
        crate::nifti::write::write_volume(path, data, dims, voxel_size, &affine)
    };

    match kind {
        MultiOrientKind::Cosmos => {
            progress(&format!("COSMOS ({} orientations)", members.len()));
            let params = qsm_core::inversion::CosmosParams {
                lambda: config.multi_orientation.lambda,
                ..Default::default()
            };
            let chi = qsm_core::inversion::cosmos(&fields, &bdirs, &mask, &grid, &params);
            let path = output.cosmos_path(&key);
            write(&path, &chi)?;
            written.push(path);
        }
        MultiOrientKind::Sti => {
            progress(&format!("STI ({} orientations)", members.len()));
            let params = qsm_core::inversion::StiParams { lambda: config.multi_orientation.lambda };
            let tensor = qsm_core::inversion::sti(&fields, &bdirs, &mask, &grid, &params);
            let maps = qsm_core::inversion::tensor_maps(&tensor, &mask);
            // qsm-core stores the symmetric tensor as [X11, X12, X13, X22, X23, X33].
            for (name, data) in ["11", "12", "13", "22", "23", "33"]
                .iter()
                .zip(tensor.components.iter())
            {
                let path = output.sti_path(&key, &format!("sti{name}"), "Chitensor");
                write(&path, data)?;
                written.push(path);
            }
            for (desc, data) in [("mms", &maps.mms), ("msa", &maps.msa)] {
                let path = output.sti_path(&key, desc, "Chimap");
                write(&path, data)?;
                written.push(path);
            }
            for (axis, data) in ["x", "y", "z"].iter().zip(maps.pev.iter()) {
                let path = output.sti_path(&key, &format!("pev{axis}"), "Chitensor");
                write(&path, data)?;
                written.push(path);
            }
        }
    }

    // The direction table is the half of the input that does not live in the NIfTI files, so
    // without it the reconstruction cannot be reproduced or audited.
    let desc = match kind {
        MultiOrientKind::Cosmos => "cosmos",
        MultiOrientKind::Sti => "sti",
    };
    let sidecar = serde_json::json!({
        "Description": format!("{kind} reconstruction over {} orientations", members.len()),
        "Sources": members.iter().map(|m| m.key.to_string()).collect::<Vec<_>>(),
        "B0_dirs": orientations.iter().map(|o| vec![o.b0.0, o.b0.1, o.b0.2]).collect::<Vec<_>>(),
        "B0_dir_sources": orientations.iter().map(|o| o.source.to_string()).collect::<Vec<_>>(),
        "Lambda": config.multi_orientation.lambda,
        "MaxPairwiseAngleDeg": check.max_pairwise_deg,
        "MinPairwiseAngleDeg": check.min_pairwise_deg,
        "DirectionRank": check.rank,
        "MaskVoxels": kept,
    });
    let sidecar_path = output.multiorient_sidecar_path(&key, desc);
    if let Some(parent) = sidecar_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(&sidecar)
        .map_err(|e| QsmxtError::Config(format!("multi-orientation sidecar: {e}")))?;
    std::fs::write(&sidecar_path, text)?;
    written.push(sidecar_path);

    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bids::entities::AcquisitionKey;

    fn run_with(acq: Option<&str>, rec: Option<&str>, run_e: Option<&str>) -> QsmRun {
        QsmRun {
            key: AcquisitionKey {
                subject: "01".into(),
                session: Some("01".into()),
                acquisition: acq.map(Into::into),
                reconstruction: rec.map(Into::into),
                inversion: None,
                run: run_e.map(Into::into),
                suffix: "MEGRE".into(),
            },
            echoes: vec![],
            coils: None,
            magnetic_field_strength: 3.0,
            echo_times: vec![],
            b0_dir: None,
            dims: (1, 1, 1),
            has_magnitude: false,
            mese: None,
        }
    }

    /// The entity that varied is dropped; an entity every member agrees on is kept, because
    /// it still describes the output.
    #[test]
    fn group_key_drops_only_what_varied() {
        let a = run_with(Some("dir1"), Some("mcpc3ds"), None);
        let b = run_with(Some("dir2"), Some("mcpc3ds"), None);
        let key = group_key(&[&a, &b]);
        assert_eq!(key.acquisition, None, "acq varied, so it is dropped");
        assert_eq!(key.reconstruction.as_deref(), Some("mcpc3ds"), "rec agrees, so it is kept");
        assert_eq!(key.basename(), "sub-01_ses-01_rec-mcpc3ds");
    }

    #[test]
    fn group_key_drops_a_varying_run_entity() {
        let a = run_with(Some("gre"), None, Some("1"));
        let b = run_with(Some("gre"), None, Some("2"));
        let key = group_key(&[&a, &b]);
        assert_eq!(key.run, None);
        assert_eq!(key.acquisition.as_deref(), Some("gre"));
        assert_eq!(key.basename(), "sub-01_ses-01_acq-gre");
    }

    #[test]
    fn cosmos_output_lands_beside_the_other_derivatives() {
        let a = run_with(Some("dir1"), None, None);
        let b = run_with(Some("dir2"), None, None);
        let out = DerivativeOutputs::new(std::path::Path::new("/out"));
        let path = out.cosmos_path(&group_key(&[&a, &b]));
        assert_eq!(
            path,
            std::path::PathBuf::from("/out/sub-01/ses-01/anat/sub-01_ses-01_desc-cosmos_Chimap.nii")
        );
    }

    /// A sidecar direction beats the affine — which is the only thing that can be right for a
    /// group already resampled into one frame, where every affine is identical.
    #[test]
    fn sidecar_directions_win_over_identical_affines() {
        let mut a = run_with(Some("dir1"), None, None);
        let mut b = run_with(Some("dir2"), None, None);
        a.b0_dir = Some((0.0, 0.0, 1.0));
        b.b0_dir = Some((0.0, 0.5, 0.866));
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let orientations = orientations_for(&[&a, &b], &[identity, identity]);
        assert_eq!(orientations[0].label, "acq-dir1");
        assert!(orientations.iter().all(|o| o.source == DirectionSource::Sidecar));
        let check = check_directions(&orientations, MultiOrientKind::Cosmos);
        assert!(check.is_ok(), "{:?}", check.verdict);
    }

    /// ...and without sidecars, that same group is refused rather than silently reconstructed.
    #[test]
    fn identical_affines_without_sidecars_are_refused() {
        let a = run_with(Some("dir1"), None, None);
        let b = run_with(Some("dir2"), None, None);
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let orientations = orientations_for(&[&a, &b], &[identity, identity]);
        assert!(orientations.iter().all(|o| o.source == DirectionSource::Affine));
        let why = check_directions(&orientations, MultiOrientKind::Cosmos).verdict.unwrap_err();
        assert!(why.contains("affines"), "{why}");
    }
}
