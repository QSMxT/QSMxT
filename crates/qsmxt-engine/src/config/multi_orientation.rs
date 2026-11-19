use super::*;

/// Multi-orientation reconstruction (COSMOS / STI) over a group of runs.
///
/// Off unless `group_by` names a pattern. BIDS has no entity meaning "same object, different
/// orientation", so the dataset cannot declare the intention — the user does, by setting
/// this. The pattern's only job is to say *which runs* belong together; see
/// `crate::bids::orientation` in the qsmxt crate for its three tiers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MultiOrientationConfig {
    /// Orientation-group pattern: an entity name (`acq`, `run`, `rec`, `inv`), a glob over the
    /// run key (`*acq-dir*`), `re:` and a regex with one capture group, or `runs:` and the
    /// groups' run keys (`,` between runs, `;` between groups, each group's reference first).
    /// Empty = off.
    pub group_by: String,
    /// Which reconstruction to run over each group.
    pub algorithm: MultiOrientAlgorithm,
    /// Regularization: COSMOS's k-space L2 weight, or STI's per-k-point ridge term. 0 is plain
    /// least squares, which is right whenever the orientations cover k-space.
    pub lambda: f64,
    /// Reconstruct even when the direction set looks degenerate. The check exists because a
    /// degenerate set produces a plausible map that is not the method asked for, so this
    /// should stay off unless the metadata is known-good and the check is known-wrong.
    pub force: bool,
    /// Which orientation of each group is the reference: a glob over the run key
    /// (`*acq-Neutral*`), or a plain word matched anywhere in it. Empty = the first in
    /// discovery order. Must match exactly one member of every group.
    ///
    /// The reference is the common frame. The other orientations are registered onto it, and the
    /// group map is written on its grid and referenced and analysed on its segmentation. Pick the
    /// neutral head position: it is the smallest rotation from every other orientation, which
    /// keeps registration inside its capture range, and it gives the output a standard view.
    pub reference: String,
    /// Allow QSMxT to rigidly co-register the orientations onto the reference member's grid.
    ///
    /// On by default, and it costs nothing when there is nothing to do: registration runs only
    /// when the group actually needs it — the members disagree on dimensions or affine, or the
    /// only directions available came from affines that cannot tell the orientations apart.
    /// A group already on one grid with declared `B0_dir`s is left alone.
    ///
    /// Turn it off when the orientations were co-registered externally (FLIRT, ANTs) and you do
    /// not want a second interpolation on top. Then a group that still needs registration is
    /// refused rather than reconstructed, which is the behaviour QSMxT had before it could
    /// register at all.
    #[serde(default = "default_true")]
    pub register: bool,
    /// COSMOS only: weight each orientation's field by its combined magnitude and solve by
    /// conjugate gradient (`qsm_core::inversion::cosmos_weighted`), so low-signal voxels count
    /// for less. Off is the closed-form, unweighted least squares.
    pub magnitude_weighting: bool,
    /// Conjugate-gradient tolerance of magnitude-weighted COSMOS; unused without weighting.
    pub cosmos_tol: f64,
    /// Conjugate-gradient iteration cap of magnitude-weighted COSMOS; unused without weighting.
    pub cosmos_max_iter: usize,
}

fn default_true() -> bool {
    true
}

impl Default for MultiOrientationConfig {
    fn default() -> Self {
        Self {
            group_by: String::new(),
            reference: String::new(),
            algorithm: MultiOrientAlgorithm::Cosmos,
            lambda: 0.0,
            force: false,
            register: true,
            magnitude_weighting: false,
            cosmos_tol: qsm_core::inversion::CosmosParams::default().tol,
            cosmos_max_iter: qsm_core::inversion::CosmosParams::default().max_iter,
        }
    }
}

impl MultiOrientationConfig {
    /// Whether multi-orientation grouping is switched on.
    pub fn enabled(&self) -> bool {
        let g = self.group_by.trim();
        !g.is_empty() && !g.eq_ignore_ascii_case("none")
    }

    /// Why the COSMOS solver settings cannot be used as given, if they cannot: the CG settings
    /// only drive magnitude-weighted COSMOS, and STI has no magnitude weighting.
    pub fn validate(&self) -> Result<(), String> {
        let d = Self::default();
        if self.magnitude_weighting && self.algorithm == MultiOrientAlgorithm::Sti {
            return Err("magnitude weighting (--cosmos-magnitude-weighting) is a COSMOS option; \
                        STI has none".into());
        }
        if !self.magnitude_weighting
            && (self.cosmos_tol != d.cosmos_tol || self.cosmos_max_iter != d.cosmos_max_iter)
        {
            return Err("--cosmos-tol and --cosmos-max-iter set the conjugate gradient of \
                        magnitude-weighted COSMOS; without --cosmos-magnitude-weighting COSMOS \
                        is solved in closed form and they would be ignored".into());
        }
        if self.cosmos_tol <= 0.0 || !self.cosmos_tol.is_finite() || self.cosmos_max_iter == 0 {
            return Err(format!("COSMOS tolerance {} and iteration cap {} must be positive",
                               self.cosmos_tol, self.cosmos_max_iter));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum MultiOrientAlgorithm {
    /// Scalar susceptibility from 2+ orientations, closed form in k-space.
    #[default]
    Cosmos,
    /// Rank-2 susceptibility tensor from 6+ orientations.
    Sti,
}

impl std::fmt::Display for MultiOrientAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultiOrientAlgorithm::Cosmos => write!(f, "cosmos"),
            MultiOrientAlgorithm::Sti => write!(f, "sti"),
        }
    }
}
