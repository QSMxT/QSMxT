//! Which steps read which: what has to be recomputed when a step's output changes.
//!
//! A step's cache key ([`crate::cache::step_cache_key`]) covers its own parameters and the
//! configuration it reads, not its inputs. When a step is recomputed with other parameters, every
//! step that read its old output is stale, and this table says which those are. Every front end
//! invalidates by it, so a re-run recomputes the same steps whichever front end runs it.

use crate::cache::RELIABLE_SUFFIX;

/// Every step that has to be re-run when `step_name` changes, the two-pass steps included.
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
    if direct.iter().any(|ds| PASS_STEPS.contains(ds)) {
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
        // the separation's echoes), as well as the field.
        "scale_phase" => &[
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
        "mask" => &[
            "motion",
            "swi",
            "t2star_r2star",
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
        "unwrap" => &["bgremove", "invert", "tgv", "qsmart", "reference"],
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
        for step in ["bgremove", "invert", "tgv", "qsmart",
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
        for parent in ["mask", "scale_phase", "load", "resample"] {
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
