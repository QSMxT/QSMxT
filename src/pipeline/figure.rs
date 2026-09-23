//! One figure per map: the per-structure statistics as a PNG bar chart.
//!
//! A separate file per map rather than one stacked figure, so each can be dropped into a slide or
//! a paper on its own — and so each is ranked by its own values instead of inheriting an order
//! from whichever map came first.
//!
//! Values are not printed on every bar. The axis carries them, the extremes are labelled, and the
//! TSV beside the figure is the table view — a number on every row of thirty is chaos and goes
//! unread.

use std::path::{Path, PathBuf};

use crate::pipeline::canvas::{Anchor, Canvas, Fonts, Rgb, Weight};
use crate::pipeline::stats::Row;

// ── Geometry, in layout units ──
const WIDTH: f64 = 760.0;
const LABEL_W: f64 = 196.0;
const RIGHT_W: f64 = 74.0;
const ROW_H: f64 = 17.0;
const BAR_H: f64 = 10.0;
/// Room for the title, the map name, the subtitle and the legend — all of which sit above the
/// first row, so this has to clear the lowest of them plus its descender.
const HEAD_H: f64 = 64.0;
/// Extra header room when a susceptibility figure also prints what it was referenced to.
const REF_LINE_H: f64 = 14.0;
const AXIS_H: f64 = 24.0;
const PAD: f64 = 14.0;

// ── Palette ──
// Validated for CVD separation and contrast against the light surface (OKLab ΔE 21.6 protan,
// 32.3 normal, both ≥ 3:1). See the data-visualisation reference.
const SURFACE: Rgb = Rgb::hex("#fcfcfb");
const INK: Rgb = Rgb::hex("#0b0b0b");
const INK2: Rgb = Rgb::hex("#52514e");
const MUTED: Rgb = Rgb::hex("#898781");
const GRID: Rgb = Rgb::hex("#e1e0d9");
const AXIS: Rgb = Rgb::hex("#c3c2b7");
/// The diverging pair, used only where sign is a real distinction.
const POS: Rgb = Rgb::hex("#e34948");
const NEG: Rgb = Rgb::hex("#2a78d6");
/// The single hue for a map that never changes sign, where colour would otherwise encode nothing.
const NEUTRAL: Rgb = Rgb::hex("#2a78d6");

/// How a panel is drawn.
///
/// Volume spans three orders of magnitude — cerebral cortex is four hundred times the accumbens —
/// so on a linear axis from zero every subcortical structure is an invisible sliver. A log axis
/// fixes that, but a *bar* on a log axis is a lie: its length is measured from whatever floor the
/// axis happens to start at, not from zero, so length no longer encodes magnitude. Dots encode
/// position only, which is exactly what a log axis supports — and with no bar lengths to compare,
/// every value is labelled instead of just the extremes.
#[derive(Clone, Copy, PartialEq)]
struct Style {
    log: bool,
    label_every: bool,
}

impl Style {
    const BARS: Self = Self { log: false, label_every: false };
    const LOG_DOTS: Self = Self { log: true, label_every: true };
}

/// A map's figure, ready to write.
pub struct Figure {
    /// Filename stem, e.g. `desc-chimap_stats`.
    pub stem: String,
    canvas: Canvas,
}

impl Figure {
    pub fn save_in(&self, dir: &Path, prefix: &str) -> crate::Result<PathBuf> {
        let name = if prefix.is_empty() {
            format!("{}.png", self.stem)
        } else {
            format!("{prefix}_{}.png", self.stem)
        };
        let path = dir.join(name);
        self.canvas.save(&path)?;
        Ok(path)
    }
}

/// `Chimap` -> `desc-chimap_stats`; `desc-paramagnetic_Chimap` -> `desc-paramagnetic-chimap_stats`.
///
/// One `desc-` per name, so the chi-separation maps do not end up with two.
fn stem_for(map: &str) -> String {
    let slug = map.replace("desc-", "").replace('_', "-").to_lowercase();
    format!("desc-{slug}_stats")
}

