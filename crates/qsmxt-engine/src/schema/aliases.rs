//! Names a front end offers for a combination of settings, beside a setting's own values: a
//! picker entry that chooses one value and turns another setting on with it.

/// A picker entry that stands for a value of `setting` with `flag` turned on. The configuration
/// never holds it: choosing it sets `setting` to `value` and `flag` on; choosing `value` itself
/// turns `flag` off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PickerAlias {
    /// The setting the picker chooses, by its path in the configuration.
    pub setting: &'static str,
    /// The entry's name, listed right after `value`.
    pub name: &'static str,
    pub value: &'static str,
    /// The switch the entry turns on, by its path.
    pub flag: &'static str,
    pub help: &'static str,
}

/// LSQR with HEIDI: HEIDI as published.
pub const LSQR_HEIDI: PickerAlias = PickerAlias {
    setting: "inversion.algorithm",
    name: "lsqr+heidi",
    value: "lsqr",
    flag: "inversion.heidi.enabled",
    help: "LSQR+HEIDI (HEIDI as published) — LSQR with the HEIDI box ticked: LSQR, then the dipole cone \
           refilled under a homogeneity prior. https://doi.org/10.1016/j.neuroimage.2012.05.067",
};

/// Every picker alias.
pub const PICKER_ALIASES: &[PickerAlias] = &[LSQR_HEIDI];
