//! Finding multi-orientation sets in a BIDS dataset.
//!
//! BIDS has no entity for "this is the same object at a different orientation". Practice is
//! split: OpenNeuro `ds007958` (in-vivo 7T COSMOS) uses `acq-dir1/2/3`, heudiconv uses
//! `chunk-` when dcm2niix splits one series by orientation, and plenty of datasets just use
//! `run-`. Rather than pick a winner, QSMxT takes a pattern and shows you what it matched.
//!
//! # The pattern language, in three tiers
//!
//! The field accepts one string, and almost everyone only ever needs the first tier:
//!
//! 1. **A bare entity name** — `acq`, `run`, `rec`, `inv` — expanded to `*<entity>-*`.
//! 2. **A glob** over the run key, in exactly the language the Include/Exclude filters
//!    already use (`*acq-dir*`). This is what distinguishes an orientation set from an
//!    unrelated acquisition that happens to share the entity:
//!
//!    ```text
//!    sub-01_ses-01_acq-dir1_MEGRE     ┐
//!    sub-01_ses-01_acq-dir2_MEGRE     ├─ *acq-dir* matches these three
//!    sub-01_ses-01_acq-dir3_MEGRE     ┘
//!    sub-01_ses-01_acq-highres_MEGRE  ← and not this one
//!    ```
//!
//!    "Group by the `acq` entity" would sweep the fourth file in; a glob does not.
//! 3. **`re:` and a regex with one capture group** — the escape hatch, for the only case a
//!    glob genuinely cannot express: two separate orientation sets in one session. The
//!    captured span is the orientation, everything outside it is the group key, so
//!    `re:.*acq-(?:lowres|highres)-dir(.+?)_.*` keeps the two sets apart.
//!
//! # Why the orientation label carries no ordering
//!
//! COSMOS and STI are both symmetric sums over orientations — neither cares which one is
//! "first". The only thing an ordering could decide is which orientation becomes the common
//! frame, and that is better chosen from the geometry than from whoever numbered the files.
//! So the capture group exists to *separate* groups, never to index within one.

use super::discovery::{matches_glob, QsmRun};
use super::entities::AcquisitionKey;

/// A compiled orientation-group pattern.
#[derive(Debug, Clone)]
pub enum GroupPattern {
    /// Tiers 1 and 2: runs matching this glob, within one subject+session, form a group.
    Glob(String),
    /// Tier 3: the capture group (when present) is removed from the run key to form the
    /// group key, so two sets in one session stay apart.
    Regex(regex::Regex),
}

/// Entity shorthands that expand to a glob. These are the entities QSMxT's filename parser
/// actually distinguishes; anything else would silently collapse two orientations into one
/// run, so [`parse_pattern`] rejects it instead.
const ENTITY_SHORTHANDS: &[&str] = &["acq", "run", "rec", "inv"];

/// Entities people reasonably reach for that QSMxT does not yet tell apart. Naming them
/// explicitly turns a silent mis-grouping into a message that says what to do instead.
const UNSUPPORTED_SHORTHANDS: &[&str] = &["chunk", "desc", "echo", "part", "ses", "sub"];

/// Compile a pattern. `Ok(None)` means multi-orientation grouping is off.
pub fn parse_pattern(pattern: &str) -> Result<Option<GroupPattern>, String> {
    let p = pattern.trim();
    if p.is_empty() || p.eq_ignore_ascii_case("none") {
        return Ok(None);
    }

    if let Some(rest) = p.strip_prefix("re:") {
        let re = regex::Regex::new(rest.trim())
            .map_err(|e| format!("orientation group: invalid regex '{}': {e}", rest.trim()))?;
        if re.captures_len() > 2 {
            return Err(format!(
                "orientation group: regex '{}' has {} capture groups; use at most one — it \
                 marks the part that varies between orientations",
                rest.trim(),
                re.captures_len() - 1
            ));
        }
        return Ok(Some(GroupPattern::Regex(re)));
    }

    // A bare word with no glob metacharacters is an entity shorthand.
    if p.chars().all(|c| c.is_ascii_alphanumeric()) {
        let lower = p.to_ascii_lowercase();
        if ENTITY_SHORTHANDS.contains(&lower.as_str()) {
            return Ok(Some(GroupPattern::Glob(format!("*{lower}-*"))));
        }
        if UNSUPPORTED_SHORTHANDS.contains(&lower.as_str()) {
            return Err(format!(
                "orientation group: QSMxT does not tell '{lower}-' apart when it reads BIDS \
                 filenames, so it cannot group by it. Use one of {} — `acq` is what published \
                 multi-orientation datasets use — or give a glob or `re:` pattern",
                ENTITY_SHORTHANDS.join(", ")
            ));
        }
        return Err(format!(
            "orientation group: '{p}' is not an entity QSMxT knows ({}). Did you mean a glob, \
             like '*{p}-*'?",
            ENTITY_SHORTHANDS.join(", ")
        ));
    }

    Ok(Some(GroupPattern::Glob(p.to_string())))
}

