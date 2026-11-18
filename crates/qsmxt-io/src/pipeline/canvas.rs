//! A small RGB canvas: filled rectangles, hairlines, and text, encoded as PNG.
//!
//! Everything the per-structure figures draw is an axis-aligned rectangle, a one-pixel line, or a
//! run of glyphs, so this is a few hundred lines rather than a 2D graphics stack. Glyphs come from
//! `fontdue`; the rest is direct pixel work.
//!
//! Drawing happens at [`SCALE`]× and the result is the image itself — no downsampling — so text is
//! rasterised at its true device size rather than being smoothed twice.

use std::path::Path;

use fontdue::{Font, FontSettings};

use crate::error::QsmxtError;

/// Device pixels per layout unit. Layout is written in comfortable on-screen units and rendered at
/// twice that, which is what stops 11px labels looking soft.
pub const SCALE: f64 = 2.0;

/// Liberation Sans, subsetted to the characters the figures use. Embedded rather than loaded from
/// the system so a figure looks the same everywhere it is produced, containers included.
/// SIL OFL 1.1 — see `assets/README.md`.
static REGULAR: &[u8] = include_bytes!("../../assets/sans.ttf");
static BOLD: &[u8] = include_bytes!("../../assets/sans-bold.ttf");

pub struct Fonts {
    regular: Font,
    bold: Font,
}

impl Fonts {
    pub fn load() -> crate::Result<Self> {
        let one = |b: &[u8]| {
            Font::from_bytes(b, FontSettings::default())
                .map_err(|e| QsmxtError::Config(format!("embedded font: {e}")))
        };
        Ok(Self { regular: one(REGULAR)?, bold: one(BOLD)? })
    }

    fn pick(&self, weight: Weight) -> &Font {
        match weight {
            Weight::Regular => &self.regular,
            Weight::Bold => &self.bold,
        }
    }

    /// Width of `text` at `size` layout units, in layout units.
    ///
    /// Measured from the font rather than estimated, which is what lets labels be placed against
    /// their real extent instead of a guess.
    pub fn width(&self, text: &str, size: f64, weight: Weight) -> f64 {
        let font = self.pick(weight);
        let px = (size * SCALE) as f32;
        text.chars().map(|c| font.metrics(c, px).advance_width as f64).sum::<f64>() / SCALE
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Weight { Regular, Bold }

#[derive(Clone, Copy, PartialEq)]
pub enum Anchor { Start, Middle, End }

/// An sRGB colour.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Parse `#rrggbb`. Panics on anything else — every caller passes a literal from the palette.
    pub const fn hex(s: &str) -> Self {
        let b = s.as_bytes();
        assert!(b.len() == 7 && b[0] == b'#', "expected #rrggbb");
        const fn n(c: u8) -> u8 {
            match c {
                b'0'..=b'9' => c - b'0',
                b'a'..=b'f' => c - b'a' + 10,
                b'A'..=b'F' => c - b'A' + 10,
                _ => panic!("bad hex digit"),
            }
        }
        Rgb(n(b[1]) * 16 + n(b[2]), n(b[3]) * 16 + n(b[4]), n(b[5]) * 16 + n(b[6]))
    }
}

pub struct Canvas {
    w: usize,
    h: usize,
    px: Vec<u8>,
}

impl Canvas {
    /// A canvas `w` × `h` *layout units*, filled with `bg`.
    pub fn new(w: f64, h: f64, bg: Rgb) -> Self {
        let (w, h) = ((w * SCALE) as usize, (h * SCALE) as usize);
        let mut px = Vec::with_capacity(w * h * 3);
        for _ in 0..w * h {
            px.extend_from_slice(&[bg.0, bg.1, bg.2]);
        }
        Self { w, h, px }
    }

    #[cfg(test)]
    pub fn width(&self) -> usize { self.w }
    #[cfg(test)]
    pub fn height(&self) -> usize { self.h }

    /// Blend `c` over the pixel at (`x`, `y`) with coverage `a` in 0..=1.
    fn blend(&mut self, x: isize, y: isize, c: Rgb, a: f64) {
        if a <= 0.0 || x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return;
        }
        let a = a.min(1.0);
        let i = (y as usize * self.w + x as usize) * 3;
        for (k, ch) in [c.0, c.1, c.2].into_iter().enumerate() {
            let old = self.px[i + k] as f64;
            self.px[i + k] = (old + (ch as f64 - old) * a).round() as u8;
        }
    }

