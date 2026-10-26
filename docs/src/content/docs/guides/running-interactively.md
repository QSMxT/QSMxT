---
title: Running interactively
description: Convert data, configure, and run the QSM pipeline from QSMxT's interactive terminal interface.
---

The interactive terminal interface (TUI) is the recommended way to use QSMxT. It
takes you through the whole workflow — converting DICOMs, configuring the
pipeline, and running it — without writing a single command flag or config file.

```sh
qsmxt tui
```

## What you can do

- **Convert DICOMs to BIDS** — point at a DICOM directory, review the automatic
  series classification, relabel anything that needs it, and convert. (See
  [DICOM → BIDS](/QSMxT/guides/running-noninteractively/#converting-dicoms-to-bids) for what the classifier handles.)
- **Point at a dataset** and let QSMxT discover the phase/magnitude runs.
- **Download an example dataset** — no data of your own to hand? Set
  **Input mode → Example dataset**, tick one or more real in-vivo acquisitions,
  and press Download. The TUI fetches them, writes a BIDS dataset, and drops you
  straight into BIDS mode pointed at it. (See
  [Fetch an example](/QSMxT/getting-started/quick-start/#no-data-yet-fetch-an-example).)
- **Choose algorithms** for masking, unwrapping, background-field removal, and
  dipole inversion from menus — no need to memorise flag names or valid values.
- **Adjust parameters** for each stage, with sensible defaults pre-filled.
- **Copy the equivalent command** — as you configure, the TUI shows the matching
  `qsmxt run` command and updates it live. It's the easiest way to build a run
  command without memorising flags. The TUI does not write a config file itself,
  but every run (from the TUI or the CLI) records its resolved settings in
  `derivatives/qsmxt/pipeline_config.toml`, which `--config` (or the TUI's
  *Config file* field) loads back.
- **Export results as DICOM** — tick *Export DICOM* under Other outputs → Export to
  write every final map as a DICOM series alongside the NIfTIs, ready for a PACS
  or a clinical viewer. (See
  [Exporting results as DICOM](/QSMxT/guides/running-noninteractively/#exporting-results-as-dicom).)
- **Run the pipeline** and watch progress without leaving the terminal.
- **Submit to a cluster** — generate and launch SLURM jobs for large cohorts
  directly from the TUI; scaling out doesn't require the command line.

## Tips

When the TUI opens it shows a *Did you know?* tip. Press `Enter` on **OK** to close it,
or `n` to see the next tip. To stop the tips, press `→` to select **Don't show tips** and
then `Enter`. Press `?` at any time to see a tip. When tips are off, its **Show tips at
startup** button turns them back on. The setting is saved in `~/.qsmxt/tips.json`. Setting
the `QSMXT_NO_TIPS` environment variable also turns tips off, for example on a shared
install.

## Recommendations

When a change makes a setting depart from what a chosen method recommends (or requires, or
discourages), a pop-up gives the reason and offers **Apply** (the recommended value, selected),
the other values the method allows, and **Keep mine**. `w` shows the source. **Keep mine** keeps
your choice and saves it in the configuration, so the pop-up does not come back until the
setting changes. `Esc` closes the pop-up and leaves the setting marked ⚑.

When you choose a method that has a reference pipeline (for example MEDI, QSMART or iLSQR) and
your settings differ from it, a pop-up titled with that pipeline (*MEDI's reference pipeline*)
lists each setting that differs: your value, the reference value, why the reference uses it,
and the source (the paper, or the file and line in the authors' code). Nothing changes unless
you apply it:

- **Apply all** sets every listed setting to the reference value.
- **Choose per setting** lists the settings again with a choice on each (`↑`/`↓` to move,
  `←`/`→` to choose a value or to keep yours); `Enter` applies them.
- **Keep mine** keeps your values and saves that in the configuration, so the pop-up does not
  come back while the method stays chosen. Choosing the method again after switching away
  shows it again.

The pop-up opens once per method choice. If the same change also creates recommendations, they
are listed first in the same pop-up. If your settings already match the reference pipeline, there is no pop-up.

A setting that departs from a rule is marked with the method whose rule it is and what that
rule uses, for example `ⓘ MEDI: BET`, or `ⓘ MEDI, PDF: BET` when several methods' references
agree. ⚑ marks a recommendation (or something required or discouraged) that is still open; ⓘ
marks a difference from a reference pipeline, or a recommendation you kept. Press `!` on a
marked setting to see its details: whose rule it is, what it uses, why, the source, and
**Apply** with the other values it allows. The line under the help gives each covered setting's
reference value and its source, whether or not the setting differs.

## Layout

The top row holds the tabs; press a digit (`1`–`8`) or `Tab`/`Shift-Tab` to switch. You can
also press `↑` at the top of a tab to put focus on the tab bar (the current tab turns yellow).
`←`/`→` then move between tabs, and `↓` or `Enter` go into the tab. A tab
with more than one page shows a strip of sub-tabs along the top of its panel, and opens with
focus on that strip (the current sub-tab turns yellow): `←`/`→` choose a sub-tab, and `↓` or
`Enter` go into its settings. `↑` from the first setting goes back up to the strip, and `↑`
from the strip goes up to the tab bar. `[` and `]` switch sub-tabs from anywhere on the tab. A
tab without sub-tabs opens on its first setting.

| Tab | Sub-tabs | What is there |
|---|---|---|
| 1 Input | — | Input mode, dataset/output/config paths, include/exclude patterns and the run tree, number of echoes, and the orientation-group pattern with a live preview of the groups it forms |
| 2 Preprocessing | Geometry · Masking · Motion | Axial resampling and output space, inhomogeneity correction, slice-gap check, crop to mask, FFT padding, coil combination (multi-coil data); the mask recipe, custom masks and the two-pass reliable mask; motion correction and its rigid registration |
| 3 QSM | Field map · Background · Inversion · Multi-orientation · Reference | *Compute QSM*, then one page per stage, shown only when that stage runs: unwrapping and B0 estimation; background-field removal (inversions of a local field only); the dipole inversion; how an orientation group is reconstructed (with an orientation group only); the χ reference |
| 4 χ-separation | — | χ-separation method and its parameters |
| 5 Other outputs | SWI/SMWI · Relaxometry · Segmentation · Export | SWI and SMWI; T2\*, R2\*, R2 and R2′ maps, the R2′ source and supplied maps; segmentation (SynthSeg or a dseg from derivatives) and per-structure statistics; DICOM export |
| 6 Execution | — | Local or SLURM, each with its own settings; debug logging, force re-run, clean-up; and a button that does what `F5` does |
| 7 Pipeline | — | The pipeline the current settings run, step by step: each step's algorithm and settings, and the steps it reads from and what it reads. Above it, the peak memory the run is estimated to need and the deep-learning models it uses, with their download sizes. It is planned on the first selected run's scan (a typical scan until one is chosen). `Enter` (or `s`) saves it as an SVG drawing and opens it |
| 8 Methods | — | The methods text for the current settings; `Enter` (or `c`) copies it to the clipboard |

Settings appear only where and when they apply: a sub-setting sits indented under the
checkbox or choice it belongs to, and a map another output needs reads as ticked (and says why
when you try to untick it). `↑`/`↓` move, `Space` ticks a checkbox, `Enter` edits a value, `r`
resets the focused setting, `R` the whole sub-tab, and `F5` runs. A short choice of up to three
values (for example ROMEO or Laplacian unwrapping) shows as `◀ romeo ▶` when focused and changes
with `←`/`→`. A method (the inversion, background removal, χ-separation), or a choice with more
than three values, shows as a button, `[ medi ▾ ]`: `Enter` (or `←`/`→`) opens the list to choose
from, so you never pass through other methods on the way. The help line and the key hints at
the bottom say which a setting is ("Enter: choose method"). The button at the bottom of the Execution tab runs too, and its label says what will
happen: *Run*, *Dry run*, *Write SLURM scripts* or *Submit SLURM jobs*.

| Key | Where | Does |
|---|---|---|
| `1`–`8`, `Tab`, `Shift-Tab` | anywhere | switch tab |
| `←` `→` | tab bar | previous / next tab |
| `↓`, `Enter` | tab bar | go into the tab |
| `←` `→` | sub-tab strip | previous / next sub-tab |
| `↓`, `Enter` | sub-tab strip | go into the sub-tab's settings |
| `[` `]` | any tab with sub-tabs | previous / next sub-tab |
| `↑` | sub-tab strip | go up to the tab bar |
| `↑` `↓` | settings | move; `↑` from the first setting returns to the strip, or to the tab bar on a tab without sub-tabs |
| `←` `→` | settings | change a short choice (`◀ … ▶`); on a button (`[ … ▾ ]`), open its list |
| `Space` | settings | tick a checkbox |
| `Enter` | settings | open a button's list or edit a value; on the Execution tab's button, run |
| `r` / `R` | settings | reset the setting / the whole sub-tab |
| `Enter`, `s` | Pipeline | save the pipeline as an SVG drawing and open it |
| `Enter`, `c`, `y` | Methods | copy the methods text to the clipboard |
| `F5` | anywhere | run |
| `?` | anywhere | show a tip (and turn startup tips on or off) |
| `q`, `Esc` | anywhere | quit |

Copying goes through the terminal (the OSC 52 escape sequence), so it reaches the clipboard of
the machine you are typing on even when QSMxT runs on a remote server over SSH. Most current
terminals support it (iTerm2, kitty, WezTerm, Alacritty, foot, Windows Terminal; xterm with
`allowWindowOps`). Inside tmux, set `set -g set-clipboard on`. If the clipboard stays empty,
select the text in the Methods tab with the mouse instead.

The Pipeline tab's drawing is written to the system's temporary folder. Over SSH, or with no
display, QSMxT does not try to open it and shows where it was written instead, so you can copy
it to your own computer. A run writes the same drawing next to its results (`pipeline.svg`).

The TUI builds the same configuration the CLI consumes, so anything you set up
interactively can be reproduced [noninteractively](/QSMxT/guides/running-noninteractively/)
with `qsmxt run --config` — or by copying the generated command.

:::note
The TUI is ideal for exploring a new dataset and dialling in parameters. Once
you're happy, capture the configuration to a TOML file and reuse it
[noninteractively](/QSMxT/guides/running-noninteractively/) for scripted batch
processing.
:::
