# G3 status (agent restart notes)
Task: follow-up to QSMxT #382 (G2). Branch engine-g3 from 69d0b04 (rebase onto main once #382 squash-merges; patch-id check).
1. SVG of pipeline graph (Rust, engine): pipeline_graph/layout.rs (port of qsmbly js/next/graph-layout.js) + svg.rs; qsmxt writes .svg beside each .mmd
2. Dataset methods.md + pipeline graph from merged run records (engine dataset.rs: summarise_run / summarise); changes/ rebuilt in run_dataset_steps from that
3. --dry prints planned methods + graph (writes nothing: dry runs must not create files in the dataset)
## Done
- engine: pipeline_graph/layout.rs (layout identical to qsmbly JS on 4 graphs, checked with node: /scratch/user/uqaste15/src/g3-parity/parity.mjs), svg.rs, Node.dataset/NodeDataset, pipeline_graph_dataset, to_text, dataset.rs (summarise_run/summarise; "changed" = exception among runs: text != most common text, or as-ran shape != most common of runs planned alike)
- qsmxt: write_graph (.mmd+.svg), write_run_files via summarise_run (no changes/ there), write_dataset_summary in run_dataset_steps (methods.md, pipeline.mmd/svg, changes/ rebuilt), --dry prints graph text + methods; docs/README/bidsignore/cli.md
- target dir /scratch/user/uqaste15/src/g3-target
## Todo
- verify: /scratch/user/uqaste15/src/g3-verify/run.sh single|multi (bin/qsmxt-g3 = release of this branch); compare with g2-verify/out/<case>-g2
- full tests/clippy, PR
## State
