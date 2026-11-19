//! A [`PipelineGraph`] as a Mermaid flowchart ([`PipelineGraph::to_mermaid`]).

use super::{output_text, Branch, Node, PipelineGraph};
use crate::record::StepState;

/// `text` inside a Mermaid quoted label: quotes and angle brackets as entities.
fn mermaid_text(text: &str) -> String {
    text.replace('&', "#amp;").replace('"', "#quot;").replace('<', "#lt;").replace('>', "#gt;")
}

/// A step name as a Mermaid node id (no `-`, which Mermaid reads as part of an arrow).
fn mermaid_id(id: &str) -> String {
    id.replace('-', "_")
}

impl PipelineGraph {
    /// The graph as a Mermaid flowchart: one box per step (label, algorithm, settings), each
    /// arrow labelled with what flows along it (an echo's items once, for all echoes). A two-pass
    /// run's passes are subgraphs; a step a setting asked for is dashed, and one that may do
    /// nothing is dotted.
    pub fn to_mermaid(&self) -> String {
        let mut out = String::from("flowchart TD\n");
        let node_line = |n: &Node| {
            let mut text = mermaid_text(&n.label);
            if let Some(alg) = &n.algorithm_label {
                text.push_str(&format!("<br/><i>{}</i>", mermaid_text(alg)));
            }
            for s in &n.settings {
                text.push_str(&format!("<br/><small>{}</small>", mermaid_text(s)));
            }
            if let Some(when) = &n.only_when {
                text.push_str(&format!("<br/><small>only when {}</small>", mermaid_text(when)));
            }
            if let Some(run) = &n.run {
                let state = match run.state {
                    StepState::Computed => "computed",
                    StepState::Cached => "reused from the cache",
                    StepState::Skipped => "skipped",
                };
                text.push_str(&format!("<br/><small><b>{state}</b></small>"));
                for d in &run.decisions {
                    text.push_str(&format!("<br/><small>{}</small>", mermaid_text(&d.note())));
                }
            }
            if let Some(d) = &n.dataset {
                for a in &d.alternatives {
                    text.push_str(&format!("<br/><small>{}</small>", mermaid_text(&d.alternative_text(a))));
                }
                for line in d.summary() {
                    text.push_str(&format!("<br/><small><b>{}</b></small>", mermaid_text(&line)));
                }
                for c in &d.decisions {
                    text.push_str(&format!("<br/><small>{}</small>", mermaid_text(&d.decision_note(c))));
                }
            }
            format!("{}[\"{text}\"]", mermaid_id(&n.id))
        };
        for n in self.nodes.iter().filter(|n| n.branch.is_none()) {
            out.push_str(&format!("    {}\n", node_line(n)));
        }
        for (branch, id, title) in [(Branch::Main, "main_pass", "Main pass"), (Branch::Reliable, "reliable_pass", "Reliable pass")] {
            let members: Vec<&Node> = self.nodes.iter().filter(|n| n.branch == Some(branch)).collect();
            if members.is_empty() {
                continue;
            }
            out.push_str(&format!("    subgraph {id}[\"{title}\"]\n"));
            for n in members {
                out.push_str(&format!("        {}\n", node_line(n)));
            }
            out.push_str("    end\n");
        }
        for e in &self.edges {
            let mut labels: Vec<String> = Vec::new();
            for key in &e.items {
                let label = self.item(key).map_or_else(|| key.clone(), |i| i.label.clone());
                if !labels.contains(&label) {
                    labels.push(label);
                }
            }
            out.push_str(&format!("    {} -->|\"{}\"| {}\n", mermaid_id(&e.from), mermaid_text(&labels.join(", ")), mermaid_id(&e.to)));
        }
        // The published outputs, as leaves.
        for n in &self.nodes {
            for (k, key) in n.publishes.iter().enumerate() {
                let leaf = format!("{}_out{}", mermaid_id(&n.id), k + 1);
                let text = output_text(key);
                out.push_str(&format!("    {leaf}([\"{}\"])\n", mermaid_text(text.trim_start_matches("→ "))));
                out.push_str(&format!("    {} --> {leaf}\n", mermaid_id(&n.id)));
            }
        }
        let optional: Vec<String> = self.nodes.iter().filter(|n| n.enabled_by.is_some()).map(|n| mermaid_id(&n.id)).collect();
        let conditional: Vec<String> = self.nodes.iter().filter(|n| n.only_when.is_some()).map(|n| mermaid_id(&n.id)).collect();
        if !optional.is_empty() {
            out.push_str("    classDef optional stroke-dasharray: 6 4\n");
            out.push_str(&format!("    class {} optional\n", optional.join(",")));
        }
        if !conditional.is_empty() {
            out.push_str("    classDef conditional stroke-dasharray: 2 3\n");
            out.push_str(&format!("    class {} conditional\n", conditional.join(",")));
        }
        let skipped: Vec<String> = self.nodes.iter()
            .filter(|n| n.run.as_ref().is_some_and(|r| r.state == StepState::Skipped))
            .map(|n| mermaid_id(&n.id)).collect();
        if !skipped.is_empty() {
            out.push_str("    classDef skipped fill:#eeeeee,color:#777777\n");
            out.push_str(&format!("    class {} skipped\n", skipped.join(",")));
        }
        out
    }
}
