//! The BIDS names `qsmxt dicom-convert` gives each series, and the names of dcm2niix's outputs.
//!
//! | Series | Name |
//! |---|---|
//! | gradient-echo magnitude / phase / real / imaginary, one echo | `…_part-mag_T2starw` |
//! | the same, several echoes | `…_echo-N_part-mag_MEGRE` |
//! | T1-weighted structural | `…_T1w` |
//! | MP2RAGE inversion images | `…_inv-N_MP2RAGE` |
//! | scanner susceptibility map | `…_rec-scanner_Chimap` |
//! | scanner T2* / R2* map | `…_rec-scanner_T2starmap` / `…_R2starmap` |
//! | scanner T1 map | `…_T1map` |
//! | scanner SWI | `…_rec-swi_part-mag_T2starw` |
//! | scanner minimum-intensity projection | `…_rec-mip_T2starw` |
//! | anything else | `extra_data/<series number>_<description>…` as dcm2niix names it |
//!
//! Within one run, the plain ORIGINAL image of each kind is the main file. Any other
//! reconstruction of the same kind gets a `rec-` label: from the ImageType marker that sets it
//! apart (`FIL` → `rec-filtered`, `ASPIRE` → `rec-aspire`), else from its description
//! (`…_UNI-DEN` → `rec-uniden`), else from its series number. The magnitude follows the phase:
//! when the run's phase is a particular reconstruction (ASPIRE), the magnitude of that
//! reconstruction is the main one and the scanner's plain combination becomes `rec-scanner`. Uncombined coil images get
//! `rec-uncombined` when the scanner's combined images are there too, and a `coil-NN` entity
//! (a QSMxT extension of BIDS, listed in `.bidsignore`). `desc-` is never used: BIDS allows it only
//! in derivatives.

use std::collections::HashMap;

use super::classify::{recon_token_label, recon_tokens};
use super::group::is_plain_original;
use super::{clean_bids_label, same_te, CoilType, DicomAcquisition, DicomSeries, DicomSession, FlatSeriesRef, SeriesType};

/// What dcm2niix appended to an output's base name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dcm2niixSuffix {
    /// `_eN`: the echo number.
    pub echo_num: Option<u32>,
    /// `_e<TE>` with a fractional TE in ms (dcm2niix writes this when there are no echo numbers).
    pub echo_te: Option<f64>,
    /// `_ph`, `_real`, `_imaginary`.
    pub part: Option<&'static str>,
    /// `_cN`: a coil.
    pub coil: Option<u32>,
    /// Anything else (`_i00001`, `_t1234`, `a`/`b` for names dcm2niix had to repeat, …), in order:
    /// kept in the name so the outputs stay distinct.
    pub extra: Vec<String>,
}

fn known_token(tok: &str, out: &mut Dcm2niixSuffix) -> bool {
    match tok {
        "ph" => out.part = Some("phase"),
        "real" => out.part = Some("real"),
        "imaginary" => out.part = Some("imag"),
        _ => {
            if let Some(num) = tok.strip_prefix('e') {
                if let Ok(n) = num.parse::<u32>() {
                    out.echo_num = Some(n);
                    return true;
                }
                if num.contains('.') {
                    if let Ok(te) = num.parse::<f64>() {
                        out.echo_te = Some(te);
                        return true;
                    }
                }
                return false;
            }
            if let Some(num) = tok.strip_prefix('c') {
                if let Ok(n) = num.parse::<u32>() {
                    out.coil = Some(n);
                    return true;
                }
            }
            return false;
        }
    }
    true
}