    /// A filled rectangle in layout units, antialiased on every edge.
    ///
    /// Coverage is the overlap of the pixel square with the rectangle, computed per axis and
    /// multiplied — exact for an axis-aligned rectangle, and it makes a hairline at a fractional
    /// position land as a soft line rather than jumping a whole pixel.
    pub fn rect(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, c: Rgb) {
        let (x0, x1) = (x0.min(x1) * SCALE, x0.max(x1) * SCALE);
        let (y0, y1) = (y0.min(y1) * SCALE, y0.max(y1) * SCALE);
        for y in y0.floor() as isize..=y1.ceil() as isize {
            let cy = ((y as f64 + 1.0).min(y1) - (y as f64).max(y0)).clamp(0.0, 1.0);
            if cy <= 0.0 {
                continue;
            }
            for x in x0.floor() as isize..=x1.ceil() as isize {
                let cx = ((x as f64 + 1.0).min(x1) - (x as f64).max(x0)).clamp(0.0, 1.0);
                self.blend(x, y, c, cx * cy);
            }
        }
    }

    /// A horizontal bar growing from `x_base` to `x_end`, with the data end rounded.
    ///
    /// The baseline end stays square: it is where every bar starts, and a row of rounded stubs at
    /// zero would read as a column of marks rather than a shared origin.
    pub fn bar(&mut self, x_base: f64, x_end: f64, y: f64, h: f64, c: Rgb) {
        let r = (h / 2.0).min((x_end - x_base).abs() / 2.0).min(4.0);
        if r <= 0.0 {
            return;
        }
        let dir = if x_end >= x_base { 1.0 } else { -1.0 };
        // The straight part, then the rounded cap as a quarter-disc pair.
        self.rect(x_base, y, x_end - dir * r, y + h, c);
        let (cx, cy_t, cy_b) = (x_end - dir * r, y + r, y + h - r);
        self.rect(cx, y + r, x_end, y + h - r, c);
        for (cy, sy) in [(cy_t, -1.0), (cy_b, 1.0)] {
            let (px0, px1) = (cx.min(x_end), cx.max(x_end));
            let (py0, py1) = if sy < 0.0 { (y, cy) } else { (cy, y + h) };
            for py in (py0 * SCALE).floor() as isize..=(py1 * SCALE).ceil() as isize {
                for px in (px0 * SCALE).floor() as isize..=(px1 * SCALE).ceil() as isize {
                    let (fx, fy) = ((px as f64 + 0.5) / SCALE, (py as f64 + 0.5) / SCALE);
                    if (fy - cy) * sy < 0.0 || (fx - cx) * dir < 0.0 {
                        continue;
                    }
                    let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
                    self.blend(px, py, c, ((r + 0.5 / SCALE - d) * SCALE).clamp(0.0, 1.0));
                }
            }
        }
    }

