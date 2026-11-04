//! What a DICOM series is.
//!
//! The decision runs in this order, first match wins:
//! 1. Series that are clearly not for QSM (scouts, reformats and projections, diffusion, Dixon,
//!    summed images, mosaics) go to `extra_data/`.
//! 2. The scanner's QSM-related reconstructions: its susceptibility map, SWI, minimum-intensity
//!    projection, T2*/R2* maps and T1 map. From ImageType markers (`QSM`, `SWI`, `MNIP`,
//!    `T2_STAR MAP`, `T1 MAP`, …), or, for a series whose description is the scanner's name for
//!    a reconstruction rather than the protocol name, from that description (`…_QSM`,
//!    `T2Star_Images`, `R2star_…`).
//! 3. MP2RAGE: its UNI image is the T1-weighted image, INV1/INV2 are inversion images, anything
//!    else from the protocol goes to `extra_data/`.
//! 4. A T1-weighted structural by name (MPRAGE, SPACE/TFE/3D T1, T1w).
//! 5. A gradient-echo image's part: magnitude, phase, real or imaginary, from ImageType
//!    (`M`/`P`/`REAL`/`IMAGINARY`; Philips `R`/`I`), GE's private image type (0043,102F), the
//!    Enhanced `ComplexImageComponent`, then the description; an ORIGINAL image with no marker
//!    is a magnitude. Only for gradient-echo sequences: a spin-echo series (TSE, HASTE, FLAIR,
//!    SE-EPI) goes to `extra_data/`.
//! 6. Anything else goes to `extra_data/`.

use super::{normalize_series_description, FileMeta, SeriesType};

/// The part of a complex image a file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Component {
    Magnitude,
    Phase,
    Real,
    Imaginary,
}

impl Component {
    pub fn series_type(self) -> SeriesType {
        match self {
            Component::Magnitude => SeriesType::Magnitude,
            Component::Phase => SeriesType::Phase,
            Component::Real => SeriesType::Real,
            Component::Imaginary => SeriesType::Imaginary,
        }
    }
}

fn is_philips(manufacturer: &str) -> bool {
    manufacturer.trim().to_uppercase().starts_with("PHILIPS")
}

/// The image part a file's own markers name, if any: ImageType, then GE's private image type,
/// then ComplexImageComponent.
pub fn file_component(meta: &FileMeta) -> Option<Component> {
    let philips = is_philips(&meta.manufacturer);
    for t in &meta.image_type {
        match t.as_str() {
            "M" | "MAG" | "MAGNITUDE" => return Some(Component::Magnitude),
            "P" | "PHASE" => return Some(Component::Phase),
            "REAL" | "R_FFE" => return Some(Component::Real),
            "IMAGINARY" | "I_FFE" => return Some(Component::Imaginary),
            // Philips writes R and I for real and imaginary; Siemens XA uses R for other things.
            "R" if philips => return Some(Component::Real),
            "I" if philips => return Some(Component::Imaginary),
            _ => {}
        }
    }
    match meta.ge_image_type {
        Some(0) => return Some(Component::Magnitude),
        Some(1) => return Some(Component::Phase),
        Some(2) => return Some(Component::Real),
        Some(3) => return Some(Component::Imaginary),
        _ => {}
    }
    match meta.complex_component.trim().to_uppercase().as_str() {
        "MAGNITUDE" => Some(Component::Magnitude),
        "PHASE" => Some(Component::Phase),
        "REAL" => Some(Component::Real),
        "IMAGINARY" => Some(Component::Imaginary),
        _ => None,
    }
}

/// ImageType values that say nothing about which reconstruction a series is: the
/// ORIGINAL/DERIVED and PRIMARY/SECONDARY flags, the image-part markers, and vendor bookkeeping
/// (normalisation, distortion correction, Philips/GE sequence labels).
fn is_noise_token(t: &str) -> bool {
    matches!(
        t,
        "" | "ORIGINAL" | "DERIVED" | "PRIMARY" | "SECONDARY" | "M" | "P" | "R" | "I" | "MAG"
            | "MAGNITUDE" | "PHASE" | "REAL" | "IMAGINARY" | "M_FFE" | "P_FFE" | "R_FFE" | "I_FFE"
            | "PHASE MAP" | "FFE" | "SE" | "IR" | "ND" | "NORM" | "NONE" | "OTHER" | "MFSPLIT"
            | "DIS2D" | "DIS3D" | "FM" | "MOSAIC"
    ) || t.starts_with("FM") // Siemens filter-mode bookkeeping (FM3_2), paired with FIL
        || t.starts_with("DIS")
}

