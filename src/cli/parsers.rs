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
pub fn parse_ilsqr_precision(s: &str) -> Result<qsmxt_engine::IlsqrPrecision, String> {
    qsmxt_engine::IlsqrPrecision::ALL.into_iter().find(|p| p.as_str() == s.trim())
        .ok_or_else(|| format!("expected double or single, got '{s}'"))
}
