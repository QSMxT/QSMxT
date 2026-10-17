//! Files a front end supplies for a run in place of one the run would compute, and where it writes
//! them so that `qsmxt run` reads them the same way.
//!
//! A brain mask drawn, edited or uploaded in a front end (qsmbly) is used for that run as a mask
//! supplied with the scan. Written as a BIDS derivative of the front end's tool under
//! [`supplied_mask_path`], it is the mask `qsmxt run --use-custom-masks <tool>` finds for the run,
//! so the exported command reproduces the run.

/// The suffix of a brain mask: the run's own brain mask is `<prefix>_mask` too ([`crate::store::Item::BrainMask`]).
const MASK: &str = "mask";

/// Where a front end writes a brain mask it supplies for a run, relative to the BIDS root:
/// `derivatives/<tool>/sub-<label>/[ses-<label>/]anat/<prefix>_mask.nii.gz`.
///
/// `prefix` is the run's BIDS prefix: its entities up to the suffix, `sub-<label>` first and
/// `ses-<label>` next when there is one (`sub-01_ses-pre_acq-gre_run-1`). `qsmxt run
/// --use-custom-masks <tool>` takes a mask named for the run over the subject's (or session's)
/// other masks, so each run of a subject can have its own.
///
/// An error says what is wrong with `tool` or `prefix`.
pub fn supplied_mask_path(tool: &str, prefix: &str) -> Result<String, String> {
    let label = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric());
    if !tool.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)) || tool.is_empty() || tool.starts_with('.') {
        return Err(format!("'{tool}' cannot name a derivatives directory"));
    }
    let mut entities = Vec::new();
    for part in prefix.split('_') {
        match part.split_once('-') {
            Some((key, value)) if label(key) && label(value) => entities.push((key, value)),
            _ => return Err(format!("'{prefix}' is not a BIDS prefix: '{part}' is not key-value")),
        }
    }
    let Some(&("sub", subject)) = entities.first() else {
        return Err(format!("'{prefix}' is not a BIDS prefix: it must start with sub-<label>"));
    };
    let mut dir = format!("derivatives/{tool}/sub-{subject}");
    if let Some(&("ses", session)) = entities.get(1) {
        dir.push_str(&format!("/ses-{session}"));
    }
    if entities.iter().skip(1).any(|&(k, _)| k == "sub") || entities.iter().skip(2).any(|&(k, _)| k == "ses") {
        return Err(format!("'{prefix}' is not a BIDS prefix: sub- comes first, ses- next"));
    }
    Ok(format!("{dir}/anat/{prefix}_{MASK}.nii.gz"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_supplied_mask_is_named_for_its_run_under_the_tools_derivatives() {
        assert_eq!(supplied_mask_path("qsmbly", "sub-01").unwrap(), "derivatives/qsmbly/sub-01/anat/sub-01_mask.nii.gz");
        assert_eq!(
            supplied_mask_path("qsmbly", "sub-01_ses-pre_acq-gre_run-2").unwrap(),
            "derivatives/qsmbly/sub-01/ses-pre/anat/sub-01_ses-pre_acq-gre_run-2_mask.nii.gz",
        );
        assert_eq!(crate::store::Item::BrainMask.meta().bids_name.as_deref(), Some(MASK), "named as the run's own mask");
    }

    #[test]
    fn a_tool_or_prefix_that_cannot_be_a_path_is_refused() {
        for (tool, prefix) in [("", "sub-01"), ("*", "sub-01"), ("../x", "sub-01"), ("a/b", "sub-01"), (".", "sub-01"),
                               ("qsmbly", "01"), ("qsmbly", "ses-1_sub-01"), ("qsmbly", "sub-01_run-1_ses-2"),
                               ("qsmbly", "sub-01_mask"), ("qsmbly", "sub-0/1"), ("qsmbly", "sub-01__run-1")] {
            assert!(supplied_mask_path(tool, prefix).is_err(), "{tool} {prefix}");
        }
    }
}