/// ImageType values that name the series' type rather than a variant of it.
fn is_type_token(t: &str) -> bool {
    matches!(
        t,
        "QSM" | "SWI" | "MNIP" | "MINIP" | "MIN IP" | "T1 MAP" | "T1_MAP" | "T1MAP" | "UNI"
            | "T2_STAR MAP" | "T2_STAR_MAP" | "T2STAR_MAP" | "T2_STAR" | "T2STAR" | "T2* MAP"
            | "R2_STAR MAP" | "R2_STAR_MAP" | "R2STAR_MAP" | "R2_STAR" | "R2STAR" | "R2* MAP"
    )
}

/// The ImageType values that tell one reconstruction of an acquisition from another (e.g. `FIL`
/// for Siemens' filtered magnitude, `ASPIRE` for ASPIRE's coil combination), sorted.
pub fn recon_tokens(image_type: &[String]) -> Vec<String> {
    let mut v: Vec<String> = image_type
        .iter()
        .map(|t| t.trim().to_uppercase())
        .filter(|t| !is_noise_token(t) && !is_type_token(t))
        .collect();
    v.sort();
    v.dedup();
    v
}

/// The `rec-` label recon tokens give: `FIL` → `filtered`, others lower-cased.
pub fn recon_token_label(tokens: &[String]) -> String {
    tokens
        .iter()
        .map(|t| match t.as_str() {
            "FIL" => "filtered".to_string(),
            other => other.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase(),
        })
        .collect::<Vec<_>>()
        .join("")
}

/// What classification looks at for one series.
#[derive(Debug, Clone, Default)]
pub struct SeriesFacts<'a> {
    pub image_type: &'a [String],
    /// The part the series' files' markers name (all files agree once a series is split).
    pub component: Option<Component>,
    pub description: &'a str,
    pub protocol: &'a str,
    pub scanning_sequence: &'a [String],
    pub inversion_time: Option<f64>,
}

fn has(tokens: &[String], wanted: &[&str]) -> bool {
    tokens.iter().any(|t| wanted.contains(&t.as_str()))
}

/// Whether a series' ScanningSequence allows a gradient-echo image: GR or EP without SE, or a
/// research-mode/unknown sequence (RM, empty: Bruker, GE research builds). Spin echo (TSE, HASTE,
/// SPACE), SE-EPI and inversion-recovery spin echo (FLAIR) do not.
fn gradient_echo(scanning_sequence: &[String]) -> bool {
    let has_tok = |t: &str| scanning_sequence.iter().any(|s| s == t);
    if has_tok("GR") {
        return true;
    }
    if has_tok("SE") {
        return false;
    }
    if has_tok("EP") {
        return true;
    }
    // IR alone is an inversion-recovery spin echo.
    !has_tok("IR")
}

/// Whether a (lower-case) name says T1-weighted structural.
fn t1w_name(name: &str) -> bool {
    if name.contains("map") {
        return false;
    }
    name.contains("mprage")
        || name.contains("mp-rage")
        || name.contains("t1_mpr")
        || name.contains("t1mpr")
        || name.contains("t1w")
        || name.contains("t1-w")
        || name.contains("3dt1")
        || (name.contains("t1")
            && ["space", "tfe", "3d", "bravo", "spgr", "fspgr", "mpr", "ir-fl", "irfl"].iter().any(|k| name.contains(k)))
}