    /// A filled circle, antialiased at the rim.
    pub fn dot(&mut self, cx: f64, cy: f64, r: f64, c: Rgb) {
        let (cx, cy, r) = (cx * SCALE, cy * SCALE, r * SCALE);
        for py in (cy - r - 1.0).floor() as isize..=(cy + r + 1.0).ceil() as isize {
            for px in (cx - r - 1.0).floor() as isize..=(cx + r + 1.0).ceil() as isize {
                let d = ((px as f64 + 0.5 - cx).powi(2) + (py as f64 + 0.5 - cy).powi(2)).sqrt();
                self.blend(px, py, c, (r + 0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    /// A one-layout-unit-wide line. `width` is in layout units.
    pub fn line_h(&mut self, x0: f64, x1: f64, y: f64, width: f64, c: Rgb) {
        self.rect(x0, y - width / 2.0, x1, y + width / 2.0, c);
    }

    pub fn line_v(&mut self, x: f64, y0: f64, y1: f64, width: f64, c: Rgb) {
        self.rect(x - width / 2.0, y0, x + width / 2.0, y1, c);
    }

    /// Draw `text` with its baseline at `y`, anchored at `x`.
    #[allow(clippy::too_many_arguments)]
    pub fn text(
        &mut self, fonts: &Fonts, text: &str, x: f64, y: f64, size: f64,
        weight: Weight, anchor: Anchor, c: Rgb,
    ) {
        let font = fonts.pick(weight);
        let px_size = (size * SCALE) as f32;
        let start = match anchor {
            Anchor::Start => x,
            Anchor::Middle => x - fonts.width(text, size, weight) / 2.0,
            Anchor::End => x - fonts.width(text, size, weight),
        };
        let mut pen = start * SCALE;
        let baseline = y * SCALE;
        for ch in text.chars() {
            // A character the subset lacks maps to `.notdef`, which has an ordinary advance and
            // draws as blank space — the figure comes out looking merely oddly spaced. Two such
            // gaps reached rendered output before this assertion existed (`⁻` and `³`), each time
            // slipping past a coverage test that enumerated characters by hand rather than
            // checking what the code actually draws.
            debug_assert_ne!(font.lookup_glyph_index(ch), 0,
                             "no glyph for {ch:?} in the embedded font subset (drawing {text:?})");
            let (m, bitmap) = font.rasterize(ch, px_size);
            // fontdue reports `ymin` as the offset from the baseline to the bitmap's *bottom*, so
            // the top row sits that far above it, minus the bitmap's own height.
            let ox = pen + m.xmin as f64;
            let oy = baseline - m.ymin as f64 - m.height as f64;
            for row in 0..m.height {
                for col in 0..m.width {
                    let a = bitmap[row * m.width + col] as f64 / 255.0;
                    self.blend((ox as isize) + col as isize, (oy as isize) + row as isize, c, a);
                }
            }
            pen += m.advance_width as f64;
        }
    }

    /// Write the canvas as an 8-bit RGB PNG.
    pub fn save(&self, path: &Path) -> crate::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::File::create(path)?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), self.w as u32, self.h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        // The figure is drawn at SCALE×, so tag it as such: a viewer that honours pHYs shows it at
        // its intended size instead of twice as large.
        enc.set_pixel_dims(Some(png::PixelDimensions {
            xppu: (2835.0 * SCALE) as u32, yppu: (2835.0 * SCALE) as u32, // 72dpi × SCALE, in px/m
            unit: png::Unit::Meter,
        }));
        enc.write_header()
            .and_then(|mut w| w.write_image_data(&self.px))
            .map_err(|e| QsmxtError::Config(format!("{}: {e}", path.display())))?;
        Ok(())
    }

    #[cfg(test)]
    fn at(&self, x: usize, y: usize) -> Rgb {
        let i = (y * self.w + x) * 3;
        Rgb(self.px[i], self.px[i + 1], self.px[i + 2])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgb = Rgb::hex("#e34948");
    const WHITE: Rgb = Rgb::hex("#ffffff");

    #[test]
    fn hex_parses_at_compile_time() {
        assert_eq!(Rgb::hex("#000000"), Rgb(0, 0, 0));
        assert_eq!(Rgb::hex("#ffffff"), Rgb(255, 255, 255));
        assert_eq!(Rgb::hex("#e34948"), Rgb(0xe3, 0x49, 0x48));
    }

    /// A rectangle must cover the pixels inside it exactly and leave the ones outside alone —
    /// an off-by-one here shifts every mark in the figure by half a pixel.
    #[test]
    fn rectangles_land_where_they_are_asked_to() {
        let mut c = Canvas::new(10.0, 10.0, WHITE);
        c.rect(2.0, 2.0, 4.0, 4.0, RED);
        // Interior, in device pixels (2 × SCALE .. 4 × SCALE).
        assert_eq!(c.at(5, 5), RED);
        assert_eq!(c.at(4, 4), RED);
        assert_eq!(c.at(7, 7), RED);
        // Outside stays untouched on both sides.
        assert_eq!(c.at(3, 5), WHITE);
        assert_eq!(c.at(8, 5), WHITE);
        assert_eq!(c.at(5, 3), WHITE);
    }

    /// The baseline end is square and the data end is rounded, so the corner pixel at the data end
    /// is partly uncovered while the one at the baseline is solid.
    #[test]
    fn bars_are_square_at_the_baseline_and_round_at_the_data_end() {
        let mut c = Canvas::new(40.0, 20.0, WHITE);
        c.bar(5.0, 35.0, 5.0, 11.0, RED);
        let (top, bottom) = ((5.0 * SCALE) as usize, (16.0 * SCALE) as usize - 1);
        let base = (5.0 * SCALE) as usize;
        assert_eq!(c.at(base, top), RED, "the baseline corner is square");
        assert_eq!(c.at(base, bottom), RED, "and so is the other one");

        let end = (35.0 * SCALE) as usize - 1;
        assert_ne!(c.at(end, top), RED, "the data end is rounded away");
        // The middle of the data end is still solid.
        assert_eq!(c.at(end, (10.5 * SCALE) as usize), RED);
    }

    /// A bar shorter than the corner radius must not invert or reach past its own end.
    #[test]
    fn a_very_short_bar_stays_inside_itself() {
        let mut c = Canvas::new(20.0, 10.0, WHITE);
        c.bar(10.0, 10.5, 2.0, 6.0, RED);
        for x in 0..(9.0 * SCALE) as usize {
            for y in 0..c.height() {
                assert_eq!(c.at(x, y), WHITE, "bar leaked left of its baseline at {x},{y}");
            }
        }
        for x in (11.0 * SCALE) as usize..c.width() {
            for y in 0..c.height() {
                assert_eq!(c.at(x, y), WHITE, "bar leaked past its data end at {x},{y}");
            }
        }
    }

    /// A dot must be centred where it is asked for and stay inside its radius — it is the only
    /// thing carrying the value on a log axis, where there are no bar lengths to fall back on.
    #[test]
    fn dots_are_round_and_centred() {
        let mut c = Canvas::new(20.0, 20.0, WHITE);
        c.dot(10.0, 10.0, 4.0, RED);
        assert_eq!(c.at((10.0 * SCALE) as usize, (10.0 * SCALE) as usize), RED, "solid at centre");
        for (dx, dy) in [(3.0, 0.0), (0.0, 3.0), (-3.0, 0.0), (0.0, -3.0)] {
            assert_eq!(c.at(((10.0 + dx) * SCALE) as usize, ((10.0 + dy) * SCALE) as usize), RED,
                       "solid {dx},{dy} from the centre");
        }
        // Well outside the radius, nothing.
        assert_eq!(c.at((15.0 * SCALE) as usize, (10.0 * SCALE) as usize), WHITE);
        assert_eq!(c.at((10.0 * SCALE) as usize, (16.0 * SCALE) as usize), WHITE);
    }

    /// Negative bars grow the other way from the same baseline.
    #[test]
    fn bars_grow_both_ways() {
        let mut c = Canvas::new(40.0, 20.0, WHITE);
        c.bar(20.0, 5.0, 5.0, 11.0, RED);
        assert_eq!(c.at((10.0 * SCALE) as usize, (10.0 * SCALE) as usize), RED);
        assert_eq!(c.at((30.0 * SCALE) as usize, (10.0 * SCALE) as usize), WHITE);
    }

    /// Text must sit on its baseline, advance to the right, and measure what it draws — the
    /// measurement is what every label placement decision is made against.
    #[test]
    fn text_sits_on_its_baseline_and_measures_itself() {
        let f = Fonts::load().unwrap();
        let w = f.width("0.118", 10.0, Weight::Regular);
        assert!(w > 10.0 && w < 40.0, "implausible width {w} for a 5-character 10px number");
        assert!(f.width("00", 10.0, Weight::Regular) > f.width("0", 10.0, Weight::Regular));
        assert!(f.width("X", 10.0, Weight::Bold) >= f.width("X", 10.0, Weight::Regular));

        let mut c = Canvas::new(60.0, 20.0, WHITE);
        c.text(&f, "Hxy", 5.0, 14.0, 11.0, Weight::Regular, Anchor::Start, Rgb::hex("#000000"));
        let ink = |y0: usize, y1: usize| (y0..y1).any(|y| (0..c.width()).any(|x| c.at(x, y) != WHITE));
        assert!(ink((6.0 * SCALE) as usize, (14.0 * SCALE) as usize), "glyphs above the baseline");
        assert!(!ink(0, (4.0 * SCALE) as usize), "nothing well above the cap height");
        assert!(!ink((17.0 * SCALE) as usize, c.height()), "nothing below the descender");
    }

    /// The characters the figures draw have to exist in the subset.
    ///
    /// Checked by glyph index, not by advance width: a missing character maps to `.notdef`, which
    /// has a perfectly ordinary advance, so measuring it tells you nothing. That is how a missing
    /// superscript minus got as far as a rendered figure.
    #[test]
    fn the_font_subset_covers_what_the_figures_draw() {
        let f = Fonts::load().unwrap();
        for (font, which) in [(&f.regular, "regular"), (&f.bold, "bold")] {
            for ch in "0123456789.,-+/() abcdefghijklmnopqrstuvwxyz\
                       ABCDEFGHIJKLMNOPQRSTUVWXYZ±χ—·".chars() {
                assert_ne!(font.lookup_glyph_index(ch), 0,
                           "{which} subset has no glyph for {ch:?} — it would draw as .notdef");
            }
        }
        // Every unit spelling the figures can produce, taken from the code rather than retyped.
        for unit in ["ppm", "s", "s-1", "mm3"] {
            for ch in crate::pipeline::figure::pretty_unit(unit).chars() {
                assert_ne!(f.regular.lookup_glyph_index(ch), 0,
                           "no glyph for {ch:?} in the unit {unit:?}");
            }
        }
        // The guard itself has to work: a character deliberately outside the subset must fail it.
        assert_eq!(f.regular.lookup_glyph_index('\u{4e2d}'), 0, "the check would never catch a gap");
    }

    #[test]
    fn a_canvas_round_trips_through_png() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("fig.png");
        let mut c = Canvas::new(20.0, 10.0, WHITE);
        c.rect(2.0, 2.0, 8.0, 8.0, RED);
        c.save(&path).unwrap();
        assert!(path.exists(), "save should create missing directories");

        let decoded = png::Decoder::new(std::fs::File::open(&path).unwrap())
            .read_info().unwrap();
        let info = decoded.info();
        assert_eq!((info.width, info.height), (c.width() as u32, c.height() as u32));
        assert_eq!(info.color_type, png::ColorType::Rgb);
    }
}
