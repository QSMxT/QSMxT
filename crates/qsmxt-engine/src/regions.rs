//! Parcellation regions usable as a QSM reference.
//!
//! A susceptibility map has no absolute zero — only differences within it mean anything — so every
//! map is referenced to something. The brain-mask mean (`mean`) is the default because it needs no
//! extra information, but it moves with whatever else is in the mask: two subjects with different
//! amounts of iron-rich tissue in the field of view get different zeros. Referencing to a
//! structure instead pins the zero to tissue, which is what makes values comparable between
//! subjects and studies.
//!
//! The regions come from the SynthSeg parcellation, so the names and ids here are FreeSurfer's and
//! the slugs are derived from the label table rather than listed by hand — a label table that
//! gains an entry gains a reference option with it.

use std::sync::LazyLock;

use crate::enums::SynthSegVersion;

/// Named unions of labels, for references that are not one structure.
///
/// Deliberately short. Anything else is spelled as a list (`left-thalamus,right-caudate`), which
/// covers the general case without a vocabulary to memorise.
///
/// `ventricles` is *not* called `csf`: SynthSeg 2.0 has a label of its own named `csf` (id 24),
/// and having `--qsm-reference csf` mean one thing on v1 and another on v2 is exactly the kind of
/// silent difference that makes two studies incomparable.
pub const COMPOSITES: &[(&str, &[i32])] = &[
    // Lateral (+ inferior lateral), 3rd and 4th ventricles — the CSF spaces a QSM paper means by
    // "CSF reference". Excludes label 24, which is v2-only and covers extraventricular CSF too.
    ("ventricles", &[4, 5, 14, 15, 43, 44]),
];

/// A region the reference can be set to, as the pickers present it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionOption {
    /// What the user types or picks, e.g. `left-thalamus`.
    pub slug: &'static str,
    /// Human-readable, e.g. `left thalamus`.
    pub label: String,
    /// The FreeSurfer ids it resolves to.
    pub ids: Vec<i32>,
    /// Only available on SynthSeg 2.0 (its label set is v1's plus `csf`).
    pub v2_only: bool,
}

/// `"left lateral ventricle"` -> `"left-lateral-ventricle"`.
fn slugify(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join("-").to_lowercase()
}

/// The side-free form of a label name, when it has a side: `"left thalamus"` -> `"thalamus"`.
fn without_side(name: &str) -> Option<&str> {
    name.strip_prefix("left ").or_else(|| name.strip_prefix("right "))
}

