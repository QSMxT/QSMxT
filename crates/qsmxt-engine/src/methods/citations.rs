//! The works the methods text cites, and how each is cited inline.
//!
//! A method's papers come from QSM.rs's method catalogue ([`qsm_core::catalogue`]): [`method`]
//! looks a method up by stage and id, and [`reference_text`] and [`inline_text`] write a
//! catalogue record in QSMxT's style. The constants below are the works that are not a catalogue
//! method's (QSMxT and QSMbly themselves, consensus papers, repositories), and
//! [`TEXT_OVERRIDES`] lists the records QSMxT writes in its own form.

use qsm_core::catalogue;
use std::borrow::Cow;

pub(super) use qsm_core::catalogue::Stage as MethodStage;

/// A work as the methods text cites it: its key (one per work, so a work cited twice is listed
/// once), its reference-list entry, and its in-text citation ("Kames et al., 2018").
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Citation {
    pub(super) key: &'static str,
    pub(crate) text: Cow<'static, str>,
    pub(super) inline: Cow<'static, str>,
}

/// A catalogue record's reference-list entry: `Authors (year). "Title." *Venue*,
/// volume(issue):pages. https://doi.org/…`. An arXiv preprint gives its identifier after the
/// venue (`*arXiv*:2506.13293`), taken from its DOI. A work with no venue (unpublished) ends at
/// its title.
pub(crate) fn reference_text(c: &catalogue::Citation) -> String {
    let stop = if c.title.ends_with(['.', '?', '!']) { "" } else { "." };
    let mut s = format!("{} ({}). \"{}{stop}\"", c.authors, c.year, c.title);
    if let Some(venue) = c.venue {
        s.push_str(&format!(" *{venue}*"));
        if venue == "arXiv" {
            if let Some(id) = c.doi.and_then(|d| d.strip_prefix("10.48550/arXiv.")) {
                s.push_str(&format!(":{id}"));
            }
        }
        match (c.volume, c.pages) {
            (Some(volume), _) => {
                s.push_str(&format!(", {volume}"));
                if let Some(issue) = c.issue {
                    s.push_str(&format!("({issue})"));
                }
                if let Some(pages) = c.pages {
                    s.push_str(&format!(":{pages}"));
                }
            }
            (None, Some(pages)) => s.push_str(&format!(", {pages}")),
            (None, None) => {}
        }
        s.push('.');
    }
    if let Some(doi) = c.doi {
        s.push_str(&format!(" https://doi.org/{doi}"));
    } else if let Some(url) = c.url {
        s.push_str(&format!(" {url}"));
    }
    s
}

/// A catalogue record's in-text citation: "Jenkinson, 2003", "Sun & Wilman, 2014", or the first
/// author's surname and "et al." for three or more authors or a list the record shortens.
pub(super) fn inline_text(c: &catalogue::Citation) -> String {
    // `authors` is "Surname, Initials., Surname, Initials.", perhaps ending "et al.".
    let parts: Vec<&str> = c.authors.split(", ").collect();
    let shortened = parts.last() == Some(&"et al.");
    let surnames: Vec<&str> = parts.iter().filter(|p| **p != "et al.").step_by(2).copied().collect();
    let who = match (surnames.as_slice(), shortened) {
        ([one], false) => one.to_string(),
        ([a, b], false) => format!("{a} & {b}"),
        _ => format!("{} et al.", surnames[0]),
    };
    format!("{who}, {}", c.year)
}

/// Works whose record QSMxT cites in a form [`reference_text`] does not write, each with why.
const TEXT_OVERRIDES: &[(&str, &str)] = &[
    // A conference abstract: cited by meeting volume and abstract number ("23, p.3313"), not as
    // a journal's volume and pages.
    ("li2015iharp", "Li, W., Wu, B., Liu, C. (2015). \"iHARPERELLA: an improved method for integrated 3D phase unwrapping and background phase removal.\" *Proc. ISMRM* 23, p.3313."),
    // A thesis: its degree and university are not a venue to set in italics.
    ("eckstein2021phd", "Eckstein, K. (2021). \"Improved Methods for Ultra-High Field Magnetic Susceptibility Imaging.\" PhD thesis, TU Wien. https://doi.org/10.34726/hss.2021.43447"),
];

