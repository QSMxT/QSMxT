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
pub(super) struct Citation {
    pub(super) key: &'static str,
    pub(super) text: Cow<'static, str>,
    pub(super) inline: Cow<'static, str>,
}

/// A catalogue record's reference-list entry: `Authors (year). "Title." *Venue*,
/// volume(issue):pages. https://doi.org/…`. An arXiv preprint gives its identifier after the
/// venue (`*arXiv*:2506.13293`), taken from its DOI. A work with no venue (unpublished) ends at
/// its title.
pub(super) fn reference_text(c: &catalogue::Citation) -> String {
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
/// are both "Dimov et al., 2022").
pub(super) fn cite_inline(cites: &[Citation]) -> String {
    let mut inline: Vec<&str> = Vec::new();
    for c in cites {
        if !inline.contains(&c.inline.as_ref()) {
            inline.push(&c.inline);
        }
    }
    inline.join("; ")
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

pub(super) const CITE_QSMXT: Citation = Citation {
    key: "stewart2026qsmxt",
    text: Cow::Borrowed("Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT"),
    inline: Cow::Borrowed("Stewart, 2026"),
};

pub(super) const CITE_QSMBLY: Citation = Citation {
    key: "stewart2026qsmbly",
    text: Cow::Borrowed("Stewart, A. (2026). QSMbly: Browser-based Quantitative Susceptibility Mapping. https://github.com/astewartau/qsmbly"),
    inline: Cow::Borrowed("Stewart, 2026"),
};
