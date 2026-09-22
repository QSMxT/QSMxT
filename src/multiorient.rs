//! Multi-orientation reconstruction: the shared parts of COSMOS and STI.
//!
//! Both algorithms take N background-removed field maps that already sit on **one common
//! grid**, plus one B0 direction per orientation expressed in that grid's voxel frame
//! ([`qsm_core::inversion::cosmos`], [`qsm_core::inversion::sti`]). Producing those two
//! things honestly is the whole job, and it is where multi-orientation QSM goes wrong.
//!
//! # Where a B0 direction can legitimately come from
//!
//! The object rotates; B0 does not. What each orientation contributes is therefore where B0
//! points *relative to the common grid*, and there are exactly three ways to know that:
//!
//! 1. **A sidecar `B0_dir`.** Authoritative, because it describes the acquisition better than
//!    any header can. This is what a curated multi-orientation dataset should ship.
//! 2. **The NIfTI affine**, via [`qsm_core::geometry::b0_direction_from_affine`] — but *only*
//!    when the orientations were separately prescribed, so the slab followed the head and the
//!    affines genuinely differ.
//! 3. **A rigid registration** between orientations. QSMxT cannot do this yet; see the module
//!    note on [`DirectionSource::Affine`] below.
//!
//! # The failure this module exists to prevent
//!
//! Two very common dataset shapes make the affine useless while leaving it perfectly
//! readable:
//!
//! - The head rotated inside an *identically prescribed* slab. Every affine is the same; the
//!   anatomy moved in voxel space instead.
//! - The orientations were already resampled into a reference frame before distribution
//!   (OpenNeuro `ds007958` is exactly this — three orientations, one shared 698x800x512 grid).
//!
//! In both cases `b0_direction_from_affine` returns the *same* direction for every
//! orientation. Handed to COSMOS, N identical directions collapse the closed form to a
//! single-orientation inversion with no regularization — which does not error, does not look
//! obviously wrong, and is not COSMOS. So [`check_directions`] refuses rather than guesses,
//! and every direction carries a [`DirectionSource`] so the user can see which of the three
//! routes above actually produced it.

use std::fmt;

/// How a B0 direction was obtained. Surfaced in the TUI and written into provenance, because
/// "the sidecar said so" and "we read it off the affine" carry very different confidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectionSource {
    /// A sidecar `B0_dir` field. Trustworthy for any dataset shape.
    Sidecar,
    /// Derived from the NIfTI affine. Only meaningful when the affines actually differ
    /// between orientations — [`check_directions`] enforces that.
    Affine,
    /// Given explicitly on the command line.
    Explicit,
}

impl fmt::Display for DirectionSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DirectionSource::Sidecar => write!(f, "sidecar"),
            DirectionSource::Affine => write!(f, "affine"),
            DirectionSource::Explicit => write!(f, "explicit"),
        }
    }
}

/// One orientation of a multi-orientation set.
#[derive(Debug, Clone)]
pub struct Orientation {
    /// Display label — a BIDS entity value (`acq-dir2`) or a file name.
    pub label: String,
    /// B0 direction in the common grid's voxel frame, normalised.
    pub b0: (f64, f64, f64),
    pub source: DirectionSource,
}

/// Which reconstruction a direction set is being checked for. They have different
/// requirements: COSMOS needs the orientations to *differ*, STI needs enough of them,
/// spread over enough of the sphere, to determine six tensor components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiOrientKind {
    Cosmos,
    Sti,
}

impl fmt::Display for MultiOrientKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MultiOrientKind::Cosmos => write!(f, "COSMOS"),
            MultiOrientKind::Sti => write!(f, "STI"),
        }
    }
}

/// Minimum pairwise angle (degrees) below which a direction set is treated as degenerate.
///
/// Rotating a head 17 degrees already moves the magic-angle cone clear of where it started
/// (see the COSMOS module docs in qsm-core), so a set whose *widest* separation is under a
/// few degrees is measuring one orientation N times, whatever the file names say. Three
/// degrees is comfortably below any deliberate repositioning and comfortably above the
/// rounding in a stored direction vector.
pub const MIN_SPREAD_DEG: f64 = 3.0;