/// The type a series' own metadata gives it.
pub fn classify_series(f: &SeriesFacts) -> SeriesType {
    let it: Vec<String> = f.image_type.iter().map(|t| t.trim().to_uppercase()).collect();
    let desc = normalize_series_description(f.description);
    let desc_l = desc.to_lowercase();
    let prot_l = f.protocol.trim().to_lowercase();
    let names = format!("{desc_l} {prot_l}");
    // The scanner names its reconstructions itself (`…_QSM`, `T2Star_Images`): the description
    // then differs from the protocol name. Description rules for reconstructions apply only then,
    // so a protocol called "R2star_mapping" is still its own acquisition.
    // Philips prefixes work-in-progress protocols with "WIP" in ProtocolName only.
    let without_wip = |s: &str| super::clean_bids_label(s.trim_start_matches("wip"));
    let scanner_named = !desc_l.is_empty() && without_wip(&desc_l) != without_wip(&prot_l);
    let original = it.first().map(|t| t == "ORIGINAL").unwrap_or(false);

    // 1. Not for QSM.
    if has(&it, &["LOCALIZER", "MPR", "PROJECTION IMAGE", "CSA MPR", "CSAPARALLEL", "MIP", "DIFFUSION",
                  "ADC", "TRACEW", "TRACE", "FA", "EXP", "DIXON", "WATER", "FAT", "FAT_FRACTION",
                  "WATER_FRACTION", "IN_PHASE", "OUT_PHASE", "OPP_PHASE", "SUM", "MOCO", "MOSAIC"])
        || ["localizer", "localiser", "scout", "survey", "dixon", "opp-in", "opp_in", "in-opp", "smwi"]
            .iter()
            .any(|k| names.contains(k))
        || desc_l.starts_with('<')
    {
        return SeriesType::Extra;
    }

    // 2. The scanner's QSM-related reconstructions.
    if has(&it, &["QSM"]) || (scanner_named && !desc_l.starts_with("r2star") && (desc_l.ends_with("_qsm") || desc_l == "qsm")) {
        return SeriesType::ScannerQsm;
    }
    if has(&it, &["SWI"]) || (scanner_named && desc_l.starts_with("swi_images")) {
        return SeriesType::Swi;
    }
    if has(&it, &["MNIP", "MINIP", "MIN IP"])
        || (scanner_named && (desc_l.starts_with("mip_images") || desc_l.contains("minip") || desc_l.contains("mnip")))
    {
        return SeriesType::MinIp;
    }
    if has(&it, &["T2_STAR MAP", "T2_STAR_MAP", "T2STAR_MAP", "T2_STAR", "T2STAR", "T2* MAP"])
        || (scanner_named
            && (desc_l.contains("t2star_images")
                || desc_l.contains("t2star_map")
                || desc_l.contains("t2starmap")
                || (desc_l.contains("t2*") && (desc_l.contains("map") || desc_l.contains("parametric")))))
    {
        return SeriesType::T2starMap;
    }
    if has(&it, &["R2_STAR MAP", "R2_STAR_MAP", "R2STAR_MAP", "R2_STAR", "R2STAR", "R2* MAP"])
        || (scanner_named && (desc_l.contains("r2star") || desc_l.contains("r2*")))
    {
        return SeriesType::R2starMap;
    }
    if has(&it, &["T1 MAP", "T1_MAP", "T1MAP"]) || (scanner_named && desc_l.ends_with("t1_images")) {
        return SeriesType::T1map;
    }

    // 3. MP2RAGE.
    if names.contains("mp2rage") {
        if has(&it, &["UNI"]) || desc_l.contains("uni") {
            return SeriesType::T1w;
        }
        let inv_named = ["inv1", "inv2", "inv-1", "inv-2", "inv_1", "inv_2"].iter().any(|k| desc_l.contains(k));
        if inv_named || (original && f.inversion_time.is_some_and(|ti| ti > 0.0)) {
            return SeriesType::Inversion;
        }
        return SeriesType::Extra;
    }

    // 4. T1-weighted structural.
    if t1w_name(&desc_l) || t1w_name(&prot_l) {
        return SeriesType::T1w;
    }

    // 5. A gradient-echo image's part.
    let component = f.component.or_else(|| {
        if desc_l.ends_with("_ph") || desc_l.contains("phase") || desc_l.starts_with("pha_") {
            Some(Component::Phase)
        } else if desc_l.contains("mag") {
            Some(Component::Magnitude)
        } else if original {
            // An original image with no part marker (Bruker, older exports) is a magnitude.
            Some(Component::Magnitude)
        } else {
            None
        }
    });
    match component {
        Some(c) if gradient_echo(f.scanning_sequence) => c.series_type(),
        _ => SeriesType::Extra,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts<'a>(it: &'a [String], desc: &'a str, prot: &'a str, ss: &'a [String]) -> SeriesFacts<'a> {
        SeriesFacts { image_type: it, description: desc, protocol: prot, scanning_sequence: ss, ..Default::default() }
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn classify(it: &[&str], desc: &str, prot: &str, ss: &[&str]) -> SeriesType {
        let it = s(it);
        let ss = s(ss);
        let meta = FileMeta { image_type: it.clone(), ..Default::default() };
        let mut f = facts(&it, desc, prot, &ss);
        f.component = file_component(&meta);
        classify_series(&f)
    }

    #[test]
    fn scanner_reconstructions_get_their_own_types() {
        let p = "t2star_SWIandQSM_tra_p3";
        assert_eq!(classify(&["ORIGINAL", "PRIMARY", "M", "ND", "NORM"], p, p, &["GR"]), SeriesType::Magnitude);
        assert_eq!(classify(&["ORIGINAL", "PRIMARY", "P", "ND"], p, p, &["GR"]), SeriesType::Phase);
        assert_eq!(classify(&["ORIGINAL", "PRIMARY", "QSM", "ND"], "t2star_SWIandQSM_tra_p3_QSM", p, &["GR"]), SeriesType::ScannerQsm);
        assert_eq!(classify(&["ORIGINAL", "PRIMARY", "MNIP", "ND", "NORM"], "mIP_Images(SW)", p, &["GR"]), SeriesType::MinIp);
        assert_eq!(classify(&["ORIGINAL", "PRIMARY", "M", "SWI", "ND", "NORM"], "SWI_Images", p, &["GR"]), SeriesType::Swi);
        // No ImageType at all (older Siemens exports): the scanner's name for it decides.
        assert_eq!(classify(&[], "fl3d_vibe_QSM_QSM", "fl3d_vibe_QSM", &["GR"]), SeriesType::ScannerQsm);
        assert_eq!(classify(&["ORIGINAL", "PRIMARY", "T2_STAR MAP", "ND"], "T2Star_Images", "gre_MapT2star_4TEs", &["GR"]), SeriesType::T2starMap);
        assert_eq!(classify(&["ORIGINAL", "PRIMARY", "M", "DIS2D"], "R2star_ASPIRE_bipolar", "ASPIRE_bipolar", &["GR"]), SeriesType::R2starMap);
        assert_eq!(classify(&["ORIGINAL", "PRIMARY", "M", "DIS2D"], "R2star_gre_qsm", "gre_qsm", &["GR"]), SeriesType::R2starMap);
        // A protocol named after a map is still the acquisition.
        assert_eq!(classify(&["ORIGINAL", "PRIMARY", "M", "ND"], "gre_MapT2star_4TEs", "gre_MapT2star_4TEs", &["GR"]), SeriesType::Magnitude);
        // Bruker: no part marker, research-mode sequence; the parametric map by its description.
        assert_eq!(classify(&["ORIGINAL", "PRIMARY", "OTHER"], "", "T2star_map_MGE", &["RM"]), SeriesType::Magnitude);
        assert_eq!(classify(&["DERIVED", "SECONDARY", "OTHER"], "Parametric image for T2* relaxation", "", &["RM"]), SeriesType::T2starMap);
    }

    #[test]
    fn non_qsm_series_go_to_extra() {
        assert_eq!(classify(&["ORIGINAL", "PRIMARY", "M", "ND", "NORM"], "AAHead_Scout_64ch-head-coil", "AAHead_Scout_64ch-head-coil", &["GR"]), SeriesType::Extra);
        assert_eq!(classify(&["DERIVED", "PRIMARY", "MPR", "ND", "NORM"], "AAHead_Scout_MPR_sag", "AAHead_Scout", &["GR"]), SeriesType::Extra);
        assert_eq!(classify(&["DERIVED", "SECONDARY", "MPR", "CSA MPR THICK", "", "M", "ND"], "<MPR Thick Range>", "t1_space_fs_sag_iso", &["SE"]), SeriesType::Extra);
        assert_eq!(classify(&["ORIGINAL", "PRIMARY", "M", "ND", "NORM"], "t2_tirm_tra_dark-fluid", "t2_tirm_tra_dark-fluid", &["SE", "IR"]), SeriesType::Extra);
        assert_eq!(classify(&["ORIGINAL", "PRIMARY", "M", "ND"], "t2_tse_tra", "t2_tse_tra", &["SE"]), SeriesType::Extra);
        assert_eq!(classify(&["DERIVED", "PRIMARY", "DIFFUSION", "ADC", "ND"], "resolve_ADC", "resolve", &["RM"]), SeriesType::Extra);
        assert_eq!(classify(&["ORIGINAL", "PRIMARY", "DIXON", "WATER"], "vibe_q-dixon_W", "vibe_q-dixon", &["GR"]), SeriesType::Extra);
        assert_eq!(classify(&["ORIGINAL", "PRIMARY", "DIXON", "ND", "T2_STAR_MAP"], "vibe_q-dixon_T2s", "vibe_q-dixon", &["GR"]), SeriesType::Extra);
        assert_eq!(classify(&["ORIGINAL", "PRIMARY", "SUM_FFE", "SUM", "FFE"], "DelRec - QSM", "WIP DelRec - QSM", &["GR"]), SeriesType::Extra);
        assert_eq!(classify(&["ORIGINAL", "PRIMARY", "PROJECTION IMAGE", "M", "FFE"], "T1W COR", "T1W COR", &["GR"]), SeriesType::Extra);
        // Derived without any part marker.
        assert_eq!(classify(&[], "Aspire_Qmetric_ASPIRE_bipolar", "ASPIRE_bipolar", &["GR"]), SeriesType::Extra);
    }

    #[test]
    fn t1w_structurals() {
        for name in ["t1_mprage_sag_p2", "t1_mpr_ns_sag_p2_1mm_iso", "t1_space_fs_sag_iso_C+", "3DSAG_T1_", "3D T1W_1mmiso_CS2.5_TFE157", "T1_MPRAGE_TRA_ISO_1MM"] {
            assert_eq!(classify(&["ORIGINAL", "PRIMARY", "M", "ND", "NORM"], name, name, &["GR", "IR"]), SeriesType::T1w, "{name}");
        }
        // SPACE is a spin-echo sequence, still the T1w structural.
        assert_eq!(classify(&["ORIGINAL", "PRIMARY", "M", "ND"], "t1_space_fs_sag", "t1_space_fs_sag", &["SE"]), SeriesType::T1w);
        // A 2D T1 opposed/in-phase GRE is not the structural.
        assert_eq!(classify(&["ORIGINAL", "PRIMARY", "M", "NORM"], "t1_fl2d_opp-in_tra", "t1_fl2d_opp-in_tra", &["GR"]), SeriesType::Extra);
    }

    #[test]
    fn mp2rage_series() {
        let p = "MP2RAGE_wip925b_0p75mm";
        let c = |it: &[&str], d: &str, ti: Option<f64>| {
            let it = s(it);
            let ss = s(&["GR", "IR"]);
            let meta = FileMeta { image_type: it.clone(), ..Default::default() };
            classify_series(&SeriesFacts { image_type: &it, component: file_component(&meta), description: d, protocol: p,
                scanning_sequence: &ss, inversion_time: ti })
        };
        assert_eq!(c(&["ORIGINAL", "PRIMARY", "M", "DIS3D"], "MP2RAGE_wip925b_0p75mm_RR_INV1", Some(840.0)), SeriesType::Inversion);
        assert_eq!(c(&["ORIGINAL", "PRIMARY", "M", "DIS3D"], "MP2RAGE_wip925b_0p75mm_RR_INV2", Some(2370.0)), SeriesType::Inversion);
        assert_eq!(c(&["DERIVED", "PRIMARY", "M", "UNI", "DIS3D"], "MP2RAGE_wip925b_0p75mm_RR_UNI_Images", Some(0.0)), SeriesType::T1w);
        assert_eq!(c(&["DERIVED", "PRIMARY", "M", "UNI", "DIS3D"], "MP2RAGE_wip925b_0p75mm_RR_UNI-DEN", Some(0.0)), SeriesType::T1w);
        assert_eq!(c(&["DERIVED", "PRIMARY", "T1 MAP", "DIS3D"], "MP2RAGE_wip925b_0p75mm_RR_T1_Images", Some(0.0)), SeriesType::T1map);
        assert_eq!(c(&["DERIVED", "PRIMARY", "M", "DIS3D"], "MP2RAGE_wip925b_0p75mm_RR_T1_Images_SIM-TI20ms", Some(20.0)), SeriesType::Extra);
    }

    #[test]
    fn image_part_markers() {
        let meta = |it: &[&str], ge: Option<i32>, cic: &str, mfr: &str| FileMeta {
            image_type: s(it), ge_image_type: ge, complex_component: cic.into(), manufacturer: mfr.into(), ..Default::default()
        };
        assert_eq!(file_component(&meta(&["ORIGINAL", "PRIMARY", "OTHER"], Some(0), "", "GE MEDICAL SYSTEMS")), Some(Component::Magnitude));
        assert_eq!(file_component(&meta(&["ORIGINAL", "PRIMARY", "OTHER"], Some(1), "", "GE MEDICAL SYSTEMS")), Some(Component::Phase));
        assert_eq!(file_component(&meta(&["ORIGINAL", "PRIMARY", "OTHER"], Some(2), "", "GE MEDICAL SYSTEMS")), Some(Component::Real));
        assert_eq!(file_component(&meta(&["ORIGINAL", "PRIMARY", "OTHER"], Some(3), "", "GE MEDICAL SYSTEMS")), Some(Component::Imaginary));
        assert_eq!(file_component(&meta(&["ORIGINAL", "PRIMARY", "PHASE MAP", "P", "FFE"], None, "", "Philips")), Some(Component::Phase));
        assert_eq!(file_component(&meta(&["ORIGINAL", "PRIMARY", "R_FFE", "R", "FFE"], None, "", "Philips")), Some(Component::Real));
        // Siemens XA's R is not "real".
        assert_eq!(file_component(&meta(&["ORIGINAL", "PRIMARY", "R", "NONE"], None, "MAGNITUDE", "Siemens")), Some(Component::Magnitude));
        assert_eq!(file_component(&meta(&["ORIGINAL", "PRIMARY", "MIXED"], None, "PHASE", "Siemens")), Some(Component::Phase));
        assert_eq!(file_component(&meta(&["ORIGINAL", "PRIMARY", "OTHER"], None, "", "Bruker")), None);
    }

    #[test]
    fn recon_tokens_keep_only_what_tells_reconstructions_apart() {
        assert!(recon_tokens(&s(&["ORIGINAL", "PRIMARY", "M", "ND", "NORM"])).is_empty());
        assert!(recon_tokens(&s(&["ORIGINAL", "PRIMARY", "P", "ND"])).is_empty());
        assert_eq!(recon_tokens(&s(&["ORIGINAL", "PRIMARY", "M", "ND", "NORM", "FM", "FIL"])), s(&["FIL"]));
        assert_eq!(recon_tokens(&s(&["ORIGINAL", "PRIMARY", "M", "NORM", "DIS2D", "FM3_2", "FIL", "MFSPLIT"])), s(&["FIL"]));
        assert_eq!(recon_tokens(&s(&["ORIGINAL", "PRIMARY", "M", "ASPIRE", "DIS2D"])), s(&["ASPIRE"]));
        assert!(recon_tokens(&s(&["ORIGINAL", "PRIMARY", "M", "SWI", "ND"])).is_empty());
        assert_eq!(recon_token_label(&s(&["FIL"])), "filtered");
        assert_eq!(recon_token_label(&s(&["ASPIRE"])), "aspire");
    }
}