/// Render one figure per map in `rows`.
///
/// `subtitle` says what the whiskers mean, which differs between a single run (the spread of
/// voxels inside the structure) and the group view (the spread across runs). `volume_subtitle`
/// does the same for the volume figure, whose whiskers mean something different again.
pub fn render_all(
    rows: &[Row], title: &str, subtitle: &str, volume_subtitle: &str,
) -> crate::Result<Vec<Figure>> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let fonts = Fonts::load()?;
    let mut maps: Vec<&str> = Vec::new();
    for r in rows {
        if !maps.contains(&r.map.as_str()) {
            maps.push(&r.map);
        }
    }
    let mut figs: Vec<Figure> = maps.into_iter()
        .filter_map(|map| {
            let panel: Vec<&Row> = rows.iter().filter(|r| r.map == map).collect();
            (!panel.is_empty()).then(|| Figure {
                stem: stem_for(map),
                canvas: draw(&fonts, &panel, map, title, subtitle, Style::BARS),
            })
        })
        .collect();

    // Volume is a property of the parcellation, not of any map, so it is carried as a column and
    // repeats across a run's rows. Drawn from the first row that has it for each structure, rather
    // than as a pseudo-map, which would have duplicated every structure once per map in the table.
    let volumes = volume_rows(rows);
    if !volumes.is_empty() {
        let panel: Vec<&Row> = volumes.iter().collect();
        figs.push(Figure {
            stem: "desc-volume_stats".to_string(),
            canvas: draw(&fonts, &panel, "volume", title, volume_subtitle, Style::LOG_DOTS),
        });
    }
    Ok(figs)
}

/// One row per structure carrying its volume, for the volume figure.
///
/// A structure appears once per map in the table with the same volume each time, so the first
/// occurrence is taken and the rest ignored. A supplied segmentation has no posteriors and so no
/// volumes, and then there is no figure to draw.
fn volume_rows(rows: &[Row]) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    for r in rows {
        let Some(v) = r.volume_mm3 else { continue };
        if out.iter().any(|o| o.name == r.name) {
            continue;
        }
        out.push(Row {
            map: "volume".into(), unit: "mm3".into(), name: r.name.clone(),
            // The volume's own spread, not the map's — a cohort's thalamus varies in size by a
            // quite different amount from how its susceptibility varies.
            mean: v, sd: r.volume_sd_mm3.unwrap_or(0.0),
            volume_mm3: Some(v), volume_sd_mm3: r.volume_sd_mm3,
            reference: None, reference_offset_ppm: None,
        });
    }
    out
}