/// Summary of a direction set, for display and for the go/no-go decision.
#[derive(Debug, Clone)]
pub struct OrientationCheck {
    pub n: usize,
    /// Widest angle between any two orientations, in degrees.
    pub max_pairwise_deg: f64,
    /// Narrowest angle between any two *distinct* orientations, in degrees.
    pub min_pairwise_deg: f64,
    /// Numerical rank of the direction set: 1 = all parallel, 2 = coplanar, 3 = spanning.
    pub rank: usize,
    /// `Err` means refuse to reconstruct; the string says why, in the user's terms.
    pub verdict: Result<(), String>,
}

impl OrientationCheck {
    pub fn is_ok(&self) -> bool {
        self.verdict.is_ok()
    }

    /// One-line summary for the TUI tree and CLI logs.
    pub fn summary(&self) -> String {
        match &self.verdict {
            Ok(()) => format!(
                "{} orientations, spread {:.0}°, rank {}",
                self.n, self.max_pairwise_deg, self.rank
            ),
            Err(why) => why.clone(),
        }
    }
}

fn normalise(d: (f64, f64, f64)) -> Option<(f64, f64, f64)> {
    let n = (d.0 * d.0 + d.1 * d.1 + d.2 * d.2).sqrt();
    if n < 1e-12 {
        None
    } else {
        Some((d.0 / n, d.1 / n, d.2 / n))
    }
}

/// Angle between two directions in degrees, folded to [0, 90].
///
/// B0 is an axis, not an arrow: an orientation and its antipode sample the dipole kernel
/// identically (the kernel depends on `(H.k)^2`), so a 180-degree "difference" is no
/// difference at all and must not be read as a wide spread.
fn axis_angle_deg(a: (f64, f64, f64), b: (f64, f64, f64)) -> f64 {
    let dot = (a.0 * b.0 + a.1 * b.1 + a.2 * b.2).abs().clamp(0.0, 1.0);
    dot.acos().to_degrees()
}

/// Numerical rank of the direction set, via Gram-Schmidt with a generous tolerance.
///
/// Used to catch the coplanar case, where six orientations obtained by rotating about a
/// single axis leave the six tensor components under-determined no matter how many there are.
fn direction_rank(dirs: &[(f64, f64, f64)]) -> usize {
    // A rotation of ~3 degrees out of a plane is real repositioning; less is noise in a
    // stored vector. sin(3 deg) ~ 0.05, so that is the residual norm we demand.
    const TOL: f64 = 0.05;
    let mut basis: Vec<(f64, f64, f64)> = Vec::new();
    for &d in dirs {
        let mut r = d;
        for b in &basis {
            let dot = r.0 * b.0 + r.1 * b.1 + r.2 * b.2;
            r = (r.0 - dot * b.0, r.1 - dot * b.1, r.2 - dot * b.2);
        }
        if let Some(u) = normalise(r) {
            if (r.0 * r.0 + r.1 * r.1 + r.2 * r.2).sqrt() > TOL {
                basis.push(u);
            }
        }
        if basis.len() == 3 {
            break;
        }
    }
    basis.len()
}

/// Decide whether a direction set can support the requested reconstruction.
///
/// This is the gate that stops a silently-degenerate run. It is deliberately strict: every
/// rejection here has a fix the user can actually apply (supply `B0_dir`, pass the directions
/// explicitly, or acquire more orientations), whereas a reconstruction from a degenerate set
/// produces a plausible-looking map that is quietly not what the method claims.
pub fn check_directions(orientations: &[Orientation], kind: MultiOrientKind) -> OrientationCheck {
    let dirs: Vec<(f64, f64, f64)> = orientations.iter().map(|o| o.b0).collect();
    let n = dirs.len();

    let (mut min_deg, mut max_deg) = (f64::INFINITY, 0.0f64);
    for i in 0..n {
        for j in (i + 1)..n {
            let a = axis_angle_deg(dirs[i], dirs[j]);
            min_deg = min_deg.min(a);
            max_deg = max_deg.max(a);
        }
    }
    if !min_deg.is_finite() {
        min_deg = 0.0;
    }
    let rank = direction_rank(&dirs);

    let min_n = match kind {
        MultiOrientKind::Cosmos => 2,
        MultiOrientKind::Sti => qsm_core::inversion::N_TENSOR,
    };

    let verdict = if n < min_n {
        Err(format!(
            "{kind} needs at least {min_n} orientations, found {n}"
        ))
    } else if max_deg < MIN_SPREAD_DEG {
        // The headline failure. Say which of the two dataset shapes it probably is, since the
        // fix differs, and name the source so the user knows where to look.
        let all_affine = orientations.iter().all(|o| o.source == DirectionSource::Affine);
        if all_affine {
            Err(format!(
                "all {n} B0 directions came from the NIfTI affines and are within {max_deg:.1}° \
                 of each other — the affines are effectively identical, so they cannot describe \
                 a rotation. Either the orientations share one prescribed slab, or they were \
                 already resampled into a common frame. Supply a `B0_dir` in each sidecar, or \
                 pass the directions explicitly"
            ))
        } else {
            Err(format!(
                "the {n} B0 directions are within {max_deg:.1}° of each other — this is one \
                 orientation measured {n} times, not a multi-orientation set. {kind} needs at \
                 least {MIN_SPREAD_DEG:.0}° of separation"
            ))
        }
    } else if kind == MultiOrientKind::Sti && rank < 3 {
        Err(format!(
            "the {n} B0 directions are coplanar (rank {rank}) — STI cannot determine all six \
             tensor components from rotations about a single axis. Acquire orientations that \
             tilt out of that plane"
        ))
    } else {
        Ok(())
    };

    OrientationCheck { n, max_pairwise_deg: max_deg, min_pairwise_deg: min_deg, rank, verdict }
}

