//! Which steps read which: what has to be recomputed when a step's output changes.
//!
//! A step's cache key ([`crate::cache::step_cache_key`]) covers its own parameters and the
//! configuration it reads, not its inputs. When a step is recomputed with other parameters, every
//! step that read its old output is stale. A store records what each step read when it was
//! computed ([`crate::store::Completed::inputs`], the plan's reads for it), and [`readers`] says
//! which recorded steps read what a step wrote: what each step reads depends on the plan (χ-sepnet's
//! toolbox route reads R2′ only where it can be measured or is supplied), so a fixed table would
//! recompute steps that never read the output. Every front end invalidates by it, so a re-run
//! recomputes the same steps whichever front end runs it.
//!
//! The fixed table ([`downstream_of`]) is what a step recorded before stores kept what it read
//! falls back to: it holds every edge any plan has, and more.

use crate::cache::RELIABLE_SUFFIX;
use crate::store::OUTPUT_SPACE;

/// The recorded steps that read what `step` wrote (`outputs`, as the store names them: [`crate::store::Item`]s,
/// `qsmxt`'s paths), and so are stale once it is computed again under another key or forgotten.
/// `recorded` is each recorded step's name with what it recorded reading.
///
/// A step that recorded what it read is stale exactly when it read one of `outputs`; one recorded
/// before stores kept that (`None`) is stale when the fixed table ([`downstream_of`]) says so.
/// The return to the acquired grid (`output_space`) neither makes a step stale nor is made stale
/// here: it rewrites other steps' outputs in place, and a store undoes it before any of them is
/// computed again or read, and redoes it after.
pub fn readers<'a, T: PartialEq + 'a>(
    step: &str, outputs: &[T], recorded: impl IntoIterator<Item = (&'a str, Option<&'a [T]>)>,
) -> Vec<String> {
    if step == OUTPUT_SPACE {
        return Vec::new();
    }
    let table = downstream_of(step);
    recorded.into_iter()
        .filter(|&(other, read)| other != step && other != OUTPUT_SPACE && match read {
            Some(read) => read.iter().any(|i| outputs.contains(i)),
            None => table.iter().any(|d| d == other),
        })
        .map(|(other, _)| other.to_string())
        .collect()
}

/// Every step that may have to be re-run when `step_name` changes, the two-pass steps included,
/// whatever the plan: what [`readers`] falls back to for a step recorded without what it read.
///
/// Two-pass duplicates the mask, background-removal and inversion steps under
/// [`RELIABLE_SUFFIX`], so each of those is stale exactly when its main-pass twin is; and the
/// `twopass` combination is stale whenever either pass's inversion is. Deriving that from
/// [`downstream_steps`] rather than listing it keeps one table to maintain — a stale entry here
/// means silently reusing a cached output computed with different parameters.
pub fn downstream_of(step_name: &str) -> Vec<String> {
    // A reliable-pass step invalidates what its main-pass twin invalidates, on its own side.
    if let Some(base) = step_name.strip_suffix(RELIABLE_SUFFIX) {
        let mut out: Vec<String> = downstream_steps(base).unwrap_or(&[])
            .iter()
            .filter(|ds| PASS_STEPS.contains(ds))
            .map(|ds| format!("{ds}{RELIABLE_SUFFIX}"))
            .collect();
        out.push("twopass".to_string());
        out.extend(CHI_CONSUMERS.iter().map(|s| s.to_string()));
        return out;
    }
    if step_name == "twopass" {
        return CHI_CONSUMERS.iter().map(|s| s.to_string()).collect();
    }

    let direct = downstream_steps(step_name).unwrap_or(&[]);
    let mut out: Vec<String> = direct.iter().map(|s| s.to_string()).collect();
    // SMWI weights the magnitude by the *referenced* susceptibility map, chi-separation starts
    // from it and the statistics summarise it, so all three are stale whenever referencing is.
    // Derived rather than listed in each of the seven sets that reach `reference`: a missing entry
    // there would show a new reconstruction with weighting from the old one, and nothing would
    // say so.
    for &derived in CHI_CONSUMERS.iter().skip(1).chain(["reference-singlepass"].iter()) {
        if out.iter().any(|d| d == "reference") && !out.iter().any(|d| d == derived) {
            out.push(derived.to_string());
        }
    }
    out.extend(
        direct.iter()
            .filter(|ds| PASS_STEPS.contains(ds))
            .map(|ds| format!("{ds}{RELIABLE_SUFFIX}")),
    );
    // The combination reads both passes' maps: a pass step, or anything a pass step reads,
    // changes it.
    if PASS_STEPS.contains(&step_name) || direct.iter().any(|ds| PASS_STEPS.contains(ds)) {
        out.push("twopass".to_string());
    }
    out
}

