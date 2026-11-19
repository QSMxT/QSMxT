/// An echo-time choice: `first`, `last`, `spacing`, `sum`, or an echo time in milliseconds.
pub fn parse_echo_time(s: &str) -> Result<qsmxt_engine::EchoTimeChoice, String> {
    qsmxt_engine::EchoTimeChoice::parse(s)
}

/// A setting a method can derive from its other settings: a number, or `auto` for the value it
/// derives (TV's and NLTV's rho, FANSI's, L1-QSM's and HD-QSM's mu).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrAuto {
    Auto,
    Value(f64),
}
impl OrAuto {
    /// The setting: `None` for auto.
    pub fn value(self) -> Option<f64> {
        match self { OrAuto::Auto => None, OrAuto::Value(v) => Some(v) }
    }
}

/// `auto`, or a number.
pub fn parse_or_auto(s: &str) -> Result<OrAuto, String> {
    match s.trim() {
        "auto" => Ok(OrAuto::Auto),
        v => v.parse::<f64>().map(OrAuto::Value).map_err(|_| format!("expected a number or auto, got '{v}'")),
    }
}

/// DECOMPOSE's χ bound: `echo-spacing`, or a bound in ppm.
pub fn parse_chi_bound(s: &str) -> Result<qsmxt_engine::ChiBoundChoice, String> {
    qsmxt_engine::ChiBoundChoice::parse(s)
}

/// iLSQR's arithmetic: `double` or `single`.
/// The R2* χ-sep MEDI's CSF mask is extracted from: `arlo` or `nnls`.
pub fn parse_chi_sep_csf_r2star(s: &str) -> Result<qsmxt_engine::ChiSepCsfR2star, String> {
    qsmxt_engine::ChiSepCsfR2star::ALL.into_iter().find(|m| m.as_str() == s.trim())
        .ok_or_else(|| format!("expected arlo or nnls, got '{s}'"))
}

pub fn parse_ilsqr_precision(s: &str) -> Result<qsmxt_engine::IlsqrPrecision, String> {
    qsmxt_engine::IlsqrPrecision::ALL.into_iter().find(|p| p.as_str() == s.trim())
        .ok_or_else(|| format!("expected double or single, got '{s}'"))
}

/// LBV's arithmetic: `double` or `single`.
pub fn parse_lbv_precision(s: &str) -> Result<qsmxt_engine::LbvPrecision, String> {
    qsmxt_engine::LbvPrecision::ALL.into_iter().find(|p| p.as_str() == s.trim())
        .ok_or_else(|| format!("expected double or single, got '{s}'"))
}

/// A WH-QSM data weight: `flat`, `flat-in-mask`, `max-normalised` or `in-vivo`.
pub fn parse_whqsm_weight(s: &str) -> Result<qsmxt_engine::WhqsmWeight, String> {
    qsmxt_engine::WhqsmWeight::ALL.into_iter().find(|w| w.as_str() == s.trim())
        .ok_or_else(|| format!("expected flat, flat-in-mask, max-normalised or in-vivo, got '{s}'"))
}

/// BET's left-right orientation: `fsl` or `as-stored`.
pub fn parse_bet_orientation(s: &str) -> Result<qsmxt_engine::BetOrientation, String> {
    qsmxt_engine::BetOrientation::ALL.into_iter().find(|o| o.as_str() == s.trim())
        .ok_or_else(|| format!("expected fsl or as-stored, got '{s}'"))
}

/// The image geometry N4 is given: `scanner` or `make-nii`.
pub fn parse_n4_geometry(s: &str) -> Result<qsmxt_engine::N4Geometry, String> {
    qsmxt_engine::N4Geometry::ALL.into_iter().find(|g| g.as_str() == s.trim())
        .ok_or_else(|| format!("expected scanner or make-nii, got '{s}'"))
}

/// HARPERELLA's / iHARPERELLA's boundary rim: `whole` or `sti`.
pub fn parse_harperella_rim(s: &str) -> Result<qsmxt_engine::HarperellaRim, String> {
    qsmxt_engine::HarperellaRim::ALL.into_iter().find(|r| r.as_str() == s.trim())
        .ok_or_else(|| format!("expected whole or sti, got '{s}'"))
}

/// NDI's magnitude-weight normaliser: `global-max` or `mask-p999`.
pub fn parse_ndi_weight_norm(s: &str) -> Result<qsmxt_engine::NdiWeightNorm, String> {
    qsmxt_engine::NdiWeightNorm::ALL.into_iter().find(|n| n.as_str() == s.trim())
        .ok_or_else(|| format!("expected global-max or mask-p999, got '{s}'"))
}

/// An iSMV implementation: `wen2014` or `qsmjl`.
pub fn parse_ismv_variant(s: &str) -> Result<qsmxt_engine::IsmvVariant, String> {
    qsmxt_engine::IsmvVariant::ALL.into_iter().find(|v| v.as_str() == s.trim())
        .ok_or_else(|| format!("expected wen2014 or qsmjl, got '{s}'"))
}

/// A PDF dipole kernel: `image` or `kspace`.
pub fn parse_pdf_space(s: &str) -> Result<qsmxt_engine::PdfSpace, String> {
    qsmxt_engine::PdfSpace::ALL.into_iter().find(|k| k.as_str() == s.trim())
        .ok_or_else(|| format!("expected image or kspace, got '{s}'"))
}
