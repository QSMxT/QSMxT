//! The Input section's settings.

use super::*;

pub(super) fn defs() -> Vec<Def> {
    vec![
        d("multi_orientation.group_by", Input, "Orientations", "Runs that are the same object at a different angle: an entity (acq, run, rec, inv), a glob (*acq-dir*), re:<regex>, or runs:<groups>. Empty = off. How they combine: QSM → COSMOS/STI", Text).basic().off(json!("")).flag("--orientation-group").full("Orientation grouping")
        .gui("Runs that are the same object at a different angle, matched by an entity (acq, run, rec, inv), a glob (*acq-dir*), re:<regex> or runs:<groups>; empty turns grouping off. How the orientations are combined is set under COSMOS/STI").unavailable_in_browser(),
    ]
}