fn draw(
    fonts: &Fonts, panel: &[&Row], map: &str, title: &str, subtitle: &str, style: Style,
) -> Canvas {
    // A χ figure without its reference is a picture of numbers whose zero is undocumented, so it
    // says so on the figure itself rather than only in the table beside it.
    let reference: Option<String> = crate::pipeline::stats::is_susceptibility(map)
        .then(|| panel.iter().find_map(|r| r.reference.as_ref().map(|spec| {
            crate::pipeline::stats::ReferenceInfo {
                spec: spec.clone(), offset_ppm: r.reference_offset_ppm,
            }.caption()
        })))
        .flatten();
    let head_h = HEAD_H + if reference.is_some() { REF_LINE_H } else { 0.0 };
    // Ranked by value: each map is its own figure, so there is no cross-panel order to preserve
    // and the readable arrangement wins.
    let mut rows: Vec<&&Row> = panel.iter().collect();
    rows.sort_by(|a, b| b.mean.partial_cmp(&a.mean).unwrap_or(std::cmp::Ordering::Equal));

    // Colour carries sign only where the map has both. On a map that never goes negative it would
    // encode nothing, so it becomes a single hue and the legend disappears with it.
    let diverging = rows.iter().any(|r| r.mean < 0.0) && rows.iter().any(|r| r.mean > 0.0);

    let plot_w = WIDTH - LABEL_W - RIGHT_W;
    let height = head_h + rows.len() as f64 * ROW_H + AXIS_H + PAD;
    let mut c = Canvas::new(WIDTH, height, SURFACE);

    c.text(fonts, title, PAD, PAD + 11.0, 13.0, Weight::Bold, Anchor::Start, INK);
    let unit = pretty_unit(rows.first().map(|r| r.unit.as_str()).unwrap_or(""));
    let head = if unit.is_empty() { map.to_string() } else { format!("{map}  ({unit})") };
    c.text(fonts, &head, PAD, PAD + 27.0, 11.5, Weight::Bold, Anchor::Start, INK2);
    c.text(fonts, subtitle, PAD, PAD + 41.0, 10.0, Weight::Regular, Anchor::Start, MUTED);
    if let Some(ref r) = reference {
        c.text(fonts, r, PAD, PAD + 41.0 + REF_LINE_H, 10.0, Weight::Regular, Anchor::Start, INK2);
    }

    if diverging {
        // Two colours in play, so identity never rests on colour alone.
        let x = LABEL_W + plot_w - 150.0;
        c.rect(x, PAD + 34.0, x + 9.0, PAD + 41.0, POS);
        c.text(fonts, "positive", x + 13.0, PAD + 41.0, 10.0, Weight::Regular, Anchor::Start, MUTED);
        c.rect(x + 66.0, PAD + 34.0, x + 75.0, PAD + 41.0, NEG);
        c.text(fonts, "negative", x + 79.0, PAD + 41.0, 10.0, Weight::Regular, Anchor::Start, MUTED);
    }

    // The scale spans the whiskers so no mark leaves the plot, and always includes zero: the bars
    // grow from it, and a baseline off-screen would make them meaningless.
    let (mut lo, mut hi) = (0.0f64, 0.0f64);
    for r in &rows {
        lo = lo.min(r.mean - r.sd);
        hi = hi.max(r.mean + r.sd);
    }
    if (hi - lo).abs() < f64::EPSILON {
        hi = 1.0;
    }
    // A log axis needs strictly positive values; anything else falls back to linear rather than
    // silently dropping the rows it cannot place.
    let log = style.log && rows.iter().all(|r| r.mean > 0.0);
    // Linear axes run to their ticks, which already bracket the data. A log axis runs to the data
    // instead, with gridlines at whatever decades fall inside: rounding the *span* out to whole
    // decades would leave most of the plot empty — 630 mm³ to 245 000 mm³ would stretch the axis
    // from 100 to a million, over a third of it showing nothing.
    let (lo, hi, ticks) = if log {
        let (dlo, dhi) = log_span(&rows);
        let ticks = decades_within(dlo, dhi);
        (dlo, dhi, ticks)
    } else {
        let ticks = nice_ticks(lo, hi);
        (ticks[0], ticks[ticks.len() - 1], ticks)
    };
    let pos = |v: f64| if log { v.max(f64::MIN_POSITIVE).log10() } else { v };
    let (plo, phi) = (pos(lo), pos(hi));
    let x_of = |v: f64| LABEL_W + (pos(v) - plo) / (phi - plo) * plot_w;

    let top = head_h;
    let bottom = top + rows.len() as f64 * ROW_H;
    for &t in &ticks {
        let x = x_of(t);
        let (col, w) = if t == 0.0 && !log { (AXIS, 1.0) } else { (GRID, 0.5) };
        c.line_v(x, top, bottom, w, col);
        let label = if log { fmt_compact(t) } else { fmt_val(t, hi - lo) };
        c.text(fonts, &label, x, bottom + 14.0, 9.5, Weight::Regular, Anchor::Middle, MUTED);
    }

    let (min_name, max_name) = (
        rows.last().map(|r| r.name.clone()).unwrap_or_default(),
        rows.first().map(|r| r.name.clone()).unwrap_or_default(),
    );

    for (i, r) in rows.iter().enumerate() {
        let y = top + i as f64 * ROW_H;
        let mid = y + ROW_H / 2.0;
        c.text(fonts, &r.name, LABEL_W - 9.0, mid + 3.3, 10.0, Weight::Regular, Anchor::End, INK2);

        let x1 = x_of(r.mean);
        let fill = if !diverging { NEUTRAL } else if r.mean < 0.0 { NEG } else { POS };
        if !log {
            c.bar(x_of(0.0), x1, y + (ROW_H - BAR_H) / 2.0, BAR_H, fill);
        }

        // An I-beam. Over a bar it goes on top: a surface-coloured under-stroke would read as a
        // white slot cut through the data, which is worse than the overlap it avoids. Under a dot
        // it goes beneath, with the dot carrying a surface ring, so the arms emerge from a clean
        // mark instead of a line being drawn across it.
        let (mut wl, mut wr) = (x1, x1);
        if r.sd > 0.0 {
            wl = x_of(r.mean - r.sd).max(LABEL_W);
            wr = x_of(r.mean + r.sd).min(LABEL_W + plot_w);
            c.line_h(wl, wr, mid, 1.0, INK2);
            c.line_v(wl, mid - 2.5, mid + 2.5, 1.0, INK2);
            c.line_v(wr, mid - 2.5, mid + 2.5, 1.0, INK2);
        }
        if log {
            c.dot(x1, mid, 4.5, SURFACE);
            c.dot(x1, mid, 3.5, fill);
        }

        // Only the extremes are labelled. Past the whisker, not the bar — the whisker is what
        // actually ends the mark — and inside the zero line when there is no room outside, so a
        // long negative bar never prints its value over a structure name.
        if style.label_every || r.name == min_name || r.name == max_name {
            let text = if log { label_with_spread(r) } else { fmt_val(r.mean, hi - lo) };
            let w = fonts.width(&text, 9.5, Weight::Regular);
            let (tx, anchor) = if r.mean < 0.0 && wl - 7.0 - w >= LABEL_W {
                (wl - 7.0, Anchor::End)
            } else if r.mean < 0.0 {
                (x_of(if log { lo } else { 0.0 }) + 7.0, Anchor::Start)
            } else {
                (wr + 7.0, Anchor::Start)
            };
            c.text(fonts, &text, tx, mid + 3.3, 9.5, Weight::Regular, anchor, INK2);
        }
    }
    c
}