/// Parse what dcm2niix appended to the base name (`-f`) of an output, e.g. `_e2_ph`, `_e3.3`,
/// `a`, `_e2a`, `_i00001`.
pub fn parse_dcm2niix_suffix(remainder: &str) -> Dcm2niixSuffix {
    let mut out = Dcm2niixSuffix::default();
    for (i, tok) in remainder.split('_').enumerate() {
        if tok.is_empty() {
            continue;
        }
        // Letters right after the base name: dcm2niix repeating a name.
        if i == 0 {
            out.extra.push(tok.to_string());
            continue;
        }
        if known_token(tok, &mut out) {
            continue;
        }
        // `e2a`, `pha`: a known token with dcm2niix's repeat letter after it.
        let (head, last) = tok.split_at(tok.len() - 1);
        if !head.is_empty() && last.chars().all(|c| c.is_ascii_lowercase()) && known_token(head, &mut out) {
            out.extra.push(last.to_string());
            continue;
        }
        out.extra.push(tok.to_string());
    }
    out
}

/// A BIDS name without what depends on dcm2niix's output (echo, coil, extension).
#[derive(Debug, Clone, PartialEq)]
pub struct BidsStem {
    pub sub: String,
    pub ses: Option<String>,
    pub acq: String,
    pub rec: Option<String>,
    pub run: Option<u32>,
    pub inv: Option<u32>,
    pub part: Option<&'static str>,
    pub suffix: &'static str,
    /// The run has several echoes: every file gets `echo-N`.
    pub multi_echo: bool,
    /// Whether BIDS allows `echo-` / `part-` with this suffix.
    pub allows_echo: bool,
    pub allows_part: bool,
    /// The run's echo times (ms), sorted: `echo-N` is the TE's position.
    pub run_tes: Vec<f64>,
    /// The series' own echo times (ms).
    pub series_tes: Vec<f64>,
    /// Per-coil images: each file gets `coil-NN`.
    pub uncombined: bool,
}

impl BidsStem {
    /// `sub-X[/ses-Y]/anat`.
    pub fn dir(&self) -> Vec<String> {
        let mut d = vec![format!("sub-{}", self.sub)];
        if let Some(ses) = &self.ses {
            d.push(format!("ses-{ses}"));
        }
        d.push("anat".to_string());
        d
    }

    fn echo_index(&self, sfx: &Dcm2niixSuffix) -> Option<u32> {
        let pos = |tes: &[f64], te: f64| tes.iter().position(|&t| same_te(t, te)).map(|i| i as u32 + 1);
        if let Some(te) = sfx.echo_te {
            return pos(&self.run_tes, te).or_else(|| pos(&self.series_tes, te));
        }
        if let Some(n) = sfx.echo_num {
            return Some(n);
        }
        // One echo per series (Siemens XA exports): its place among the run's echoes.
        if self.series_tes.len() == 1 {
            return pos(&self.run_tes, self.series_tes[0]);
        }
        None
    }

    /// The name of one of dcm2niix's outputs for this series.
    pub fn file_name(&self, sfx: &Dcm2niixSuffix, coil: Option<u32>, extension: &str) -> String {
        let mut rec = self.rec.clone().unwrap_or_default();
        let mut part = self.part;
        if let Some(p) = sfx.part {
            if self.allows_part {
                part = Some(p);
            } else {
                rec.push_str(p);
            }
        }
        let mut echo = self.echo_index(sfx);
        if sfx.echo_te.is_some() && echo.is_none() {
            // A TE that is none of the run's echoes: keep it distinct.
            rec.push_str(&format!("te{}", clean_bids_label(&format!("{}", sfx.echo_te.unwrap_or(0.0)))));
        }
        if !self.allows_echo {
            if let Some(e) = echo.take() {
                if self.series_tes.len() > 1 || sfx.echo_num.is_some() || sfx.echo_te.is_some() {
                    rec.push_str(&format!("echo{e}"));
                }
            }
        } else if self.multi_echo {
            echo = echo.or(Some(1));
        } else if echo == Some(1) {
            echo = None;
        }
        for t in &sfx.extra {
            rec.push_str(&clean_bids_label(t).to_lowercase());
        }
        let coil = if self.uncombined { coil.or(sfx.coil) } else { None };

        let mut name = format!("sub-{}", self.sub);
        if let Some(ses) = &self.ses {
            name.push_str(&format!("_ses-{ses}"));
        }
        name.push_str(&format!("_acq-{}", self.acq));
        if !rec.is_empty() {
            name.push_str(&format!("_rec-{rec}"));
        }
        if let Some(run) = self.run {
            name.push_str(&format!("_run-{run}"));
        }
        if let Some(c) = coil {
            name.push_str(&format!("_coil-{c:02}"));
        }
        if let Some(e) = echo {
            name.push_str(&format!("_echo-{e}"));
        }
        if let Some(inv) = self.inv {
            name.push_str(&format!("_inv-{inv}"));
        }
        if let Some(p) = part {
            name.push_str(&format!("_part-{p}"));
        }
        name.push_str(&format!("_{}{}", self.suffix, extension));
        name
    }

