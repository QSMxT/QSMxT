//! The analysis step of a run ([`crate::run`]): statistics over the segmentation's regions
//! ([`super`]).

use log::Level;
use serde_json::Value;

use crate::cache::step_cache_key;
use crate::observe::StageEvent;
use crate::record::{Choice, Decision, Topic};
use crate::run::{Done, Run};
use crate::stages;
use crate::store::{Data, Item, StageStore};

impl<S: StageStore + ?Sized> Run<'_, S> {
    /// Per-structure statistics of every quantitative map the run made (or was given) over the
    /// segmentation ([`stages::analysis`]). The maps are the ones there, not the ones the
    /// configuration implies, so a supplied map is summarised the same as one this run computed,
    /// and a map that was enabled but could not be made simply contributes no rows. Last of the
    /// map-producing steps, so it summarises all of them.
    pub(crate) fn analysis(&mut self) -> Result<(), S::Error> {
        use stages::analysis::{self as a, MAPS};
        let Some(seg_item) = self.segmentation_item() else {
            self.log(Level::Warn, "Skipping analysis: no segmentation");
            self.skip("There was no segmentation to summarise the maps over.");
            return Ok(());
        };
        let mut maps: Vec<(Item, &str, &str)> = MAPS.into_iter().filter(|(i, ..)| self.store.has(i)).collect();
        // A supplied χ is summarised where the run made none.
        if self.config.separation.custom_qsm_tool.is_some() && !maps.iter().any(|(i, ..)| *i == Item::Chi)
            && self.store.has(&Item::SuppliedChi)
        {
            maps.insert(0, (Item::SuppliedChi, a::CHIMAP, "ppm"));
        }
        let identities: Vec<(&str, Value)> = maps.iter().map(|(i, name, _)| (*name, self.store.identity(i))).collect();
        let params = a::params(self.config.segmentation.version, self.store.identity(&seg_item), &identities);
        let key = step_cache_key("analysis", None, &params, self.config);
        if self.store.cached("analysis", &key) {
            self.log(Level::Info, "Skipping analysis (cached)");
            self.observer.stage("analysis", None, StageEvent::Cached);
            self.reuse("analysis");
            return Ok(());
        }
        if maps.is_empty() {
            self.log(Level::Warn, "Skipping analysis: the run produced no quantitative map to summarise");
            self.skip("The run made no quantitative map to summarise.");
            return Ok(());
        }
        if self.dry {
            self.pretend("analysis", None, &params, &key);
            return Ok(());
        }
        self.observer.stage("analysis", None, StageEvent::Started("Summarising maps per structure"));
        self.store.start("analysis");
        let seg = self.map(&seg_item)?;
        let labels = self.config.segmentation.version.to_core().labels();
        // SynthSeg's per-label volumes, when this run segmented the scan itself.
        let volumes: Option<Vec<f64>> = self.store.report(&Item::SegmentationVolumes)
            .and_then(|v| serde_json::from_value(v).ok());
        let mut table = String::from(a::HEADER);
        for (item, name, unit) in &maps {
            let map = self.map(item)?;
            if map.len() != seg.len() {
                let what = self.store.identity(item);
                return Err(crate::EngineError::DimensionMismatch(format!(
                    "segmentation has {} voxels but {} has {}", seg.len(), what.as_str().unwrap_or_default(), map.len())).into());
            }
            // What this map was referenced to, as referencing recorded it, so the table says what
            // its zero means. A map that was never referenced (the χ-separation maps) has no such
            // record, and its columns stay empty.
            let reference = a::ReferenceInfo::from_fields(&self.store.annotation(item));
            table.push_str(&a::map_rows(name, unit, &map, &seg, labels, volumes.as_deref(), reference.as_ref()));
        }
        let names: Vec<&str> = maps.iter().map(|(_, name, _)| *name).collect();
        self.log(Level::Info, &format!("Per-structure statistics for {}", names.join(", ")));
        self.decide(Decision::new(Topic::AnalysisMaps, Choice::Summarised, "The maps summarised per structure.")
            .with_detail(names.join(", ")));
        self.put(Item::RegionStats, Data::Text(&table))?;
        self.complete(Done {
            step: "analysis", algorithm: None, params: &params, outcome: None, key: &key, outputs: &[Item::RegionStats],
        }, "Per-structure analysis")
    }
}