/// A set of runs that a pattern says are the same object at different orientations.
#[derive(Debug, Clone)]
pub struct OrientationGroup {
    /// Display label — the shared part of the members' keys, e.g. `sub-01_ses-01`.
    pub label: String,
    /// Indices into the slice that was grouped, in discovery order.
    pub members: Vec<usize>,
}

/// The part of a run key that members of one group share.
fn group_key_for(key: &AcquisitionKey, key_str: &str, pattern: &GroupPattern) -> Option<String> {
    match pattern {
        GroupPattern::Glob(glob) => {
            if !matches_glob(key_str, glob) {
                return None;
            }
            // An orientation set is always within one subject and session.
            Some(match &key.session {
                Some(ses) => format!("sub-{}_ses-{}", key.subject, ses),
                None => format!("sub-{}", key.subject),
            })
        }
        GroupPattern::Regex(re) => {
            let caps = re.captures(key_str)?;
            match caps.get(1) {
                // Everything outside the captured span identifies the group, so two sets that
                // differ elsewhere in the name do not merge.
                Some(m) => {
                    let mut label = String::with_capacity(key_str.len());
                    label.push_str(&key_str[..m.start()]);
                    label.push('*');
                    label.push_str(&key_str[m.end()..]);
                    Some(label)
                }
                None => Some(match &key.session {
                    Some(ses) => format!("sub-{}_ses-{}", key.subject, ses),
                    None => format!("sub-{}", key.subject),
                }),
            }
        }
    }
}

/// Group `(key, key_string)` pairs into orientation sets.
///
/// Groups of one are not returned: a single match is an ordinary run, not an orientation set,
/// and it should fall through to normal single-orientation processing rather than failing.
/// Callers that want to warn about a pattern matching too little should compare the group
/// count against what the user expected — which is exactly what the TUI preview is for.
pub fn group_keys(
    keys: &[(AcquisitionKey, String)],
    pattern: &GroupPattern,
) -> Vec<OrientationGroup> {
    // Insertion-ordered so groups come out in discovery order and runs are reproducible.
    let mut order: Vec<String> = Vec::new();
    let mut buckets: std::collections::HashMap<String, Vec<usize>> = Default::default();

    for (i, (key, key_str)) in keys.iter().enumerate() {
        if let Some(g) = group_key_for(key, key_str, pattern) {
            if !buckets.contains_key(&g) {
                order.push(g.clone());
            }
            buckets.entry(g).or_default().push(i);
        }
    }

    order
        .into_iter()
        .filter_map(|label| {
            let members = buckets.remove(&label).unwrap_or_default();
            (members.len() >= 2).then_some(OrientationGroup { label, members })
        })
        .collect()
}