/// Resolve B0 directions for a set of orientations, preferring a declared direction over an
/// inferred one.
///
/// `declared` is the sidecar (or explicit) direction where one exists; `affine` is each
/// volume's own affine, used only as the fallback.
pub fn resolve_directions(
    labels: &[String],
    declared: &[Option<(f64, f64, f64)>],
    affines: &[[f64; 16]],
    declared_source: DirectionSource,
) -> Vec<Orientation> {
    labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let (b0, source) = match declared.get(i).copied().flatten() {
                Some(d) => (d, declared_source),
                None => (
                    qsm_core::geometry::b0_direction_from_affine(&affines[i]),
                    DirectionSource::Affine,
                ),
            };
            let b0 = normalise(b0).unwrap_or((0.0, 0.0, 1.0));
            Orientation { label: label.clone(), b0, source }
        })
        .collect()
}

/// Render the per-orientation direction table that both the CLI log and the TUI show.
///
/// Printing where each direction came from is not decoration: it is the only thing that
/// distinguishes a real COSMOS run from three copies of one orientation.
pub fn direction_table(orientations: &[Orientation]) -> Vec<String> {
    orientations
        .iter()
        .map(|o| {
            format!(
                "{:<16} B0 [{:>6.3} {:>6.3} {:>6.3}]  {}",
                o.label, o.b0.0, o.b0.1, o.b0.2, o.source
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn orient(label: &str, b0: (f64, f64, f64), source: DirectionSource) -> Orientation {
        Orientation { label: label.into(), b0: normalise(b0).unwrap(), source }
    }

    /// Three well-spread orientations: the case everything else is measured against.
    fn good_three() -> Vec<Orientation> {
        vec![
            orient("dir1", (0.0, 0.0, 1.0), DirectionSource::Sidecar),
            orient("dir2", (0.0, 0.5, 0.87), DirectionSource::Sidecar),
            orient("dir3", (0.5, 0.0, 0.87), DirectionSource::Sidecar),
        ]
    }

    #[test]
    fn accepts_three_spread_orientations() {
        let check = check_directions(&good_three(), MultiOrientKind::Cosmos);
        assert!(check.is_ok(), "{:?}", check.verdict);
        assert_eq!(check.rank, 3);
        assert!(check.max_pairwise_deg > 25.0);
    }

    /// The ds007958 shape: already resampled to a common frame, so every affine agrees.
    #[test]
    fn rejects_identical_affine_directions() {
        let dirs: Vec<_> = (1..=3)
            .map(|i| orient(&format!("dir{i}"), (0.0, 0.0, 1.0), DirectionSource::Affine))
            .collect();
        let check = check_directions(&dirs, MultiOrientKind::Cosmos);
        let why = check.verdict.unwrap_err();
        assert!(why.contains("affines"), "{why}");
        assert!(why.contains("B0_dir"), "should name the fix: {why}");
    }

    /// Same degeneracy, but the directions were declared — different cause, different message.
    #[test]
    fn rejects_declared_directions_that_do_not_differ() {
        let dirs: Vec<_> = (1..=3)
            .map(|i| orient(&format!("dir{i}"), (0.0, 0.0, 1.0), DirectionSource::Sidecar))
            .collect();
        let why = check_directions(&dirs, MultiOrientKind::Cosmos).verdict.unwrap_err();
        assert!(why.contains("one orientation measured 3 times"), "{why}");
    }

    #[test]
    fn cosmos_needs_two_orientations() {
        let one = vec![orient("dir1", (0.0, 0.0, 1.0), DirectionSource::Sidecar)];
        assert!(check_directions(&one, MultiOrientKind::Cosmos).verdict.is_err());
    }

    #[test]
    fn sti_needs_six_orientations() {
        let check = check_directions(&good_three(), MultiOrientKind::Sti);
        let why = check.verdict.unwrap_err();
        assert!(why.contains("at least 6"), "{why}");
    }

    /// Six orientations obtained by rotating about one axis stay coplanar, and STI cannot
    /// separate six tensor components from them however many there are.
    #[test]
    fn sti_rejects_coplanar_directions() {
        let dirs: Vec<_> = (0..6)
            .map(|i| {
                let a = (i as f64) * 12.0_f64.to_radians();
                orient(&format!("dir{i}"), (0.0, a.sin(), a.cos()), DirectionSource::Sidecar)
            })
            .collect();
        let check = check_directions(&dirs, MultiOrientKind::Sti);
        assert_eq!(check.rank, 2, "single-axis rotation should be rank 2");
        assert!(check.verdict.unwrap_err().contains("coplanar"));
    }

    #[test]
    fn sti_accepts_six_spanning_directions() {
        let dirs: Vec<_> = [
            (0.0, 0.0, 1.0),
            (0.0, 0.5, 0.87),
            (0.5, 0.0, 0.87),
            (0.4, 0.4, 0.82),
            (-0.4, 0.3, 0.87),
            (0.3, -0.45, 0.84),
        ]
        .iter()
        .enumerate()
        .map(|(i, &d)| orient(&format!("dir{i}"), d, DirectionSource::Sidecar))
        .collect();
        let check = check_directions(&dirs, MultiOrientKind::Sti);
        assert!(check.is_ok(), "{:?}", check.verdict);
        assert_eq!(check.rank, 3);
    }

    /// B0 is an axis: a flipped direction samples the dipole kernel identically, so it must
    /// not be scored as a 180-degree spread.
    #[test]
    fn antipodal_directions_are_not_a_spread() {
        let dirs = vec![
            orient("a", (0.0, 0.0, 1.0), DirectionSource::Sidecar),
            orient("b", (0.0, 0.0, -1.0), DirectionSource::Sidecar),
        ];
        let check = check_directions(&dirs, MultiOrientKind::Cosmos);
        assert!(check.max_pairwise_deg < 1.0, "got {}", check.max_pairwise_deg);
        assert!(check.verdict.is_err(), "antipodal pair is degenerate, not well-spread");
    }

    #[test]
    fn declared_direction_beats_the_affine() {
        // An identity-ish affine would give (0,0,1); the sidecar says otherwise and wins.
        let affine = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let out = resolve_directions(
            &["a".into(), "b".into()],
            &[Some((0.0, 1.0, 0.0)), None],
            &[affine, affine],
            DirectionSource::Sidecar,
        );
        assert_eq!(out[0].source, DirectionSource::Sidecar);
        assert!((out[0].b0.1 - 1.0).abs() < 1e-9);
        assert_eq!(out[1].source, DirectionSource::Affine);
    }

    #[test]
    fn resolve_normalises_declared_directions() {
        let affine = [0.0; 16];
        let out = resolve_directions(
            &["a".into()],
            &[Some((0.0, 0.0, 7.0))],
            &[affine],
            DirectionSource::Explicit,
        );
        let n = (out[0].b0.0.powi(2) + out[0].b0.1.powi(2) + out[0].b0.2.powi(2)).sqrt();
        assert!((n - 1.0).abs() < 1e-9);
    }

    #[test]
    fn direction_table_shows_provenance() {
        let rows = direction_table(&good_three());
        assert_eq!(rows.len(), 3);
        assert!(rows[0].contains("sidecar"), "{}", rows[0]);
    }
}