    /// The name with `echo-<N>` (and `coil-<NN>`) standing for every echo (and coil), for
    /// showing before conversion.
    pub fn preview(&self) -> String {
        let name = self.file_name(&Dcm2niixSuffix::default(), self.uncombined.then_some(0), "");
        let name = name.replacen("_coil-00", "_coil-<NN>", 1);
        if self.allows_echo && self.multi_echo && self.series_tes.len() > 1 {
            name.replacen("_echo-1_", "_echo-<N>_", 1)
        } else {
            name
        }
    }
}

/// Where a series goes.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Bids(BidsStem),
    /// `sub-X[/ses-Y]/extra_data/<prefix>…`, named by dcm2niix after `prefix`.
    Extra { dir: Vec<String>, prefix: String },
    Skip,
}

/// One series' place in the converted dataset.
#[derive(Debug, Clone)]
pub struct SeriesPlan {
    pub r: FlatSeriesRef,
    pub target: Target,
}

/// The `rec-` label a series' description gives: the description without the protocol name.
fn desc_label(s: &DicomSeries) -> String {
    let d = s.description.to_lowercase();
    let p = s.protocol_name.trim().to_lowercase();
    let p = p.trim_start_matches("wip").trim();
    let rest = if !p.is_empty() && d.contains(p) { d.replacen(p, "", 1) } else { d };
    let label = clean_bids_label(&rest.replace("_rr", ""));
    // A long description makes an unreadable label; the series number is clearer.
    if label.is_empty() || label.len() > 20 { format!("s{}", s.series_number) } else { label }
}

fn tes_overlap(a: &DicomSeries, b: &DicomSeries) -> bool {
    if a.echo_times.is_empty() || b.echo_times.is_empty() {
        return a.echo_times.is_empty() && b.echo_times.is_empty();
    }
    a.echo_times.iter().any(|&x| b.echo_times.iter().any(|&y| same_te(x, y)))
}

fn same_coils(a: &DicomSeries, b: &DicomSeries) -> bool {
    a.coil_type == b.coil_type
        && a.coil_groups.iter().map(|g| g.coil_num).collect::<Vec<_>>() == b.coil_groups.iter().map(|g| g.coil_num).collect::<Vec<_>>()
}

/// The MP2RAGE inversion index of each Inversion series of a run, from its inversion time.
fn inversion_indices(acq: &DicomAcquisition) -> HashMap<usize, u32> {
    let inv: Vec<(usize, f64)> = acq
        .series
        .iter()
        .enumerate()
        .filter(|(_, s)| s.series_type == SeriesType::Inversion)
        .map(|(i, s)| (i, s.inversion_times.first().copied().unwrap_or(f64::MAX)))
        .collect();
    let mut tis: Vec<f64> = inv.iter().map(|(_, t)| *t).collect();
    tis.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    tis.dedup_by(|a, b| same_te(*a, *b));
    let mut out = HashMap::new();
    for (i, ti) in inv {
        let mut idx = tis.iter().position(|&t| same_te(t, ti)).map(|p| p as u32 + 1).unwrap_or(1);
        if ti == f64::MAX {
            // No inversion time: the INV number in the description.
            let d = acq.series[i].description.to_lowercase();
            if let Some(pos) = d.rfind("inv") {
                if let Some(n) = d[pos + 3..].trim_start_matches(['-', '_']).chars().next().and_then(|c| c.to_digit(10)) {
                    idx = n;
                }
            }
        }
        out.insert(i, idx);
    }
    out
}