/// A catalogue record as the methods text cites it.
fn from_record(c: &'static catalogue::Citation) -> Citation {
    let text = match TEXT_OVERRIDES.iter().find(|(key, _)| *key == c.key) {
        Some((_, text)) => Cow::Borrowed(*text),
        None => Cow::Owned(reference_text(c)),
    };
    Citation { key: c.key, text, inline: Cow::Owned(inline_text(c)) }
}

/// The catalogue's entry for method `id` in `stage`. Every method the methods text names has one
/// (the catalogue-coverage tests hold the engine's enums to it).
pub(super) fn catalogue_method(stage: MethodStage, id: &str) -> &'static catalogue::Method {
    catalogue::find(stage, id).unwrap_or_else(|| panic!("QSM.rs's catalogue has no {stage:?} method {id:?}"))
}

/// The works the methods text cites for method `id` in `stage`: the catalogue's, the method's own
/// first.
pub(super) fn method(stage: MethodStage, id: &str) -> Vec<Citation> {
    catalogue_method(stage, id).citations.iter().map(from_record).collect()
}

/// The catalogue's record of the work `key`, for a paper QSMxT cites for a step that is not
/// itself a catalogue method (UK Biobank's echo weighting is one of its pipeline's details).
pub(super) fn paper(key: &str) -> Citation {
    let record = catalogue::METHODS.iter().flat_map(|m| m.citations).find(|c| c.key == key)
        .unwrap_or_else(|| panic!("QSM.rs's catalogue cites no work {key:?}"));
    from_record(record)
}

/// The in-text citation of `cites`: each one's, joined by "; ", once each (R2*-QSM's two papers
/// are both "Dimov et al., 2022"; [`letter_sentences`] tells them apart once the whole text is
/// written).
pub(super) fn cite_inline(cites: &[Citation]) -> String {
    let mut inline: Vec<&str> = Vec::new();
    for c in cites {
        if !inline.contains(&c.inline.as_ref()) {
            inline.push(&c.inline);
        }
    }
    inline.join("; ")
}

/// In-text citations for a reference page that cites catalogue works (the docs' algorithm tables,
/// the README's): each work as the methods text cites it, and, as the methods text does, with a
/// letter after the year where different works on the page share a citation ("Liu et al.,
/// 2011a"). The letters follow the order the page first cites the works.
pub(crate) struct PageCitations {
    collisions: Vec<Collision>,
}

impl PageCitations {
    /// The citations of a page that cites `works`, in the order it cites them.
    pub(crate) fn new(works: impl IntoIterator<Item = &'static catalogue::Citation>) -> Self {
        let mut references: Vec<Citation> = Vec::new();
        for w in works {
            if !references.iter().any(|r| r.key == w.key) {
                references.push(from_record(w));
            }
        }
        Self { collisions: collisions(&references) }
    }

    /// `work`'s in-text citation on the page.
    pub(crate) fn inline(&self, work: &catalogue::Citation) -> String {
        self.collisions.iter().flat_map(|c| &c.works).find(|(key, _)| *key == work.key)
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| inline_text(work))
    }
}

// ─── Works that are not a catalogue method's ───

pub(super) const CITE_QSM_CONSENSUS: Citation = Citation {
    key: "bilgic2024consensus",
    text: Cow::Borrowed("Bilgic, B., Costagli, M., Chan, K.-S., et al. (2024). \"Recommended implementation of quantitative susceptibility mapping for clinical research in the brain: A consensus of the ISMRM electro-magnetic tissue properties study group.\" *Magnetic Resonance in Medicine*, 91(5):1834-1862. https://doi.org/10.1002/mrm.30006"),
    inline: Cow::Borrowed("Bilgic et al., 2024"),
};

pub(super) const CITE_QSMCI_SIGNAL_EROSION: Citation = Citation {
    key: "qsmci",
    text: Cow::Borrowed("QSM-CI: signal-gated mask erosion (QSM-CI harmonization masking, `hd-bet-qsmci`). https://github.com/QSMxT/QSM-CI"),
    inline: Cow::Borrowed("QSM-CI"),
};

pub(crate) const CITE_QSMXT: Citation = Citation {
    key: "stewart2026qsmxt",
    text: Cow::Borrowed("Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT"),
    inline: Cow::Borrowed("Stewart, 2026"),
};

