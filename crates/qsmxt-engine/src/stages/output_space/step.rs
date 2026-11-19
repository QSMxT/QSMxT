//! The output-space step of a run ([`crate::run`]): the maps returned to the acquired grid
//! ([`super`]).

use log::Level;

use crate::record::{Choice, Decision, Topic};
use crate::run::{Done, Run};
use crate::stages;
use crate::store::{Data, Item, StageStore};

impl<S: StageStore + ?Sized> Run<'_, S> {
    /// The outputs of a run resampled to axial, back on the grid it was acquired on
    /// ([`stages::output_space`]) — or, with `--output-space working`, left where they are.
    pub(crate) fn output_space(&mut self) -> Result<(), S::Error> {
        use stages::output_space::{self as o, Kind};
        // Not resampled: the outputs are on the acquired grid already (the record's `resampled`).
        let Some((dst_dims, dst_affine)) = self.meta.source_geometry else { return Ok(()) };
        if self.config.pipeline.output_space != crate::OutputSpace::Acquired {
            self.log(Level::Info, "Leaving outputs on the resampled grid (--output-space working)");
            return self.store.keep_working_grid();
        }
        let params = o::params(dst_dims);
        let Some(key) = self.begin("output_space", None, &params, "Skipping output_space (cached)",
                                   "Resampling outputs to the acquired grid") else { return Ok(()) };
        let (src_dims, src_affine) = (self.meta.dims, self.meta.affine);
        self.log(Level::Info, &format!(
            "Returning outputs to the acquired grid: {}x{}x{} -> {}x{}x{}",
            src_dims.0, src_dims.1, src_dims.2, dst_dims.0, dst_dims.1, dst_dims.2));
        let published: Vec<(Item, Kind)> = o::published(self.n_echoes()).into_iter()
            .filter(|(i, _)| i.published_in(self.config) && self.store.has(i)).collect();
        let dst = o::acquired(dst_dims, &dst_affine);
        let src_voxels = src_dims.0 * src_dims.1 * src_dims.2;
        let mut done: Vec<Item> = Vec::new();
        // Phase is only meaningful alongside its magnitude, so those pairs go first, together.
        for &(phase_item, kind) in &published {
            let Kind::Phase(mag_item) = kind else { continue };
            if !published.iter().any(|(i, _)| *i == mag_item) {
                let what = self.store.identity(&phase_item);
                self.log(Level::Warn, &format!(
                    "Leaving {} on the working grid: wrapped phase can only be resampled with its \
                     magnitude, and there is none beside it", what.as_str().unwrap_or_default()));
                self.decide(Decision::new(Topic::OutputGrid, Choice::Kept,
                    "Phase without a magnitude beside it was left on the working grid.")
                    .with_detail(what.as_str().unwrap_or_default()));
                done.push(phase_item);
                continue;
            }
            let (phase, magnitude) = (self.store.real(&phase_item)?, self.store.real(&mag_item)?);
            if let Some((m, p)) = qsm_core::geometry::resample_complex_onto(
                &magnitude, &phase, src_dims, &src_affine, dst_dims, &dst_affine,
            ) {
                self.put(phase_item, Data::OnGrid(&p, &dst))?;
                self.put(mag_item, Data::OnGrid(&m, &dst))?;
                done.extend([phase_item, mag_item]);
            }
        }
        for &(item, kind) in &published {
            if done.contains(&item) || matches!(kind, Kind::Projection(_)) {
                continue;
            }
            let data = self.store.real(&item)?;
            // Resampling indexes the source by the working grid, so anything on a different grid
            // would be read out of bounds. Leave it where it is rather than corrupt it.
            if data.len() != src_voxels {
                let what = self.store.identity(&item);
                self.log(Level::Warn, &format!(
                    "Leaving {} on its own grid: {} voxels, not the {} of the working grid",
                    what.as_str().unwrap_or_default(), data.len(), src_voxels));
                self.decide(Decision::new(Topic::OutputGrid, Choice::Kept,
                    "An output not on the working grid was left on its own grid.")
                    .with_detail(what.as_str().unwrap_or_default()));
                continue;
            }
            if let Some(out) = o::resample(&data, kind, (&src_dims, &src_affine), &dst)? {
                self.put(item, Data::OnGrid(&out, &dst))?;
            }
        }
        // A projection cannot be interpolated onto the acquired grid as if it were a volume — its
        // slices are slabs, not samples — so each is rebuilt from its source, now on that grid.
        let dst_grid = qsm_core::Grid::new(dst_dims.0, dst_dims.1, dst_dims.2, dst.voxel_size.0, dst.voxel_size.1, dst.voxel_size.2);
        for &(item, kind) in &published {
            let Kind::Projection(source) = kind else { continue };
            if !self.store.has(&source) {
                continue;
            }
            let window = if source == Item::Swi { self.config.swi.mip_window } else { self.config.smwi.mip_window };
            let volume = self.store.real(&source)?;
            let (mip, geometry) = stages::swi::project_on(&volume, &dst_grid, &dst_affine, window)?;
            self.put(item, Data::OnGrid(&mip, &geometry))?;
        }
        // Read: the working-grid versions each output was returned from.
        let items: Vec<Item> = published.iter().map(|(i, _)| *i).collect();
        self.complete(Done {
            step: "output_space", algorithm: None, params: &params, outcome: None, key: &key, outputs: &items,
        }, "Output space")
    }
}
