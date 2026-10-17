# G3 status (agent restart notes)
Task: follow-up to QSMxT #382 (G2). Branch engine-g3 from 69d0b04 (rebase onto main once #382 squash-merges; patch-id check).
1. SVG of pipeline graph (Rust, engine): pipeline_graph/layout.rs (port of qsmbly js/next/graph-layout.js) + svg.rs; qsmxt writes .svg beside each .mmd
2. Dataset methods.md + pipeline graph from merged run records (engine dataset.rs: summarise_run / summarise); changes/ rebuilt in run_dataset_steps from that
3. --dry prints planned methods + graph (writes nothing: dry runs must not create files in the dataset)
## Done
## Todo
- all
## State