pub(super) const CITE_QSMBLY: Citation = Citation {
    key: "stewart2026qsmbly",
    text: Cow::Borrowed("Stewart, A. (2026). QSMbly: Browser-based Quantitative Susceptibility Mapping. https://github.com/astewartau/qsmbly"),
    inline: Cow::Borrowed("Stewart, 2026"),
};

// ─── Telling apart works with the same in-text citation ───

/// Different works one methods text cites under the same in-text citation (R2*-QSM's two
/// "Dimov et al., 2022" papers), told apart in the usual author-year way: each gets a letter after
/// its year, in the text and in the reference list ("Dimov et al., 2022a", "(2022a)").
#[derive(Debug, PartialEq)]
pub(super) struct Collision {
    /// The in-text citation they share.
    pub(super) label: String,
    /// Each work's key and its in-text citation with its letter. Works that are the same work
    /// (one DOI under two keys) share a letter.
    pub(super) works: Vec<(&'static str, String)>,
}

/// A reference's DOI, from its entry's `https://doi.org/` link.
fn doi(c: &Citation) -> Option<&str> {
    c.text.split("https://doi.org/").nth(1).and_then(|s| s.split_whitespace().next())
}

/// The in-text citations `references` (the reference list, each work once by key) share between
/// different works. The letters follow the reference list, which lists works in the order the text
/// first cites them, so a reader finds "2022a" above "2022b".
pub(super) fn collisions(references: &[Citation]) -> Vec<Collision> {
    let mut labels: Vec<&str> = Vec::new();
    for c in references {
        if !labels.contains(&c.inline.as_ref()) {
            labels.push(&c.inline);
        }
    }
    labels.into_iter().filter_map(|label| {
        // Each distinct work under `label` in list order: by DOI where it has one, else by key.
        let mut works: Vec<(&str, &'static str)> = Vec::new(); // (identity, key)
        for c in references.iter().filter(|c| c.inline == label) {
            works.push((doi(c).unwrap_or(c.key), c.key));
        }
        let mut distinct: Vec<&str> = Vec::new();
        for (id, _) in &works {
            if !distinct.contains(id) {
                distinct.push(id);
            }
        }
        (distinct.len() > 1).then(|| Collision {
            label: label.to_string(),
            works: works.iter().map(|(id, key)| {
                let n = distinct.iter().position(|d| d == id).unwrap();
                (*key, format!("{label}{}", letter(n)))
            }).collect(),
        })
    }).collect()
}

/// The `n`th year suffix: a, b, …, z, aa, ab, ….
fn letter(n: usize) -> String {
    let mut s = String::new();
    let mut n = n + 1;
    while n > 0 {
        n -= 1;
        s.insert(0, (b'a' + (n % 26) as u8) as char);
        n /= 26;
    }
    s
}

/// `reference`'s list entry with its year lettered as `collisions` letter it: "(2022)" becomes
/// "(2022a)".
pub(super) fn lettered_reference(reference: &Citation, collisions: &[Collision]) -> String {
    let lettered = collisions.iter().flat_map(|c| &c.works).find(|(key, _)| *key == reference.key);
    let Some((_, label)) = lettered else { return reference.text.to_string() };
    let year = reference.inline.rsplit(", ").next().unwrap_or_default();
    let suffix = &label[reference.inline.len()..];
    reference.text.replacen(&format!("({year})"), &format!("({year}{suffix})"), 1)
}

/// The methods sentences' texts with each shared in-text citation in `collisions` replaced by the
/// lettered citations of the works it stands for: those the sentence cites, or, for a sentence
/// that names a work cited in an earlier one, the works of that group the last such sentence
/// cited. `sentences` are each sentence's text and the keys of the works it cites.
pub(super) fn letter_sentences(sentences: &mut [(String, Vec<&'static str>)], collisions: &[Collision]) {
    for collision in collisions {
        let mut last: Vec<&str> = Vec::new();
        for (text, cited) in sentences.iter_mut() {
            let here: Vec<&str> = collision.works.iter()
                .filter(|(key, _)| cited.contains(key))
                .map(|(_, label)| label.as_str())
                .collect();
            if !here.is_empty() {
                last = dedup(here);
            }
            if last.is_empty() {
                continue;
            }
            *text = replace_label(text, &collision.label, &last.join("; "));
        }
    }
}

fn dedup(labels: Vec<&str>) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    for l in labels {
        if !out.contains(&l) {
            out.push(l);
        }
    }
    out
}

/// `text` with every whole occurrence of `label` (not part of a longer word or year) replaced.
fn replace_label(text: &str, label: &str, with: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find(label) {
        let (before, after) = (&rest[..i], &rest[i + label.len()..]);
        let joined = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
        let whole = !joined(before.chars().last().or(out.chars().last()))
            && !joined(after.chars().next());
        out.push_str(before);
        out.push_str(if whole { with } else { label });
        rest = after;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work(key: &'static str, inline: &'static str, text: &'static str) -> Citation {
        Citation { key, text: Cow::Borrowed(text), inline: Cow::Borrowed(inline) }
    }

    const SMITH_X: &str = "Smith, A., et al. (2020). \"X.\" *J*, 1:1. https://doi.org/10.1/x";
    const SMITH_Y: &str = "Smith, A., et al. (2020). \"Y.\" *K*, 2:2. https://doi.org/10.1/y";

    /// Two works of one first author and year get a and b, in reference-list order, in the text
    /// and the list; another author's work is untouched.
    #[test]
    fn same_author_year_works_get_letters_in_list_order() {
        let (x, y) = (work("smith2020x", "Smith et al., 2020", SMITH_X), work("smith2020y", "Smith et al., 2020", SMITH_Y));
        let other = work("jones2020", "Jones, 2020", "Jones, B. (2020). \"Z.\"");
        for (refs, first, second) in [([x.clone(), other.clone(), y.clone()], "smith2020x", "smith2020y"),
                                      ([y.clone(), other.clone(), x.clone()], "smith2020y", "smith2020x")] {
            let c = collisions(&refs);
            assert_eq!(c, vec![Collision { label: "Smith et al., 2020".into(), works: vec![
                (first, "Smith et al., 2020a".into()), (second, "Smith et al., 2020b".into())] }]);
            let listed: Vec<String> = refs.iter().map(|r| lettered_reference(r, &c)).collect();
            assert!(listed[0].starts_with("Smith, A., et al. (2020a). "), "{listed:?}");
            assert_eq!(listed[1], other.text);
            assert!(listed[2].starts_with("Smith, A., et al. (2020b). "), "{listed:?}");

            let mut sentences = vec![
                ("Both (Smith et al., 2020).".to_string(), vec![first, second]),
                ("Not Smith et al., 2020a twice; Jones, 2020.".to_string(), vec!["jones2020"]),
                ("Again (Smith et al., 2020).".to_string(), vec![]),
                ("One (Smith et al., 2020; Jones, 2020).".to_string(), vec![second, "jones2020"]),
            ];
            letter_sentences(&mut sentences, &c);
            let texts: Vec<&str> = sentences.iter().map(|(t, _)| t.as_str()).collect();
            assert_eq!(texts, [
                "Both (Smith et al., 2020a; Smith et al., 2020b).",
                "Not Smith et al., 2020a twice; Jones, 2020.",
                "Again (Smith et al., 2020a; Smith et al., 2020b).",
                "One (Smith et al., 2020b; Jones, 2020).",
            ]);
        }
    }

    /// One work, under one key or two keys with one DOI, is one work: no letter.
    #[test]
    fn the_same_work_gets_no_letter() {
        let x = work("smith2020x", "Smith et al., 2020", SMITH_X);
        assert!(collisions(std::slice::from_ref(&x)).is_empty());
        assert!(collisions(&[x.clone(), work("smith2020x-again", "Smith et al., 2020", SMITH_X)]).is_empty());
    }

    #[test]
    fn letters_run_past_z() {
        assert_eq!([0, 1, 25, 26, 27].map(letter), ["a", "b", "z", "aa", "ab"]);
    }

    #[test]
    fn a_label_is_replaced_only_whole() {
        assert_eq!(replace_label("(Li et al., 2015; Bali et al., 2015; Li et al., 20151)", "Li et al., 2015", "L"),
                   "(L; Bali et al., 2015; Li et al., 20151)");
    }
}