/// Referencing and everything that reads the referenced susceptibility map, in the order a run
/// produces them. Anything that invalidates the reconstruction invalidates all of it.
const CHI_CONSUMERS: [&str; 4] = ["reference", "smwi", "chi_separation", "analysis"];

/// The steps two-pass runs once per pass. Everything before them (loading, phase scaling, the
/// field map) is shared, and everything after them (referencing, output space) runs on the
/// combined map.
const PASS_STEPS: [&str; 5] = ["mask", "bgremove", "invert", "tgv", "qsmart"];

/// The steps that read `step_name`'s output directly; `None` for a name the table does not know,
/// which a test keeps from happening for any step the runner records.
pub fn downstream_steps(step_name: &str) -> Option<&'static [&'static str]> {
    Some(match step_name {
        "load" => &[
            "resample",
            "motion",
            "scale_phase",
            "tukey_window",
            "resample_axial",
            "inhomog",
            "magnitude",
            "mask",
            "swi",
            "t2star_r2star",
            "unwrap",
            "bgremove",
            "invert",
            "tgv",
            "qsmart",
            "reference",
        ],
        // No run records a step by this name (the resampling to an axial grid is `resample_axial`):
        // an entry the table has had since v9.0.0, left as it is.
        "resample" => &[
            "motion",
            "scale_phase",
            "inhomog",
            "magnitude",
            "mask",
            "swi",
            "t2star_r2star",
            "unwrap",
            "bgremove",
            "invert",
            "tgv",
            "qsmart",
            "reference",
        ],
        // The per-echo phase and magnitude every later stage reads (the combined magnitude, R2*,
        // the separation's echoes), as well as the field: through the Tukey window and the
        // resampling when they run, directly otherwise.
        "scale_phase" => &[
            "tukey_window",
            "resample_axial",
            "magnitude",
            "t2star_r2star",
            "chi_separation",
            "mask",
            "motion",
            "swi",
            "unwrap",
            "bgremove",
            "invert",
            "tgv",
            "qsmart",
            "reference",
        ],
        // Echoes the same steps read, on the acquired grid or the working one.
        "tukey_window" => &[
            "resample_axial",
            "magnitude",
            "t2star_r2star",
            "chi_separation",
            "mask",
            "motion",
            "swi",
            "unwrap",
            "bgremove",
            "invert",
            "tgv",
            "qsmart",
            "reference",
        ],
        "resample_axial" => &[
            "magnitude",
            "t2star_r2star",
            "chi_separation",
            "mask",
            "motion",
            "swi",
            "unwrap",
            "bgremove",
            "invert",
            "tgv",
            "qsmart",
            "reference",
        ],
        "inhomog" => &[
            "magnitude",
            "mask",
            "swi",
            "unwrap",
            "bgremove",
            "invert",
            "tgv",
            "qsmart",
            "reference",
        ],
        // Combined magnitude feeds masking, SWI, MEDI weighting, and QSMART vasculature.
        "magnitude" => &[
            "mask",
            "swi",
            "invert",
            "qsmart",
            "reference",
            "segmentation",
        ],
        // The reliable-phase mask is restricted to the brain mask.
        "mask" => &[
            "mask-reliable",
            "motion",
            "swi",
            "t2star_r2star",
            // R2 is fitted, and R2′ measured or predicted, inside the mask.
            "r2_r2prime",
            "unwrap",
            "bgremove",
            "invert",
            "tgv",
            "qsmart",
            "reference",
        ],
        // Motion correction replaces the echoes and shrinks the mask every later stage works in.
        "motion" => &[
            "swi",
            "t2star_r2star",
            "r2_r2prime",
            "unwrap",
            "mask-reliable",
            "bgremove",
            "invert",
            "tgv",
            "qsmart",
            "reference",
            "chi_separation",
        ],
        "swi" => &[],
        // R2* feeds both the R2' it is differenced into and the per-structure table it appears in.
        "t2star_r2star" => &["r2_r2prime", "chi_separation", "analysis"],
        "r2_r2prime" => &["chi_separation", "analysis"],
        "chi_separation" => &["analysis"],
        // The field map says where the field is defined, the mask the reconstructions and the
        // reliable-phase mask are restricted to.
        "unwrap" => &["mask-reliable", "bgremove", "invert", "tgv", "qsmart", "reference"],
        "bgremove" => &["invert", "reference"],
        "invert" | "tgv" | "qsmart" => &["reference"],
        // Both consumers of the referenced χ map: SMWI's weighting, and the per-structure stats.
        // Chi-separation starts from it too.
        "reference" => &["smwi", "chi_separation", "analysis"],
        "smwi" => &[],
        "segmentation" => &["analysis"],
        // The single-pass map of a two-pass run is summarised by the statistics too.
        "reference-singlepass" => &["analysis"],
        "twopass" => &["reference"],
        "analysis" | "output_space" => &[],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Changing the mask has to invalidate both passes and the combination — a reliable-pass
    /// inversion left cached against a new mask is the silent-wrong-output case this table exists
    /// to prevent.
    #[test]
    fn mask_invalidates_both_passes_and_the_combination() {
        let ds = downstream_of("mask");
        for step in ["mask-reliable", "bgremove", "invert", "tgv", "qsmart",
                     "bgremove-reliable", "invert-reliable", "tgv-reliable", "qsmart-reliable",
                     "twopass", "reference"] {
            assert!(ds.contains(&step.to_string()), "mask should invalidate {step}: {ds:?}");
        }
    }

    /// Motion correction replaces the echoes and the mask every later stage works in, so it is
    /// a parent of all of them (#277), and the mask and phase it reads are parents of it.
    #[test]
    fn motion_invalidates_everything_that_reads_its_output() {
        let ds = downstream_of("motion");
        for step in ["swi", "t2star_r2star", "unwrap", "mask-reliable", "bgremove", "invert", "tgv", "qsmart",
                     "bgremove-reliable", "invert-reliable", "twopass", "reference", "chi_separation", "analysis"] {
            assert!(ds.contains(&step.to_string()), "motion should invalidate {step}: {ds:?}");
        }
        for parent in ["mask", "scale_phase", "tukey_window", "resample_axial", "load", "resample"] {
            assert!(downstream_of(parent).contains(&"motion".to_string()), "{parent} should invalidate motion");
        }
    }

    /// A reliable-pass step invalidates its own side and the combination, and leaves the main
    /// pass's cached reconstruction alone — re-running it would double the cost of an edit that
    /// cannot have changed it.
    #[test]
    fn a_reliable_step_leaves_the_main_pass_cached() {
        let ds = downstream_of("mask-reliable");
        assert!(ds.contains(&"invert-reliable".to_string()), "{ds:?}");
        assert!(ds.contains(&"twopass".to_string()), "{ds:?}");
        assert!(ds.contains(&"reference".to_string()), "{ds:?}");
        assert!(!ds.contains(&"invert".to_string()), "main pass must stay cached: {ds:?}");
        assert!(!ds.contains(&"bgremove".to_string()), "main pass must stay cached: {ds:?}");
    }

    /// The combination is the last χ-producing step, so what depends on it is referencing and the
    /// two consumers of the referenced map: SMWI's weighting and the per-structure statistics.
    /// Neither pass's own reconstruction does.
    #[test]
    fn the_combination_only_invalidates_the_consumers_of_chi() {
        assert_eq!(downstream_of("twopass"),
                   vec!["reference".to_string(), "smwi".to_string(),
                        "chi_separation".to_string(), "analysis".to_string()]);
    }

    /// The statistics summarise every map over the segmentation, so any of them changing must
    /// invalidate them. `segmentation` is not otherwise upstream of `reference`, so it needs its
    /// own edge, and so do the relaxometry and chi-separation stages that reach none of the χ
    /// steps.
    #[test]
    fn the_analysis_follows_all_of_its_inputs() {
        assert_eq!(downstream_of("segmentation"), vec!["analysis".to_string()]);
        for step in ["reference", "invert", "mask", "magnitude", "load",
                     "t2star_r2star", "r2_r2prime", "chi_separation"] {
            assert!(downstream_of(step).contains(&"analysis".to_string()),
                    "{step} should invalidate analysis: {:?}", downstream_of(step));
        }
        // The magnitude feeds the segmentation, not just the reconstruction.
        assert!(downstream_of("magnitude").contains(&"segmentation".to_string()));
        assert!(downstream_of("analysis").is_empty());
    }

    /// R2* is estimated, R2 fitted and R2′ measured inside the brain mask. R2 has no R2* step
    /// between it and the mask in a run that maps R2 alone, so it needs its own edge.
    #[test]
    fn relaxometry_follows_the_mask_it_is_computed_in() {
        for step in ["t2star_r2star", "r2_r2prime"] {
            assert!(downstream_of("mask").contains(&step.to_string()), "{step}: {:?}", downstream_of("mask"));
        }
    }

    /// The combination reads the main pass's map as well as the reliable pass's, so a main-pass
    /// reconstruction recorded under a new key recomputes it, whether or not the reliable pass's is.
    #[test]
    fn a_main_pass_reconstruction_invalidates_the_combination() {
        for step in ["invert", "tgv", "qsmart"] {
            assert!(downstream_of(step).contains(&"twopass".to_string()), "{step}: {:?}", downstream_of(step));
        }
    }

    /// Steps with nothing downstream must not grow a two-pass tail.
    #[test]
    fn leaf_steps_stay_leaves() {
        assert!(downstream_of("swi").is_empty());
        assert!(downstream_of("smwi").is_empty());
    }

    /// SMWI reads the referenced χ map, so anything that changes χ has to invalidate it — right
    /// back through the inversion and the mask. Otherwise a new reconstruction is displayed with
    /// weighting computed from the old one.
    #[test]
    fn smwi_is_invalidated_by_everything_that_changes_chi() {
        for step in ["mask", "unwrap", "bgremove", "invert", "tgv", "qsmart", "reference", "load"] {
            assert!(downstream_of(step).contains(&"smwi".to_string()),
                    "{step} should invalidate smwi: {:?}", downstream_of(step));
        }
        // Both passes of a two-pass run, too.
        assert!(downstream_of("invert-reliable").contains(&"reference".to_string()));
        assert!(downstream_of("twopass").contains(&"reference".to_string()));
    }

    #[test]
    fn test_downstream_steps_load() {
        let ds = downstream_steps("load").unwrap();
        assert!(ds.contains(&"mask"));
        assert!(ds.contains(&"unwrap"));
        assert!(ds.contains(&"reference"));
    }

    #[test]
    fn test_downstream_steps_unknown() {
        assert!(downstream_steps("nonexistent").is_none());
    }

    #[test]
    fn test_downstream_steps_leaf() {
        assert!(downstream_steps("swi").unwrap().is_empty());
        assert!(downstream_steps("smwi").unwrap().is_empty());
        // `reference` is not a leaf: SMWI, chi-separation and the per-structure stats all read
        // the referenced map.
        assert_eq!(downstream_steps("reference").unwrap(), &["smwi", "chi_separation", "analysis"]);
        assert!(downstream_steps("analysis").unwrap().is_empty());
    }
}