/// The `rec-` label (beyond what the type implies) of each BIDS series of a run.
fn reconstruction_labels(acq: &DicomAcquisition, inv: &HashMap<usize, u32>) -> HashMap<usize, Option<String>> {
    // Slots: series that would otherwise get the same name.
    // (type, per-coil, inversion index) → series
    type Slot = (SeriesType, bool, Option<u32>);
    let mut slots: Vec<(Slot, Vec<usize>)> = Vec::new();
    for (i, s) in acq.series.iter().enumerate() {
        if !s.series_type.is_bids() {
            continue;
        }
        let key = (s.series_type, s.coil_type == CoilType::Uncombined, inv.get(&i).copied());
        match slots.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => v.push(i),
            None => slots.push((key, vec![i])),
        }
    }
    // Phase first: a magnitude follows the phase's reconstruction.
    slots.sort_by_key(|(k, _)| k.0 != SeriesType::Phase);
    let mut phase_recon: HashMap<bool, Vec<String>> = HashMap::new();
    let mut labels: HashMap<usize, Option<String>> = HashMap::new();
    for ((kind, per_coil, _), mut members) in slots {
        members.sort_by_key(|&i| acq.series[i].series_number);
        let mut from_tokens: HashMap<usize, bool> = HashMap::new();
        for &i in &members {
            let s = &acq.series[i];
            let label = if is_plain_original(s) {
                None
            } else {
                let toks = recon_tokens(&s.image_type);
                from_tokens.insert(i, !toks.is_empty());
                Some(if toks.is_empty() { desc_label(s) } else { recon_token_label(&toks) })
            };
            labels.insert(i, label);
        }
        // Without a plain original image, the first reconstruction (by series number) is the main one.
        if !members.iter().any(|i| labels[i].is_none()) {
            if let Some(&first) = members.first() {
                labels.insert(first, None);
            }
        }
        // The main magnitude is the one from the same reconstruction as the main phase (ASPIRE's
        // magnitude next to ASPIRE's phase, rather than the scanner's own coil combination).
        if kind == SeriesType::Magnitude {
            if let Some(tokens) = phase_recon.get(&per_coil).filter(|t| !t.is_empty()) {
                let main = members.iter().copied().find(|i| labels[i].is_none());
                let same = members.iter().copied().find(|&i| recon_tokens(&acq.series[i].image_type) == *tokens);
                if let (Some(main), Some(same)) = (main, same) {
                    if main != same {
                        let toks = recon_tokens(&acq.series[main].image_type);
                        let demoted = if toks.is_empty() { "scanner".to_string() } else { recon_token_label(&toks) };
                        labels.insert(main, Some(demoted));
                        from_tokens.insert(main, true);
                        labels.insert(same, None);
                    }
                }
            }
        }
        if kind == SeriesType::Phase {
            if let Some(&main) = members.iter().find(|i| labels[i].is_none()) {
                phase_recon.insert(per_coil, recon_tokens(&acq.series[main].image_type));
            }
        }
        // Labels still shared by series holding the same echoes: tell them apart.
        for _ in 0..3 {
            let mut changed = false;
            for jj in 1..members.len() {
                for ii in 0..jj {
                    let (i, j) = (members[ii], members[jj]);
                    let (a, b) = (&acq.series[i], &acq.series[j]);
                    if labels[&i] != labels[&j] || !tes_overlap(a, b) || !same_coils(a, b) {
                        continue;
                    }
                    let current = labels[&j].clone();
                    let by_desc = desc_label(b);
                    let next = if from_tokens.get(&j).copied().unwrap_or(false) || current.is_none() {
                        from_tokens.insert(j, false);
                        match current {
                            Some(c) if c != by_desc => format!("{c}{by_desc}"),
                            _ => by_desc,
                        }
                    } else {
                        format!("{}s{}", current.unwrap_or_default(), b.series_number)
                    };
                    labels.insert(j, Some(next));
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }
    labels
}

/// The BIDS `run-` label of each acquisition in a session: every run of a repeated acquisition
/// is labelled (`run-1` included), and an acquisition acquired once gets none.
pub fn bids_runs(acquisitions: &[DicomAcquisition]) -> Vec<Option<u32>> {
    let mut runs_per_acq: HashMap<&str, usize> = HashMap::new();
    for acq in acquisitions {
        *runs_per_acq.entry(acq.name.as_str()).or_default() += 1;
    }
    acquisitions
        .iter()
        .map(|acq| (runs_per_acq[acq.name.as_str()] > 1).then_some(acq.run_number))
        .collect()
}

/// Where every series of a session goes.
pub fn plan_session(session: &DicomSession) -> Vec<SeriesPlan> {
    let mut plans = Vec::new();
    for (si, sub) in session.subjects.iter().enumerate() {
        for (sti, study) in sub.studies.iter().enumerate() {
            let ses = (!study.study_date.is_empty()).then(|| clean_bids_label(&study.study_date)).filter(|s| !s.is_empty());
            let mut base_dir = vec![format!("sub-{}", sub.patient_id)];
            if let Some(s) = &ses {
                base_dir.push(format!("ses-{s}"));
            }
            let mut extra_dir = base_dir.clone();
            extra_dir.push("extra_data".to_string());
            let mut extra_prefixes: Vec<String> = Vec::new();

            let runs = bids_runs(&study.acquisitions);
            for (ai, acq) in study.acquisitions.iter().enumerate() {
                let mut run_tes: Vec<f64> = Vec::new();
                for s in acq.series.iter().filter(|s| s.series_type.is_complex_part()) {
                    for &te in &s.echo_times {
                        super::push_te(&mut run_tes, te);
                    }
                }
                let multi_echo = run_tes.len() > 1;
                let inv = inversion_indices(acq);
                let labels = reconstruction_labels(acq, &inv);
                let mixed_coils = acq.series.iter().any(|s| s.series_type.is_complex_part() && s.coil_type == CoilType::Uncombined)
                    && acq.series.iter().any(|s| s.series_type.is_complex_part() && s.coil_type != CoilType::Uncombined);

                for (sei, s) in acq.series.iter().enumerate() {
                    let r = FlatSeriesRef { sub: si, study: sti, acq: ai, series: sei };
                    let target = match s.series_type {
                        SeriesType::Skip => Target::Skip,
                        SeriesType::Extra => {
                            let name = [s.description.as_str(), s.protocol_name.as_str()]
                                .iter()
                                .map(|n| clean_bids_label(n))
                                .find(|n| !n.is_empty())
                                .unwrap_or_else(|| "series".to_string());
                            let base = format!("{}_{}", s.series_number, name);
                            let mut prefix = base.clone();
                            let mut k = 2;
                            while extra_prefixes.contains(&prefix) {
                                prefix = format!("{base}_{k}");
                                k += 1;
                            }
                            extra_prefixes.push(prefix.clone());
                            Target::Extra { dir: extra_dir.clone(), prefix }
                        }
                        t => {
                            let (fixed_rec, part, suffix, allows_echo, allows_part, multi): (Option<&str>, Option<&'static str>, &'static str, bool, bool, bool) = match t {
                                SeriesType::Magnitude => (None, Some("mag"), if multi_echo { "MEGRE" } else { "T2starw" }, true, true, multi_echo),
                                SeriesType::Phase => (None, Some("phase"), if multi_echo { "MEGRE" } else { "T2starw" }, true, true, multi_echo),
                                SeriesType::Real => (None, Some("real"), if multi_echo { "MEGRE" } else { "T2starw" }, true, true, multi_echo),
                                SeriesType::Imaginary => (None, Some("imag"), if multi_echo { "MEGRE" } else { "T2starw" }, true, true, multi_echo),
                                SeriesType::Swi => (Some("swi"), Some("mag"), "T2starw", true, true, s.echo_times.len() > 1),
                                SeriesType::MinIp => (Some("mip"), None, "T2starw", true, true, s.echo_times.len() > 1),
                                SeriesType::T1w => (None, None, "T1w", true, true, s.echo_times.len() > 1),
                                SeriesType::Inversion => (None, None, "MP2RAGE", true, true, s.echo_times.len() > 1),
                                SeriesType::T1map => (None, None, "T1map", false, false, false),
                                SeriesType::ScannerQsm => (Some("scanner"), None, "Chimap", false, false, false),
                                SeriesType::T2starMap => (Some("scanner"), None, "T2starmap", false, false, false),
                                SeriesType::R2starMap => (Some("scanner"), None, "R2starmap", false, false, false),
                                SeriesType::Extra | SeriesType::Skip => unreachable!(),
                            };
                            let uncombined = s.coil_type == CoilType::Uncombined;
                            let mut rec = String::new();
                            if let Some(f) = fixed_rec {
                                rec.push_str(f);
                            }
                            if let Some(Some(l)) = labels.get(&sei) {
                                rec.push_str(l);
                            }
                            if uncombined && mixed_coils && t.is_complex_part() {
                                rec.push_str("uncombined");
                            }
                            Target::Bids(BidsStem {
                                sub: sub.patient_id.clone(),
                                ses: ses.clone(),
                                acq: acq.name.clone(),
                                rec: (!rec.is_empty()).then_some(rec),
                                run: runs[ai],
                                inv: inv.get(&sei).copied(),
                                part,
                                suffix,
                                multi_echo: multi,
                                allows_echo,
                                allows_part,
                                run_tes: if t.is_complex_part() { run_tes.clone() } else { s.echo_times.clone() },
                                series_tes: s.echo_times.clone(),
                                uncombined,
                            })
                        }
                    };
                    plans.push(SeriesPlan { r, target });
                }
            }
        }
    }
    plans
}

/// Names claimed twice: `(name, first owner, second owner)`. `entries` are `(name, owner)` pairs,
/// e.g. a BIDS path and the series it comes from.
pub fn find_collisions(entries: &[(String, String)]) -> Vec<(String, String, String)> {
    let mut seen: HashMap<&str, &str> = HashMap::new();
    let mut out = Vec::new();
    for (name, owner) in entries {
        match seen.get(name.as_str()) {
            Some(first) => out.push((name.clone(), first.to_string(), owner.clone())),
            None => {
                seen.insert(name, owner);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dcm2niix_suffixes() {
        let p = parse_dcm2niix_suffix;
        assert_eq!(p(""), Dcm2niixSuffix::default());
        assert_eq!(p("_e2").echo_num, Some(2));
        let s = p("_e12_ph");
        assert_eq!((s.echo_num, s.part), (Some(12), Some("phase")));
        assert_eq!(p("_e3_real").part, Some("real"));
        assert_eq!(p("_e3_imaginary").part, Some("imag"));
        assert_eq!(p("_e3.3").echo_te, Some(3.3));
        let s = p("_e2a");
        assert_eq!((s.echo_num, s.extra.clone()), (Some(2), vec!["a".to_string()]));
        assert_eq!(p("a").extra, vec!["a".to_string()]);
        assert_eq!(p("_c7").coil, Some(7));
        assert_eq!(p("_i00001").extra, vec!["i00001".to_string()]);
        assert_eq!(p("_t1234_e2").extra, vec!["t1234".to_string()]);
        assert_eq!(p("_ROI1").extra, vec!["ROI1".to_string()]);
    }

    fn stem(suffix: &'static str, part: Option<&'static str>, multi: bool, run_tes: &[f64], series_tes: &[f64]) -> BidsStem {
        BidsStem {
            sub: "1".into(), ses: None, acq: "gre".into(), rec: None, run: None, inv: None, part, suffix,
            multi_echo: multi, allows_echo: true, allows_part: true, run_tes: run_tes.to_vec(),
            series_tes: series_tes.to_vec(), uncombined: false,
        }
    }

    #[test]
    fn echo_entities() {
        let me = stem("MEGRE", Some("mag"), true, &[3.3, 6.6, 9.9], &[3.3, 6.6, 9.9]);
        let sfx = |r: &str| parse_dcm2niix_suffix(r);
        assert_eq!(me.file_name(&sfx("_e2"), None, ".nii.gz"), "sub-1_acq-gre_echo-2_part-mag_MEGRE.nii.gz");
        // Bruker: the TE in ms, no echo numbers.
        assert_eq!(me.file_name(&sfx("_e6.6"), None, ".json"), "sub-1_acq-gre_echo-2_part-mag_MEGRE.json");
        assert_eq!(me.file_name(&sfx("_e3.3_ph"), None, ".json"), "sub-1_acq-gre_echo-1_part-phase_MEGRE.json");
        // One echo per series: its place among the run's echoes.
        let per_echo = stem("MEGRE", Some("phase"), true, &[3.3, 6.6, 9.9], &[9.9]);
        assert_eq!(per_echo.file_name(&sfx(""), None, ".nii.gz"), "sub-1_acq-gre_echo-3_part-phase_MEGRE.nii.gz");
        // Single echo: T2starw, no echo entity.
        let se = stem("T2starw", Some("mag"), false, &[20.0], &[20.0]);
        assert_eq!(se.file_name(&sfx(""), None, ".nii.gz"), "sub-1_acq-gre_part-mag_T2starw.nii.gz");
        // An unknown dcm2niix suffix keeps a distinct name.
        assert_eq!(se.file_name(&sfx("a"), None, ".nii.gz"), "sub-1_acq-gre_rec-a_part-mag_T2starw.nii.gz");
        // Maps take no echo or part: they move into rec.
        let mut map = stem("Chimap", None, false, &[], &[4.0, 8.0]);
        map.allows_echo = false;
        map.allows_part = false;
        map.rec = Some("scanner".into());
        assert_eq!(map.file_name(&sfx("_e2"), None, ".nii.gz"), "sub-1_acq-gre_rec-scannerecho2_Chimap.nii.gz");
        assert_eq!(map.file_name(&sfx(""), None, ".nii.gz"), "sub-1_acq-gre_rec-scanner_Chimap.nii.gz");
    }

    #[test]
    fn previews() {
        let me = stem("MEGRE", Some("mag"), true, &[3.3, 6.6], &[3.3, 6.6]);
        assert_eq!(me.preview(), "sub-1_acq-gre_echo-<N>_part-mag_MEGRE");
        let per_echo = stem("MEGRE", Some("mag"), true, &[3.3, 6.6], &[6.6]);
        assert_eq!(per_echo.preview(), "sub-1_acq-gre_echo-2_part-mag_MEGRE");
    }

    #[test]
    fn collisions_name_both_owners() {
        let e = vec![("a".to_string(), "s1".to_string()), ("b".to_string(), "s2".to_string()), ("a".to_string(), "s3".to_string())];
        assert_eq!(find_collisions(&e), vec![("a".to_string(), "s1".to_string(), "s3".to_string())]);
    }

    #[test]
    fn repeated_acquisitions_label_every_run_and_single_ones_none() {
        let acq = |name: &str, run_number| DicomAcquisition { name: name.to_string(), run_number, series: Vec::new() };
        let acqs = [acq("gre", 1), acq("mprage", 1), acq("gre", 2), acq("gre", 3)];
        assert_eq!(bids_runs(&acqs), vec![Some(1), None, Some(2), Some(3)]);
    }
}
