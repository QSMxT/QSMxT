# Instructions for coding agents

Read [CONTRIBUTING.md](CONTRIBUTING.md) before making any change, and follow it. In particular:

- Algorithms belong in QSM.rs; QSMxT only wires them. Every option change reaches every surface
  listed under "Change surfaces", and no setting ever changes another one silently.
- Work on a branch in its own git worktree, with its own `CARGO_TARGET_DIR`. Never push to `main`
  and never merge; open a draft PR and leave merging to the maintainer.
- Don't add CHANGELOG or release-notes files; put the summary, the default-settings behaviour
  changes and the per-surface checklist in the PR body.
