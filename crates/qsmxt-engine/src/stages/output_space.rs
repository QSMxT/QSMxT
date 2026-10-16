//! The return to the acquired grid: a run resampled to axial reconstructs on the cardinal grid,
//! and its outputs go back to the grid the data was acquired on.
//!
//! That is the right place for anything the caller already holds in the acquired space — a FLIRT
//! matrix is defined in a coordinate space derived from the image's dimensions and voxel sizes,
//! so applying one to a volume on a different grid gives a wrong registration rather than an
//! error. `--output-space working` keeps the resampled grid for callers who want it.
//!
//! Each kind of volume travels the way it must: masks by nearest neighbour, label images by a
//! majority vote, wrapped phase with its magnitude through the complex domain, everything else
//! trilinearly. This is a second interpolation on top of the first, so the result is slightly
//! smoother than a reconstruction that was never resampled.

use crate::error::EngineError;
use crate::store::{Contrast, Geometry, Item};

/// What a published volume is, for resampling it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A continuous quantity (χ, R2*, a magnitude, ...): trilinear.
    Continuous,
    /// A binary mask: nearest neighbour.
    Mask,
    /// Integer label ids (a segmentation): a majority vote, never interpolated values.
    Labels,
    /// Wrapped phase: only through the complex domain, with its magnitude ([`Kind::Phase`]'s
    /// item names it).
    Phase(Item),
    /// A slab projection on its own shorter grid: rebuilt from its source volume (the item) over
    /// the window, never interpolated.
    Projection(Item),
}

/// Every output a run of `n_echoes` publishes, and what each is. The steps that run decide which
/// of them a run has.
pub fn published(n_echoes: usize) -> Vec<(Item, Kind)> {
    use Kind::*;
    let mut out = vec![
        (Item::Chi, Continuous), (Item::ChiSupport, Mask), (Item::SinglePassChi, Continuous),
        (Item::BrainMask, Mask), (Item::ReliableMask, Mask), (Item::CoverageMask, Mask),
        (Item::CombinedMagnitude, Continuous),
        (Item::TotalFieldHz, Continuous), (Item::TotalFieldMagnitude, Continuous),
        (Item::Swi, Continuous), (Item::SwiMip, Projection(Item::Swi)),
        (Item::T2star, Continuous), (Item::R2star, Continuous),
        (Item::R2, Continuous), (Item::R2prime, Continuous), (Item::R2primeCoverage, Mask),
        (Item::SeparatedTotalChi, Continuous), (Item::Segmentation, Labels),
    ];
    for c in Contrast::BOTH {
        out.extend([
            (Item::Smwi(c), Continuous), (Item::SmwiMip(c), Projection(Item::Smwi(c))),
            (Item::SeparatedChi(c), Continuous),
        ]);
    }
    for e in 1..=n_echoes {
        out.extend([
            (Item::CombinedEchoPhase(e), Phase(Item::CombinedEchoMagnitude(e))),
            (Item::CombinedEchoMagnitude(e), Continuous),
        ]);
    }
    out
}

/// The `output_space` step's parameters. `resampling` versions how each kind travels, so outputs
/// returned by an older release (a trilinearly interpolated dseg, SMWI minIPs left on the working
/// grid; #267) are redone.
pub fn params(dst_dims: (usize, usize, usize)) -> serde_json::Value {
    serde_json::json!({
        "space": "acquired", "dims": [dst_dims.0, dst_dims.1, dst_dims.2], "resampling": "by-kind",
    })
}

/// The acquired grid, for writing a volume on it.
pub fn acquired(dims: (usize, usize, usize), affine: &[f64; 16]) -> Geometry {
    Geometry { dims, voxel_size: qsm_core::geometry::voxel_sizes_from_affine(affine), affine: *affine }
}

/// One volume of `kind` (not phase or a projection) from the working grid (`src`) onto the
/// acquired grid (`dst`); `None` when the grids cannot be mapped (the volume stays where it is).
pub fn resample(
    data: &[f64], kind: Kind, src: (&(usize, usize, usize), &[f64; 16]), dst: &Geometry,
) -> Result<Option<Vec<f64>>, EngineError> {
    let (&src_dims, src_affine) = src;
    Ok(match kind {
        Kind::Mask => {
            let m: Vec<u8> = data.iter().map(|v| if *v > 0.5 { 1u8 } else { 0u8 }).collect();
            qsm_core::geometry::resample_mask_onto(&m, src_dims, src_affine, dst.dims, &dst.affine)
                .map(|out| out.iter().map(|v| *v as f64).collect())
        }
        // A label id is a name, not a quantity: interpolating between two ids invents a third.
        Kind::Labels => Some(qsm_core::geometry::resample_labels_onto(data, src_dims, src_affine, dst.dims, &dst.affine)?),
        Kind::Continuous | Kind::Phase(_) | Kind::Projection(_) =>
            qsm_core::geometry::resample_onto(data, src_dims, src_affine, dst.dims, &dst.affine),
    })
}