/// Group discovered runs into orientation sets.
pub fn group_runs(runs: &[QsmRun], pattern: &GroupPattern) -> Vec<OrientationGroup> {
    let keys: Vec<(AcquisitionKey, String)> =
        runs.iter().map(|r| (r.key.clone(), r.key.to_string())).collect();
    group_keys(&keys, pattern)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(subject: &str, session: Option<&str>, acq: Option<&str>, run: Option<&str>) -> AcquisitionKey {
        AcquisitionKey {
            subject: subject.into(),
            session: session.map(Into::into),
            acquisition: acq.map(Into::into),
            reconstruction: None,
            inversion: None,
            run: run.map(Into::into),
            suffix: "MEGRE".into(),
        }
    }

    /// `(subject, session, acquisition, run)` as the tests write them.
    type KeySpec<'a> = (&'a str, Option<&'a str>, Option<&'a str>, Option<&'a str>);

    fn keys(specs: &[KeySpec]) -> Vec<(AcquisitionKey, String)> {
        specs
            .iter()
            .map(|&(s, ses, acq, run)| {
                let k = key(s, ses, acq, run);
                let disp = k.to_string();
                (k, disp)
            })
            .collect()
    }

    fn group(specs: &[KeySpec], pattern: &str) -> Vec<OrientationGroup> {
        let p = parse_pattern(pattern).unwrap().expect("pattern should be active");
        group_keys(&keys(specs), &p)
    }

    #[test]
    fn empty_pattern_is_off() {
        assert!(parse_pattern("").unwrap().is_none());
        assert!(parse_pattern("   ").unwrap().is_none());
        assert!(parse_pattern("none").unwrap().is_none());
    }

    #[test]
    fn entity_shorthand_expands_to_a_glob() {
        match parse_pattern("acq").unwrap().unwrap() {
            GroupPattern::Glob(g) => assert_eq!(g, "*acq-*"),
            other => panic!("expected a glob, got {other:?}"),
        }
    }

    /// The whole point of tier 2: a glob separates the orientation set from an unrelated
    /// acquisition that merely shares the entity.
    #[test]
    fn a_glob_excludes_an_unrelated_acquisition() {
        let specs = [
            ("01", Some("01"), Some("dir1"), None),
            ("01", Some("01"), Some("dir2"), None),
            ("01", Some("01"), Some("dir3"), None),
            ("01", Some("01"), Some("highres"), None),
        ];
        let groups = group(&specs, "*acq-dir*");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members, vec![0, 1, 2], "highres must not be swept in");
        assert_eq!(groups[0].label, "sub-01_ses-01");

        // ...whereas the entity shorthand does sweep it in, which is exactly why the glob tier
        // exists and why the TUI shows what was matched.
        let by_entity = group(&specs, "acq");
        assert_eq!(by_entity[0].members, vec![0, 1, 2, 3]);
    }

    #[test]
    fn groups_never_cross_subject_or_session() {
        let specs = [
            ("01", Some("01"), Some("dir1"), None),
            ("01", Some("01"), Some("dir2"), None),
            ("01", Some("02"), Some("dir1"), None),
            ("01", Some("02"), Some("dir2"), None),
            ("02", Some("01"), Some("dir1"), None),
            ("02", Some("01"), Some("dir2"), None),
        ];
        let groups = group(&specs, "*acq-dir*");
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].label, "sub-01_ses-01");
        assert_eq!(groups[1].label, "sub-01_ses-02");
        assert_eq!(groups[2].label, "sub-02_ses-01");
    }

    #[test]
    fn a_lone_match_is_not_an_orientation_group() {
        let specs = [
            ("01", None, Some("dir1"), None),
            ("02", None, Some("dir1"), None),
        ];
        assert!(group(&specs, "*acq-dir*").is_empty(), "one run per subject is not a set");
    }

    #[test]
    fn grouping_by_run_works_for_datasets_that_use_it() {
        let specs = [
            ("01", None, Some("gre"), Some("1")),
            ("01", None, Some("gre"), Some("2")),
            ("01", None, Some("gre"), Some("3")),
        ];
        let groups = group(&specs, "run");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members.len(), 3);
    }

    /// Tier 3, and the only case a glob cannot express: two orientation sets in one session.
    #[test]
    fn a_capture_group_keeps_two_sets_in_one_session_apart() {
        let specs = [
            ("01", Some("01"), Some("lowres-dir1"), None),
            ("01", Some("01"), Some("lowres-dir2"), None),
            ("01", Some("01"), Some("highres-dir1"), None),
            ("01", Some("01"), Some("highres-dir2"), None),
        ];
        // A glob merges them — the failure this tier fixes.
        assert_eq!(group(&specs, "*acq-*dir*").len(), 1);

        let groups = group(&specs, r"re:.*-dir(\d+)_.*");
        assert_eq!(groups.len(), 2, "the capture should separate lowres from highres");
        assert_eq!(groups[0].members, vec![0, 1]);
        assert_eq!(groups[1].members, vec![2, 3]);
    }

    #[test]
    fn a_regex_without_a_capture_falls_back_to_subject_and_session() {
        let specs = [
            ("01", Some("01"), Some("dir1"), None),
            ("01", Some("01"), Some("dir2"), None),
        ];
        let groups = group(&specs, "re:.*acq-dir.*");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].label, "sub-01_ses-01");
    }

    #[test]
    fn rejects_an_entity_qsmxt_cannot_tell_apart() {
        let err = parse_pattern("chunk").unwrap_err();
        assert!(err.contains("does not tell 'chunk-' apart"), "{err}");
        assert!(err.contains("acq"), "should point at the supported entity: {err}");
    }

    #[test]
    fn rejects_an_unknown_bare_word_with_a_glob_suggestion() {
        let err = parse_pattern("orientation").unwrap_err();
        assert!(err.contains("*orientation-*"), "{err}");
    }

    #[test]
    fn rejects_a_regex_with_too_many_captures() {
        let err = parse_pattern(r"re:(a)(b)").unwrap_err();
        assert!(err.contains("at most one"), "{err}");
    }

    #[test]
    fn rejects_an_invalid_regex() {
        assert!(parse_pattern("re:[unclosed").is_err());
    }

    #[test]
    fn matching_is_case_insensitive_like_the_other_filters() {
        let specs = [
            ("01", None, Some("DIR1"), None),
            ("01", None, Some("DIR2"), None),
        ];
        assert_eq!(group(&specs, "*acq-dir*").len(), 1);
    }
}