/// Tick positions spanning `lo..hi` on 1/2/5 × 10^k steps, always including zero.
fn nice_ticks(lo: f64, hi: f64) -> Vec<f64> {
    let (lo, hi) = (lo.min(0.0), hi.max(0.0));
    let span = (hi - lo).max(f64::MIN_POSITIVE);
    let raw = span / 5.0;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0].iter().map(|m| m * mag)
        .find(|s| *s >= raw).unwrap_or(mag * 10.0);
    let start = (lo / step).floor() * step;
    let n = (((hi / step).ceil() * step - start) / step).round() as usize;
    (0..=n).map(|i| {
        let v = start + i as f64 * step;
        if v.abs() < step * 1e-9 { 0.0 } else { v }
    }).collect()
}

/// The span a log axis should cover: the data, whiskers included, with a little air either side.
///
/// Padding is a fraction of a decade rather than a fraction of the value, since on a log axis that
/// is what reads as an even margin.
fn log_span(rows: &[&&Row]) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::INFINITY, 0.0f64);
    for r in rows {
        // A whisker may reach below zero in linear terms; on a log axis it stops short of the dot.
        lo = lo.min((r.mean - r.sd).max(r.mean * 0.5));
        hi = hi.max(r.mean + r.sd);
    }
    if !lo.is_finite() || lo <= 0.0 {
        lo = hi.max(1.0) / 10.0;
    }
    const PAD_DECADES: f64 = 0.12;
    (10f64.powf(lo.log10() - PAD_DECADES), 10f64.powf(hi.log10() + PAD_DECADES))
}

/// Whole decades inside `lo..hi`, for gridlines. At most one decade is spanned when the data are
/// tight, so a single tick is kept rather than none.
fn decades_within(lo: f64, hi: f64) -> Vec<f64> {
    let (lo_e, hi_e) = (lo.log10().ceil() as i32, hi.log10().floor() as i32);
    let ticks: Vec<f64> = (lo_e..=hi_e).map(|e| 10f64.powi(e)).collect();
    if ticks.is_empty() {
        vec![10f64.powf(((lo.log10() + hi.log10()) / 2.0).round())]
    } else {
        ticks
    }
}

/// A log-axis label: the value, and the spread as a percentage when there is one.
///
/// A log axis compresses a whisker toward nothing — a 7% spread on an axis spanning three decades
/// is narrower than the dot that hides it — so the figure would promise whiskers it does not
/// show. The percentage says the same thing and stays readable at any scale. The whisker is still
/// drawn, and still visible where the spread is actually wide.
fn label_with_spread(r: &Row) -> String {
    let v = fmt_compact(r.mean);
    if r.sd > 0.0 && r.mean > 0.0 {
        format!("{v}  \u{b1}{:.0}%", 100.0 * r.sd / r.mean)
    } else {
        v
    }
}

/// A value at a glance: `630`, `7.6k`, `245k`.
///
/// Thirty exact figures down the side of a chart is a table pretending to be a picture; the TSV is
/// where exact volumes live.
fn fmt_compact(v: f64) -> String {
    let a = v.abs();
    if a >= 1e6 { format!("{:.1}M", v / 1e6) }
    else if a >= 1e4 { format!("{:.0}k", v / 1e3) }
    else if a >= 1e3 { format!("{:.1}k", v / 1e3) }
    else if a >= 10.0 { format!("{v:.0}") }
    else if a >= 1.0 { format!("{v:.1}") }
    else { format!("{v:.3}") }
}

