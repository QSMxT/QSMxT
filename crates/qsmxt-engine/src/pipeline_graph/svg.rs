//! A [`PipelineGraph`] as a standalone SVG image, laid out by [`super::layout`]: the boxes, the
//! arrows between them labelled with what flows along them, and each pass of a two-pass run
//! boxed. Every box and arrow has a tooltip with its text in full.

use super::layout::{layout, num, rounded_path, KeyStyle, Layout, KEY_SAMPLE_W, LABEL_LINE};
use super::PipelineGraph;
use crate::record::StepState;

/// The drawing's style: the colours and type of qsmbly's Pipeline tab, light theme.
const STYLE: &str = "\
svg { font-family: system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif; font-size: 13px; }
.bg { fill: #ffffff; }
.g-group rect { fill: rgba(37, 99, 235, 0.05); stroke: #e1e5eb; }
.g-group text { fill: #6b7585; font-size: 10px; font-weight: 600; letter-spacing: 0.05em; text-transform: uppercase; }
.g-edge path { fill: none; stroke: #8a94a3; stroke-width: 1.3; }
.g-edge.collapsed path { stroke-width: 2.2; }
.g-label rect { fill: #ffffff; opacity: 0.92; }
.g-label text { fill: #4b5565; font-size: 10.5px; }
.g-node rect.box { fill: #ffffff; stroke: #4b5565; stroke-width: 1.1; }
.g-node.optional rect.box { stroke-dasharray: 6 4; }
.g-node.conditional rect.box { stroke-dasharray: 2 3; }
.g-node text { fill: #4b5565; font-size: 11px; }
.g-node text.title { fill: #1d2430; font-size: 12.5px; font-weight: 600; }
.g-node text.algorithm { fill: #1d2430; font-style: italic; font-size: 11.5px; }
.g-node text.when { fill: #6b7585; font-style: italic; }
.g-node text.run { fill: #2563eb; font-size: 10.5px; font-weight: 600; }
.g-node text.note { fill: #4b5565; font-size: 10px; }
.g-node.run-skipped rect.box { stroke-dasharray: 2 3; opacity: 0.7; }
.g-node.run-skipped text.run { fill: #6b7585; }
.g-node .model rect { fill: rgba(37, 99, 235, 0.10); stroke: #2563eb; }
.g-node .model text { fill: #2563eb; font-size: 9.5px; font-weight: 600; }
#g-arrow path { fill: #8a94a3; }
.g-key text { fill: #4b5565; font-size: 11px; }
";

/// `text` for XML text and attribute values.
fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

impl PipelineGraph {
    /// The graph laid out for drawing ([`super::layout::layout`]).
    pub fn layout(&self) -> Layout {
        layout(self)
    }

    /// The graph as a standalone SVG image.
    pub fn to_svg(&self) -> String {
        let l = self.layout();
        let mut out = String::new();
        let (w, h) = (num(l.width), num(l.height));
        out.push_str(&format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" role=\"img\" aria-label=\"Pipeline graph\">\n"
        ));
        out.push_str(&format!("<style>\n{STYLE}</style>\n"));
        out.push_str(
            "<defs><marker id=\"g-arrow\" viewBox=\"0 0 8 8\" refX=\"7\" refY=\"4\" markerWidth=\"7\" markerHeight=\"7\" \
             orient=\"auto-start-reverse\"><path d=\"M0,0.5 L8,4 L0,7.5 z\"/></marker></defs>\n",
        );
        out.push_str("<rect class=\"bg\" width=\"100%\" height=\"100%\"/>\n");
        for g in &l.groups {
            out.push_str(&format!(
                "<g class=\"g-group\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"8\"/></g>\n",
                num(g.x), num(g.y), num(g.w), num(g.h)
            ));
        }
        for e in &l.edges {
            let cls = if e.collapsed > 1 {
                "g-edge collapsed"
            } else {
                "g-edge"
            };
            out.push_str(&format!(
                "<g class=\"{cls}\" data-from=\"{}\" data-to=\"{}\"><title>{}</title><path d=\"{}\" marker-end=\"url(#g-arrow)\"/></g>\n",
                esc(&e.from), esc(&e.to), esc(&e.title), rounded_path(&e.points, 5.0)
            ));
        }
        for g in &l.groups {
            out.push_str(&format!(
                "<g class=\"g-group\"><text x=\"{}\" y=\"{}\">{}</text></g>\n",
                num(g.x + 8.0),
                num(g.y + 13.0),
                esc(&g.title)
            ));
        }
        for n in &l.nodes {
            let Some(node) = self.node(&n.id) else {
                continue;
            };
            let mut cls = String::from("g-node");
            if n.optional {
                cls.push_str(" optional");
            }
            if n.conditional {
                cls.push_str(" conditional");
            }
            if let Some(run) = &node.run {
                cls.push_str(match run.state {
                    StepState::Computed => " run-computed",
                    StepState::Cached => " run-cached",
                    StepState::Skipped => " run-skipped",
                });
            }
            out.push_str(&format!(
                "<g class=\"{cls}\" data-step=\"{}\" transform=\"translate({},{})\"><title>{}</title>\n",
                esc(&n.id), num(n.x), num(n.y), esc(&node.tooltip())
            ));
            out.push_str(&format!(
                "  <rect class=\"box\" width=\"{}\" height=\"{}\" rx=\"6\"/>\n",
                num(n.w),
                num(n.h)
            ));
            let mut y = 8.0;
            for line in &n.lines {
                y += line.cls.height();
                out.push_str(&format!(
                    "  <text class=\"{}\" x=\"8\" y=\"{}\">{}</text>\n",
                    line.cls.name(),
                    num(y - 4.0),
                    esc(&line.text)
                ));
            }
            if !node.models.is_empty() {
                out.push_str(&format!(
                    "  <g class=\"model\" transform=\"translate({},6)\"><rect width=\"38\" height=\"14\" rx=\"7\"/>\
                     <text x=\"19\" y=\"10.5\" text-anchor=\"middle\">model</text></g>\n",
                    num(n.w - 44.0)
                ));
            }
            out.push_str("</g>\n");
        }
        for e in &l.edges {
            let b = e.label_box;
            out.push_str(&format!(
                "<g class=\"g-label\"><title>{}</title><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"3\"/>",
                esc(&e.title), num(b.x), num(b.y), num(b.w), num(b.h)
            ));
            for (i, line) in e.label.iter().enumerate() {
                out.push_str(&format!(
                    "<text x=\"{}\" y=\"{}\">{}</text>",
                    num(b.x + 4.0),
                    num(b.y + (i as f64 + 1.0) * LABEL_LINE - 1.5),
                    esc(line)
                ));
            }
            out.push_str("</g>\n");
        }
        for k in &l.key {
            let (x, y) = (k.x, k.y);
            let sample = match k.style {
                KeyStyle::Optional => format!("<g class=\"g-node optional\"><rect class=\"box\" x=\"{}\" y=\"{}\" width=\"{KEY_SAMPLE_W}\" height=\"14\" rx=\"3\"/></g>", num(x), num(y)),
                KeyStyle::Conditional => format!("<g class=\"g-node conditional\"><rect class=\"box\" x=\"{}\" y=\"{}\" width=\"{KEY_SAMPLE_W}\" height=\"14\" rx=\"3\"/></g>", num(x), num(y)),
                KeyStyle::Skipped => format!("<g class=\"g-node run-skipped\"><rect class=\"box\" x=\"{}\" y=\"{}\" width=\"{KEY_SAMPLE_W}\" height=\"14\" rx=\"3\"/></g>", num(x), num(y)),
                KeyStyle::Model => format!("<g class=\"g-node\"><g class=\"model\" transform=\"translate({},{})\"><rect width=\"38\" height=\"14\" rx=\"7\"/><text x=\"19\" y=\"10.5\" text-anchor=\"middle\">model</text></g></g>", num(x - 6.0), num(y)),
                KeyStyle::Collapsed => format!("<g class=\"g-edge collapsed\"><path d=\"M{},{} L{},{}\" marker-end=\"url(#g-arrow)\"/></g>", num(x), num(y + 7.0), num(x + KEY_SAMPLE_W), num(y + 7.0)),
            };
            out.push_str(&format!(
                "<g class=\"g-key\">{sample}<text x=\"{}\" y=\"{}\">{}</text></g>\n",
                num(x + KEY_SAMPLE_W + 10.0),
                num(y + 11.0),
                esc(&k.text)
            ));
        }
        out.push_str("</svg>\n");
        out
    }
}
