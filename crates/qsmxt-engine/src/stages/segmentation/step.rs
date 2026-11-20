//! The segmentation step of a run ([`crate::run`]) ([`super`]).

use log::Level;
use serde_json::json;

use crate::plan::Stage;
use crate::plan::Step;
use crate::record::{Choice, Decision, Topic};
use crate::run::{Done, Run};
use crate::stages;
use crate::store::{Data, Item, StageStore, Supplied};

impl<S: StageStore + ?Sized> Run<'_, S> {
    /// Whole-brain parcellation of the combined magnitude by SynthSeg, with its lookup table — or
    /// a segmentation supplied with the scan, read as FreeSurfer ids, when one is asked for and
    /// fits the working grid.
    pub(crate) fn segmentation(&mut self, step: &Step) -> Result<(), S::Error> {
        let config = self.config;
        let c = &config.segmentation;
        let supplied: &[Item] = if c.custom_dseg_tool.is_some() { &[Item::SuppliedSegmentation] } else { &[] };
        let params = self.with_supplied(stages::segmentation::params(config), supplied);
        let Some(key) = self.begin(Stage::Segmentation, Some("synthseg"), &params, "Skipping segmentation (cached)",
                                   "Segmenting") else { return Ok(()) };
        let lookup = stages::segmentation::lookup_table(c.version);
        // Prefer a bring-your-own segmentation, as masking does.
        if let Some(tool) = &c.custom_dseg_tool {
            match self.store.supplied(&Item::SuppliedSegmentation) {
                Supplied::Found { origin, data, dims, affine }
                    if stages::supplied::fit(dims, &affine, &self.meta) != stages::supplied::Fit::Refused =>
                {
                    let item = Item::SuppliedSegmentation;
                    let data = stages::supplied::conform(&item, &origin, data, dims, &affine, &self.meta)?;
                    self.log(Level::Info, &format!("Using custom segmentation from {origin}"));
                    self.decide(Decision::new(Topic::SegmentationSource, Choice::Supplied,
                        "The segmentation was the one supplied with the scan.").with_detail(origin));
                    self.put(Item::Segmentation, Data::Real(&data))?;
                    self.put(Item::SegmentationLabels, Data::Text(&lookup))?;
                    return self.complete(Done {
                        stage: Stage::Segmentation, algorithm: Some("synthseg"), params: &params, outcome: None, key: &key,
                        outputs: &[Item::Segmentation, Item::SegmentationLabels],
                    }, "Segmentation (custom)");
                }
                Supplied::Found { origin, dims, .. } => {
                    self.log(Level::Warn, &format!(
                        "custom segmentation {origin} is {}x{}x{}, not on the scan's grid (the run's is {}x{}x{}) — computing instead",
                        dims.0, dims.1, dims.2, self.meta.dims.0, self.meta.dims.1, self.meta.dims.2));
                    self.decide(Decision::new(Topic::SegmentationSource, Choice::Computed,
                        "The supplied segmentation is not on the run's grid, so the scan was segmented.")
                        .with_detail(format!("{origin}: {}x{}x{}", dims.0, dims.1, dims.2)));
                }
                Supplied::Unreadable { origin, error } => {
                    self.log(Level::Warn, &format!(
                        "could not read custom segmentation {origin} ({error}) — computing instead"));
                    self.decide(Decision::new(Topic::SegmentationSource, Choice::Computed,
                        "The supplied segmentation could not be read, so the scan was segmented.")
                        .with_detail(format!("{origin}: {error}")));
                }
                Supplied::Missing { searched } => {
                    self.log(Level::Info, &format!(
                        "no custom segmentation under derivatives (tool: {tool}) — computing"));
                    self.decide(Decision::new(Topic::SegmentationSource, Choice::Computed,
                        "No supplied segmentation was found, so the scan was segmented.").with_detail(searched));
                }
            }
        }
        if !self.store.has(&Item::CombinedMagnitude) {
            self.log(Level::Warn, "Skipping segmentation: no combined magnitude");
            self.skip("There was no combined magnitude to segment.");
            return Ok(());
        }
        stages::segmentation::weights_available(c.version)?;
        self.store.models(&step.models)?;
        let labels = c.version.to_core().labels();
        self.log(Level::Info, &format!("Segmentation (SynthSeg {}, {} labels{}{})",
            c.version, labels.ids.len(),
            if c.flip_averaging { ", flip-averaged" } else { "" },
            if c.topology_cleanup { ", topology cleanup" } else { "" }));
        let magnitude = self.store.real(&Item::CombinedMagnitude)?;
        let seg = stages::segmentation::synthseg(&magnitude, &self.meta, c, self.observer)?;
        drop(magnitude);
        self.put(Item::Segmentation, Data::Real(&seg.labels))?;
        self.put(Item::SegmentationLabels, Data::Text(&lookup))?;
        self.put(Item::SegmentationVolumes, Data::Json(&json!(seg.volumes)))?;
        let present = labels.ids.iter().skip(1).zip(seg.volumes.iter().skip(1)).filter(|(_, &v)| v > 0.0).count();
        self.log(Level::Info, &format!("Segmented {} of {} structures", present, labels.ids.len() - 1));
        self.complete(Done {
            stage: Stage::Segmentation, algorithm: Some("synthseg"), params: &params, outcome: None, key: &key,
            outputs: &[Item::Segmentation, Item::SegmentationLabels, Item::SegmentationVolumes],
        }, "Segmentation")
    }
}