/// Enough decimals to tell neighbouring ticks apart, and no more.
fn fmt_val(v: f64, span: f64) -> String {
    let dp = if span >= 50.0 { 0 } else if span >= 5.0 { 1 } else if span >= 0.5 { 2 } else { 3 };
    let s = format!("{v:.dp$}");
    if s.trim_start_matches('-').chars().all(|c| c == '0' || c == '.') { "0".into() } else { s }
}

/// How a unit is spelled on a figure.
///
/// `1/s` rather than `s⁻¹`: Liberation Sans has no superscript minus (U+207B), so the proper SI
/// form renders as `s ¹` — a missing glyph draws as nothing and says nothing about why.
pub fn pretty_unit(unit: &str) -> String {
    match unit {
        "s-1" => "1/s".to_string(),
        "mm3" => "mm\u{b3}".to_string(),
        other => other.to_string(),
    }
}

/// Collapse a group table to one row per (map, structure): the mean of each run's mean, with the
/// spread *across runs* as the SD.
///
/// This is a different quantity from the per-run figure's whisker, which is the spread of voxels
/// inside one structure. The subtitle says which, because a reader cannot tell them apart by
/// looking and would otherwise read cohort variability as within-structure variability.
pub fn aggregate_runs(rows: &[Row]) -> (Vec<Row>, usize) {
    let mut keys: Vec<(String, String)> = Vec::new();
    for r in rows {
        let k = (r.map.clone(), r.name.clone());
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    let mut n_runs = 0usize;
    let out = keys.into_iter().filter_map(|(map, name)| {
        let means: Vec<f64> = rows.iter()
            .filter(|r| r.map == map && r.name == name)
            .map(|r| r.mean)
            .collect();
        if means.is_empty() {
            return None;
        }
        n_runs = n_runs.max(means.len());
        let mean = means.iter().sum::<f64>() / means.len() as f64;
        // Sample SD: these runs are a sample of the population the cohort stands for, unlike the
        // voxels of a structure, which are all of it.
        let sd = if means.len() > 1 {
            (means.iter().map(|m| (m - mean).powi(2)).sum::<f64>() / (means.len() - 1) as f64).sqrt()
        } else {
            0.0
        };
        let unit = rows.iter().find(|r| r.map == map).map(|r| r.unit.clone()).unwrap_or_default();
        let vols: Vec<f64> = rows.iter()
            .filter(|r| r.map == map && r.name == name)
            .filter_map(|r| r.volume_mm3)
            .collect();
        let volume_mm3 = (!vols.is_empty()).then(|| vols.iter().sum::<f64>() / vols.len() as f64);
        // The cohort's spread in volume, on the same terms as its spread in the map's values.
        let volume_sd_mm3 = match (volume_mm3, vols.len()) {
            (Some(m), n) if n > 1 => Some(
                (vols.iter().map(|v| (v - m).powi(2)).sum::<f64>() / (n - 1) as f64).sqrt()),
            _ => None,
        };
        // The reference travels with the cohort rows too, when every run used the same one.
        let refs: Vec<&str> = rows.iter()
            .filter(|r| r.map == map && r.name == name)
            .filter_map(|r| r.reference.as_deref())
            .collect();
        let reference = match refs.first() {
            Some(first) if refs.iter().all(|r| r == first) => Some((*first).to_string()),
            Some(_) => Some("mixed".to_string()),
            None => None,
        };
        // A cohort has no single offset — each run removed its own — so it is not averaged into
        // one. The per-run tables and sidecars carry them.
        Some(Row { map, unit, name, mean, sd, volume_mm3, volume_sd_mm3,
                   reference, reference_offset_ppm: None })
    }).collect();
    (out, n_runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(map: &str, name: &str, mean: f64, sd: f64) -> Row {
        Row { map: map.into(), unit: "ppm".into(), name: name.into(), mean, sd,
              volume_mm3: None, volume_sd_mm3: None, reference: None,
              reference_offset_ppm: None }
    }

    /// One file per map, named so it sits beside the TSV without colliding or doubling `desc-`.
    #[test]
    fn each_map_gets_its_own_file() {
        let rows = vec![
            row("Chimap", "a", 0.1, 0.0),
            row("R2starmap", "a", 20.0, 0.0),
            row("desc-paramagnetic_Chimap", "a", 0.2, 0.0),
        ];
        let figs = render_all(&rows, "sub-01", "sub", "vol").unwrap();
        let stems: Vec<&str> = figs.iter().map(|f| f.stem.as_str()).collect();
        assert_eq!(stems, ["desc-chimap_stats", "desc-r2starmap_stats",
                           "desc-paramagnetic-chimap_stats"]);
        for s in &stems {
            assert_eq!(s.matches("desc-").count(), 1, "{s} doubles the desc- entity");
        }
        assert!(render_all(&[], "t", "s", "v").unwrap().is_empty());
    }

    /// Volume spans orders of magnitude, so it is drawn on a log axis — where a bar would encode
    /// length from an arbitrary floor rather than from zero, so the marks are dots and every value
    /// is labelled instead of just the extremes.
    #[test]
    fn volume_is_a_labelled_log_dot_plot() {
        let mut rows: Vec<Row> = ["cortex", "thalamus", "accumbens"].iter().zip([245000.0, 7600.0, 630.0])
            .map(|(n, v)| {
                let mut r = row("Chimap", n, 0.01, 0.0);
                r.volume_mm3 = Some(v);
                r
            }).collect();
        rows[0].map = "Chimap".into();

        let figs = render_all(&rows, "sub-01", "s", "v").unwrap();
        assert!(figs.iter().any(|f| f.stem == "desc-volume_stats"), "a volume figure is drawn");

        // Ticks are decades bracketing the data, so the smallest structure is not on the edge.
        let vol = volume_rows(&rows);
        let panel: Vec<&Row> = vol.iter().collect();
        let as_refs: Vec<&&Row> = panel.iter().collect();
        let (lo, hi) = log_span(&as_refs);
        // The axis hugs the data instead of rounding out to whole decades, which would have left
        // a third of the plot empty.
        assert!(lo > 100.0 && lo < 630.0, "axis starts at {lo}, too far below the smallest value");
        assert!(hi > 245_000.0 && hi < 1e6, "axis ends at {hi}, too far above the largest");
        for t in decades_within(lo, hi) {
            assert!((t.log10() - t.log10().round()).abs() < 1e-9, "{t} is not a decade");
            assert!(t >= lo && t <= hi, "gridline {t} is off the axis {lo}..{hi}");
        }
        // Tight data still gets a gridline to read against.
        assert_eq!(decades_within(900.0, 1100.0).len(), 1);

        // Compact labels, since thirty exact figures would be a table pretending to be a picture.
        assert_eq!(fmt_compact(245000.0), "245k");
        assert_eq!(fmt_compact(7600.0), "7.6k");
        assert_eq!(fmt_compact(630.0), "630");
        assert_eq!(fmt_compact(2_400_000.0), "2.4M");

        // A log axis compresses the whisker away, so the spread rides in the label instead.
        let mut r = row("volume", "x", 7600.0, 0.0);
        assert_eq!(label_with_spread(&r), "7.6k", "no spread, no plus-minus");
        r.sd = 380.0;
        assert_eq!(label_with_spread(&r), "7.6k  \u{b1}5%");

        // A log axis compresses the whisker away, so the spread rides in the label instead.
        let mut r = row("volume", "x", 7600.0, 0.0);
        assert_eq!(label_with_spread(&r), "7.6k", "no spread, no ±");
        r.sd = 380.0;
        assert_eq!(label_with_spread(&r), "7.6k  ±5%");

        // A value that cannot sit on a log axis makes the panel fall back to linear rather than
        // dropping the row or taking the log of zero.
        let mut zeroed = vol.clone();
        zeroed[0].mean = 0.0;
        let fonts = Fonts::load().unwrap();
        let panel: Vec<&Row> = zeroed.iter().collect();
        let _ = draw(&fonts, &panel, "volume", "t", "s", Style::LOG_DOTS);
    }

    /// A susceptibility figure has to say what its zero means; an R2* figure has no reference to
    /// claim, and the header must grow to fit the extra line rather than print it over row 0.
    #[test]
    fn susceptibility_figures_name_their_reference() {
        let referenced = |map: &str| {
            let mut r = row(map, "thalamus", 0.01, 0.002);
            r.reference = Some("ventricles".into());
            r.reference_offset_ppm = Some(0.0125);
            r
        };
        let fonts = Fonts::load().unwrap();
        let chi = [referenced("Chimap")];
        let with = draw(&fonts, &chi.iter().collect::<Vec<_>>(), "Chimap", "sub-01", "s", Style::BARS);
        let plain = [row("Chimap", "thalamus", 0.01, 0.002)];
        let without = draw(&fonts, &plain.iter().collect::<Vec<_>>(), "Chimap", "sub-01", "s", Style::BARS);
        assert_eq!(with.height() as f64 - without.height() as f64,
                   REF_LINE_H * crate::pipeline::canvas::SCALE,
                   "the reference line has to make room for itself");

        // Desc-qualified susceptibility maps count; the relaxometry maps do not.
        assert!(crate::pipeline::stats::is_susceptibility("Chimap"));
        assert!(crate::pipeline::stats::is_susceptibility("desc-paramagnetic_Chimap"));
        assert!(!crate::pipeline::stats::is_susceptibility("R2starmap"));
        let r2 = [referenced("R2starmap")];
        assert_eq!(draw(&fonts, &r2.iter().collect::<Vec<_>>(), "R2starmap", "sub-01", "s", Style::BARS).height(),
                   without.height(), "R2* must not grow a reference line");
    }

    /// Nothing in the header may reach into the first row's band: the subtitle and the legend
    /// both sit just above it, and a few pixels of drift puts them on top of a bar.
    #[test]
    fn the_header_clears_the_first_row() {
        let lowest_header_baseline = PAD + 41.0;
        assert!(HEAD_H > lowest_header_baseline + 4.0,
                "HEAD_H {HEAD_H} leaves the subtitle at {lowest_header_baseline} inside row 0");
    }

    /// Ticks have to bracket the data and land on zero exactly, or the baseline the bars grow from
    /// sits somewhere that is not zero.
    #[test]
    fn ticks_bracket_the_data_and_include_zero() {
        for (lo, hi) in [(-0.05, 0.12), (0.0, 45.0), (-3.0, -0.5), (0.02, 0.02)] {
            let t = nice_ticks(lo, hi);
            assert!(t[0] <= lo.min(0.0), "{lo}..{hi} -> {t:?}");
            assert!(t[t.len() - 1] >= hi.max(0.0), "{lo}..{hi} -> {t:?}");
            assert!(t.contains(&0.0), "zero must be a tick: {lo}..{hi} -> {t:?}");
            assert!(t.len() >= 2 && t.len() <= 14, "{lo}..{hi} -> {} ticks", t.len());
        }
    }

    #[test]
    fn values_are_formatted_to_the_axis_they_sit_on() {
        assert_eq!(fmt_val(0.0, 0.2), "0");
        assert_eq!(fmt_val(-0.0, 0.2), "0", "negative zero is still zero");
        assert_eq!(fmt_val(0.118, 0.2), "0.118");
        assert_eq!(fmt_val(45.0, 60.0), "45");
        assert_eq!(pretty_unit("s-1"), "1/s");
        assert_eq!(pretty_unit("ppm"), "ppm");
        assert_eq!(pretty_unit("mm3"), "mm³");
    }

    /// The group view averages each run's mean and reports the spread *between* runs — the same
    /// columns as a per-run table, but a different quantity.
    #[test]
    fn the_group_view_summarises_across_runs() {
        let rows = vec![
            row("Chimap", "thalamus", 0.010, 0.05),
            row("Chimap", "thalamus", 0.020, 0.06),
            row("Chimap", "thalamus", 0.030, 0.04),
            row("Chimap", "pallidum", 0.100, 0.03),
        ];
        let (agg, n) = aggregate_runs(&rows);
        assert_eq!(n, 3, "three runs contributed a thalamus");
        assert_eq!(agg.len(), 2, "one row per (map, structure)");

        let th = agg.iter().find(|r| r.name == "thalamus").unwrap();
        assert!((th.mean - 0.02).abs() < 1e-12, "mean of the run means");
        // Sample SD of 0.01/0.02/0.03 is 0.01 — not the per-run voxel SDs, which are discarded.
        assert!((th.sd - 0.01).abs() < 1e-12, "sd was {}", th.sd);

        // A structure only one run has gets no spread rather than a fabricated one.
        let pa = agg.iter().find(|r| r.name == "pallidum").unwrap();
        assert_eq!(pa.sd, 0.0);

        // Volume gets a spread of its own, which is not the map's spread.
        let with_vol: Vec<Row> = [1000.0, 1100.0, 1200.0].iter().map(|v| {
            let mut r = row("Chimap", "thalamus", 0.02, 0.05);
            r.volume_mm3 = Some(*v);
            r
        }).collect();
        let (agg, _) = aggregate_runs(&with_vol);
        let th = &agg[0];
        assert_eq!(th.volume_mm3, Some(1100.0));
        assert!((th.volume_sd_mm3.unwrap() - 100.0).abs() < 1e-9, "{:?}", th.volume_sd_mm3);
        // And it reaches the volume figure's rows as their spread.
        let vol = volume_rows(&agg);
        assert!((vol[0].sd - 100.0).abs() < 1e-9, "the volume figure should show its own spread");
        assert_eq!(label_with_spread(&vol[0]), "1.1k  \u{b1}9%");
    }

    /// Not a test — a way to eyeball the renderer, since nothing else here can judge whether a
    /// figure actually reads. Writes into the system temp directory and prints where.
    ///
    /// `cargo test -p qsmxt preview_figures -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn preview_figures() {
        // Plausible shapes rather than real data: a map that changes sign, one that does not, and
        // volumes spanning three orders of magnitude.
        let structures: [(&str, f64, f64, f64); 8] = [
            ("left pallidum", 0.118, 0.031, 1900.0),
            ("left putamen", 0.058, 0.024, 4900.0),
            ("left thalamus", 0.011, 0.014, 7600.0),
            ("left cerebral cortex", 0.006, 0.020, 245_000.0),
            ("left lateral ventricle", -0.009, 0.010, 7800.0),
            ("left cerebral white matter", -0.021, 0.013, 220_000.0),
            ("left accumbens area", 0.034, 0.018, 650.0),
            ("brain-stem", 0.019, 0.013, 21000.0),
        ];
        let mut rows = Vec::new();
        for (name, chi, sd, vol) in structures {
            for (map, unit, mean, sd) in [
                ("Chimap", "ppm", chi, sd), ("R2starmap", "s-1", 18.0 + chi * 200.0, sd * 100.0),
            ] {
                rows.push(Row {
                    map: map.into(), unit: unit.into(), name: name.into(), mean, sd,
                    volume_mm3: Some(vol), volume_sd_mm3: Some(vol * 0.08),
                    reference: (map == "Chimap").then(|| "ventricles".to_string()),
                    reference_offset_ppm: (map == "Chimap").then_some(-0.0084),
                });
            }
        }
        let dir = std::env::temp_dir().join("qsmxt-figure-preview");
        for f in render_all(&rows, "sub-01",
                            "Mean per structure; whiskers ±1 SD over the structure's voxels",
                            "Volume from the SynthSeg posteriors").unwrap() {
            f.save_in(&dir, "sub-01").unwrap();
        }
        let (agg, n) = aggregate_runs(&rows);
        for f in render_all(&agg, &format!("All runs (n = {n})"),
                            &format!("Mean of {n} run means; whiskers ±1 SD across runs"),
                            &format!("Mean volume over {n} runs; ± is 1 SD across runs")).unwrap() {
            f.save_in(&dir.join("group"), "").unwrap();
        }
        println!("figures written to {}", dir.display());
    }

    /// Rendering must not panic on the shapes real data takes, and must produce a real image.
    #[test]
    fn awkward_data_still_renders() {
        let cases: Vec<Vec<Row>> = vec![
            vec![row("Chimap", "only", 0.0, 0.0)],                    // everything at zero
            vec![row("Chimap", "a", -0.5, 0.2), row("Chimap", "b", -0.1, 0.1)], // all negative
            vec![row("Chimap", "a", 1e-9, 1e-10)],                    // tiny
            vec![row("Chimap", "a very long structure name indeed", 1.0, 0.5)],
        ];
        for rows in cases {
            let figs = render_all(&rows, "t", "s", "v").unwrap();
            assert_eq!(figs.len(), 1);
            let dir = tempfile::tempdir().unwrap();
            let p = figs[0].save_in(dir.path(), "sub-01").unwrap();
            assert!(std::fs::metadata(&p).unwrap().len() > 0);
            assert!(p.file_name().unwrap().to_str().unwrap().starts_with("sub-01_desc-"));
        }
    }
}
