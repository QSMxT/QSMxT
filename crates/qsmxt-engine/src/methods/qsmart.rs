//! Methods prose for QSMART.

use super::*;

/// QSMART's settings that differ from their defaults, its inner inversion and vessel magnitude
/// aside (said apart). The vessel sizes carry their units.
pub(super) fn qsmart_changes(config: &PipelineConfig) -> Changed {
    let (q, d) = (&config.inversion.qsmart, QsmartConfig::default());
    let mut c = Changed::default();
    let mut skip = vec!["inversion", "vessel_magnitude", "vessel_size_units", "offset_grid", "frangi_c_mode", "frangi_c_factor"];
    skip.extend(QSMART_VESSEL_SIZE_KEYS);
    // A relative c replaces the absolute one, which then plays no part.
    let relative_c = q.frangi_c_mode == crate::enums::QsmartFrangiCMode::Relative;
    if relative_c {
        skip.push("frangi_c");
    }
    c.add_fields(q, &d, &skip);
    c.add(relative_c, || format!(
        "Frangi's c set to {} × the median of the vessel magnitude inside the brain mask", q.frangi_c_factor));
    c.add(q.offset_grid == crate::enums::QsmartOffsetGrid::QsmartReference,
        || "the stages' offset adjusted on the dipole grid of QSMART's adjust_offset.m".to_string());
    // In other units than the default, the sizes are said whatever they are: the default numbers
    // mean something else.
    let u = q.vessel_size_units;
    let other_units = u != d.vessel_size_units;
    c.add(other_units || q.vasc_sphere_radius != d.vasc_sphere_radius,
        || format!("a bottom-hat sphere radius of {} {u}", q.vasc_sphere_radius));
    c.add(other_units || differs(q.frangi_scale_min, d.frangi_scale_min) || differs(q.frangi_scale_max, d.frangi_scale_max)
            || differs(q.frangi_scale_ratio, d.frangi_scale_ratio),
        || format!("Frangi scales from {} to {} {u} in steps of {} {u}", q.frangi_scale_min, q.frangi_scale_max, q.frangi_scale_ratio));
    c
}