/// Intern a slug so the pickers can hold `&'static str`.
///
/// The set is fixed, bounded by the label table, and built once per process.
fn intern(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Every region option, in the order the pickers should show them: composites first, then one
/// entry per structure with both sides merged, then the individual sided labels.
///
/// Built against SynthSeg 2.0, whose label set is v1's plus `csf` — so the list is a superset and
/// the v1-invalid entries are the ones flagged `v2_only`. [`resolve`] is what enforces the
/// version; this is only the menu.
pub static REGIONS: LazyLock<Vec<RegionOption>> = LazyLock::new(|| {
    let table = qsm_core::segment::SynthSegVersion::V2.labels();
    let v1: Vec<i32> = qsm_core::segment::SynthSegVersion::V1.labels().ids.to_vec();
    let mut out = Vec::new();

    for (name, ids) in COMPOSITES {
        out.push(RegionOption {
            slug: name,
            label: (*name).to_string(),
            ids: ids.to_vec(),
            v2_only: false,
        });
    }

    // Both sides at once, plus the structures that have no side. Referencing to one hemisphere
    // alone is rarely what anyone means, so these lead.
    for (&id, &name) in table.ids.iter().zip(table.names) {
        if id == 0 {
            continue;
        }
        match without_side(name) {
            Some(base) => {
                // Emit once, on the left member, paired with its right counterpart.
                if !name.starts_with("left ") {
                    continue;
                }
                let ids: Vec<i32> = table.ids.iter().zip(table.names)
                    .filter(|(_, n)| without_side(n) == Some(base))
                    .map(|(&i, _)| i)
                    .collect();
                out.push(RegionOption {
                    slug: intern(slugify(base)),
                    label: base.to_string(),
                    v2_only: ids.iter().any(|i| !v1.contains(i)),
                    ids,
                });
            }
            None => out.push(RegionOption {
                slug: intern(slugify(name)),
                label: name.to_string(),
                ids: vec![id],
                v2_only: !v1.contains(&id),
            }),
        }
    }

    // Then each label on its own, for a genuinely one-sided reference.
    for (&id, &name) in table.ids.iter().zip(table.names) {
        if id == 0 || without_side(name).is_none() {
            continue;
        }
        out.push(RegionOption {
            slug: intern(slugify(name)),
            label: name.to_string(),
            ids: vec![id],
            v2_only: !v1.contains(&id),
        });
    }
    out
});

/// The labels available on a SynthSeg generation, as `(id, name)`.
fn labels_for(version: SynthSegVersion) -> impl Iterator<Item = (i32, &'static str)> {
    let table = match version {
        SynthSegVersion::V1 => qsm_core::segment::SynthSegVersion::V1.labels(),
        SynthSegVersion::V2 => qsm_core::segment::SynthSegVersion::V2.labels(),
    };
    table.ids.iter().copied().zip(table.names.iter().copied())
}

/// Resolve a reference-region spec to the FreeSurfer ids it covers.
///
/// A spec is a comma-separated list of terms, each one of: a composite name (`ventricles`), a
/// structure with both sides merged (`thalamus`), an exact label (`left-thalamus`), or a raw
/// FreeSurfer id (`10`). The result is the union, deduplicated and sorted, so `thalamus` and
/// `left-thalamus,right-thalamus` are the same reference.
///
/// Errors name what went wrong and list what would have worked — a mistyped region is otherwise a
/// long run that ends in a map referenced to nothing anyone intended.
pub fn resolve(spec: &str, version: SynthSegVersion) -> Result<Vec<i32>, String> {
    let mut ids: Vec<i32> = Vec::new();
    let terms: Vec<&str> = spec.split(',').map(str::trim).filter(|t| !t.is_empty()).collect();
    if terms.is_empty() {
        return Err("empty reference region: name a structure, e.g. `thalamus`".into());
    }
    for term in terms {
        ids.extend(resolve_term(term, version)?);
    }
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

fn resolve_term(term: &str, version: SynthSegVersion) -> Result<Vec<i32>, String> {
    let lower = term.to_lowercase();

    // A raw FreeSurfer id.
    if let Ok(id) = lower.parse::<i32>() {
        if id == 0 {
            return Err("0 is the background label, not a structure".into());
        }
        return match labels_for(version).find(|(i, _)| *i == id) {
            Some(_) => Ok(vec![id]),
            None => Err(format!("no label {id} in SynthSeg {version}")),
        };
    }

    if let Some((_, ids)) = COMPOSITES.iter().find(|(n, _)| *n == lower) {
        // A composite only means what it says if every part of it exists.
        let missing: Vec<i32> = ids.iter().copied()
            .filter(|id| !labels_for(version).any(|(i, _)| i == *id))
            .collect();
        if !missing.is_empty() {
            return Err(format!("`{lower}` needs labels {missing:?}, which SynthSeg {version} does not have"));
        }
        return Ok(ids.to_vec());
    }

    let matched: Vec<i32> = labels_for(version)
        .filter(|(_, name)| {
            slugify(name) == lower || without_side(name).map(slugify).as_deref() == Some(&lower)
        })
        .map(|(id, _)| id)
        .collect();
    if !matched.is_empty() {
        return Ok(matched);
    }

    // Not a label on this generation — say whether it would be on the other one, since
    // `--qsm-reference csf` on v1 is the mistake that will actually happen.
    let other = match version {
        SynthSegVersion::V1 => SynthSegVersion::V2,
        SynthSegVersion::V2 => SynthSegVersion::V1,
    };
    if labels_for(other).any(|(_, name)| {
        slugify(name) == lower || without_side(name).map(slugify).as_deref() == Some(&lower)
    }) {
        return Err(format!(
            "`{lower}` is a SynthSeg {other} label, but this run uses SynthSeg {version} — \
             pass --synthseg-version {other} to use it"));
    }
    Err(format!("unknown reference region `{term}`. Valid regions: {}", valid_list(version)))
}

/// Every accepted region name for a generation, comma-separated, for an error message.
pub fn valid_list(version: SynthSegVersion) -> String {
    let mut names: Vec<&str> = REGIONS.iter()
        .filter(|r| !(r.v2_only && version == SynthSegVersion::V1))
        .map(|r| r.slug)
        .collect();
    names.dedup();
    names.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bare structure name means both sides. Referencing to one hemisphere is a deliberate act,
    /// not something a user should get by typing the obvious thing.
    #[test]
    fn a_bare_name_merges_both_sides() {
        assert_eq!(resolve("thalamus", SynthSegVersion::V2).unwrap(), vec![10, 49]);
        assert_eq!(resolve("left-thalamus", SynthSegVersion::V2).unwrap(), vec![10]);
        assert_eq!(resolve("right-thalamus", SynthSegVersion::V2).unwrap(), vec![49]);
        // The common white-matter reference falls out of the label names, with no special case.
        assert_eq!(resolve("cerebral-white-matter", SynthSegVersion::V2).unwrap(), vec![2, 41]);
    }

    /// A list is the general case; the composites are just names for particular lists.
    #[test]
    fn lists_union_and_match_their_composite() {
        assert_eq!(resolve("ventricles", SynthSegVersion::V2).unwrap(), vec![4, 5, 14, 15, 43, 44]);
        assert_eq!(
            resolve("left-lateral-ventricle,right-lateral-ventricle,3rd-ventricle,4th-ventricle,\
                     left-inferior-lateral-ventricle,right-inferior-lateral-ventricle",
                    SynthSegVersion::V2).unwrap(),
            resolve("ventricles", SynthSegVersion::V2).unwrap());
        // Overlapping terms are one reference, not a double-counted one.
        assert_eq!(resolve("thalamus,left-thalamus", SynthSegVersion::V2).unwrap(), vec![10, 49]);
        // Raw ids, and mixed with names.
        assert_eq!(resolve("10,49", SynthSegVersion::V2).unwrap(), vec![10, 49]);
        assert_eq!(resolve(" thalamus , 11 ", SynthSegVersion::V2).unwrap(), vec![10, 11, 49]);
    }

    /// `csf` is a v2 label, and `ventricles` is the composite. Confusing the two would silently
    /// reference two studies to different tissue.
    #[test]
    fn csf_is_the_v2_label_and_never_the_composite() {
        assert_eq!(resolve("csf", SynthSegVersion::V2).unwrap(), vec![24]);
        assert_ne!(resolve("csf", SynthSegVersion::V2).unwrap(),
                   resolve("ventricles", SynthSegVersion::V2).unwrap());

        // On v1 it does not exist, and the error says how to get it rather than just "unknown".
        let err = resolve("csf", SynthSegVersion::V1).unwrap_err();
        assert!(err.contains("--synthseg-version v2"), "unhelpful error: {err}");
    }

    /// A mistyped region must fail loudly at configuration time, listing what would have worked.
    #[test]
    fn unknown_regions_are_rejected_with_the_valid_list() {
        let err = resolve("hippocampuss", SynthSegVersion::V2).unwrap_err();
        assert!(err.contains("unknown reference region"), "{err}");
        assert!(err.contains("hippocampus"), "the list should name the near miss: {err}");

        assert!(resolve("0", SynthSegVersion::V2).unwrap_err().contains("background"));
        assert!(resolve("999", SynthSegVersion::V2).unwrap_err().contains("no label 999"));
        assert!(resolve("", SynthSegVersion::V2).is_err());
        assert!(resolve(" , ", SynthSegVersion::V2).is_err());
    }

    /// The menu and the parser must agree: every slug offered has to resolve.
    #[test]
    fn every_offered_region_resolves_to_its_ids() {
        for r in REGIONS.iter() {
            let version = if r.v2_only { SynthSegVersion::V2 } else { SynthSegVersion::V1 };
            let got = resolve(r.slug, version)
                .unwrap_or_else(|e| panic!("offered region `{}` does not resolve: {e}", r.slug));
            let mut want = r.ids.clone();
            want.sort_unstable();
            assert_eq!(got, want, "region `{}`", r.slug);
            assert!(!r.ids.contains(&0), "region `{}` includes the background", r.slug);
        }
        // No duplicate slugs — a repeated entry in the picker is a picker that lies about which
        // one you chose.
        let mut slugs: Vec<&str> = REGIONS.iter().map(|r| r.slug).collect();
        slugs.sort_unstable();
        let n = slugs.len();
        slugs.dedup();
        assert_eq!(slugs.len(), n, "duplicate region slugs");
    }
}
