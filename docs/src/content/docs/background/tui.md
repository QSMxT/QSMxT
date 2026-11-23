---
title: Using the TUI
description: "QSMxT's interactive terminal interface: what it does, its layout, keys and mouse."
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
  [DICOM → BIDS](/QSMxT/how-to/convert-dicom/#converting-dicoms-to-bids) for what the classifier handles.)
  Tick *Keep DICOM UIDs* above *Convert to BIDS* to keep the study's UIDs for a
  later DICOM export ([not for sharing](/QSMxT/how-to/convert-dicom/#keeping-the-studys-dicom-uids)).
- **Point at a dataset** and let QSMxT discover the phase/magnitude runs.
- **Download an example dataset** — no data of your own to hand? Set
  **Input mode → Example dataset**, tick one or more real in-vivo acquisitions,
  and press Download. The TUI fetches them, writes a BIDS dataset, and drops you
  straight into BIDS mode pointed at it. (See
  [Fetch an example](/QSMxT/start/tutorial-tui/#no-data-yet-fetch-an-example).)
- **Start from a pipeline** — QSMxT's defaults, or an original toolbox's whole
  pipeline (the MEDI toolbox, STI Suite, QSMART, ...) on the Pipeline tab. (See
  [Pipelines](/QSMxT/toolbox-pipelines/#pipelines).)
- **Choose algorithms** for masking, unwrapping, background-field removal, and
  dipole inversion from menus — no need to memorise flag names or valid values.
- **Adjust parameters** for each stage, with sensible defaults pre-filled.
- **Copy the equivalent command** — as you configure, the TUI shows the matching
  `qsmxt run` command and updates it live. It's the easiest way to build a run
  command without memorising flags. The TUI does not write a config file itself,
  but every run (from the TUI or the CLI) records its resolved settings in
  `derivatives/qsmxt/pipeline_config.toml`, which `--config` (or the TUI's
  *Config file* field) loads back.
- **Export results as DICOM** — tick *Export DICOM* in the Reconstruction tab's
  DICOM section, and the maps to export, to write them as DICOM series alongside
  the NIfTIs, ready for a PACS or a clinical viewer. (See
  [Exporting results as DICOM](/QSMxT/how-to/export-dicom/#exporting-results-as-dicom).)
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

## Basic and Advanced settings

The Reconstruction tab starts in **Basic** mode: each section shows the choices most people make
(whether a step runs, its method and variant, inputs you supply, outputs) and hides the
parameters that tune a method, such as iterations, tolerances and weights. The last row of a
section says how many are hidden (`+ 15 advanced settings · A`). Press `A`, or `Enter` on that
row, for **Advanced** mode, which shows every setting; `A` again goes back. QSMxT remembers your
choice.

Basic mode never hides a setting that matters to your run. A setting shows whenever it differs
from QSMxT's default (or, when you chose a pipeline on the Pipeline tab, from that pipeline's
value), has a value that is not allowed, or has a recommendation still open. The mask recipe
shows as one line until you edit it. The command preview and the methods text always cover
every setting.

## Layout

The top row holds the tabs; press a digit (`1`–`5`) or `Tab`/`Shift-Tab` to switch. You can
also press `↑` at the top of a tab to put focus on the tab bar (the current tab turns yellow).
`←`/`→` then move between tabs, and `↓` or `Enter` go into the tab.

The Reconstruction tab holds every setting of the run, in sections listed in a sidebar on its
left and grouped as the run goes: *Preprocessing* (Geometry, Masking, Motion), *QSM* (Compute
QSM, Field map, Background, Inversion, COSMOS/STI, Reference, and the outputs made from χ:
χ-separation and SMWI), *Other outputs* (Magnitude, SWI, Relaxometry, Segmentation) and *Export* (DICOM). It opens on Masking, with focus in the sidebar
(the current section turns yellow): `↑`/`↓` choose a section, and `→` or `Enter` go into its
settings; `←` from any setting (or `↑` from the first) goes back to the sidebar on the same
section, and `↑` from the first section to the tab bar. `[` and `]` switch sections from anywhere on the tab. A mouse click on a section
opens it, and on a group's heading opens the group's first section.

The sidebar lists every section whatever the settings, and marks how each stands: `off` (dimmed)
when its output is not made, such as χ-separation until it is ticked (the QSM heading reads `off`
when QSM is); `–` (dimmed) when the settings skip its stage, such as background removal for an
inversion that takes the total field, which the section then explains; `⚑` when a setting in it
has advice offered as a change, and `ⓘ` for information only (see [Recommendations](/QSMxT/background/recommendations/#recommendations)). On a short
terminal the sidebar drops its blank lines, then turns its headings into rules, then scrolls
around the current section (`▲`/`▼`). Narrower than 78 columns, the sections become a strip of
sub-tabs along the top of the panel instead, chosen with `←`/`→`, with `↓` going into them and
`←` from a setting coming back to them. The
Methods tab's two pages, Methods text and Graph, are a sidebar too (a strip when narrow), and the
Pipeline tab's list of pipelines is drawn the same way.

The help for the highlighted setting sits under the settings. From 138 columns wide, it moves to
a column on the right instead, so the settings use the panel's full height; the settings keep
76 columns and the help takes up to 64. Notes in a section wrap to the width they are given.

| Tab | Pages | What is there |
|---|---|---|
| 1 Input | — | Input mode, dataset/output/config paths, include/exclude patterns and the run tree, number of echoes, and the orientation groups: a pattern, or groups given to runs with ←/→ in the run tree (one of them the reference), each run marked with its group |
| 2 Pipeline | — | The pipeline to start from: QSMxT's defaults or an original toolbox's, and what choosing it changes (see [Pipelines](/QSMxT/toolbox-pipelines/#pipelines)). *About*, at the top of the list, says what the toolbox pipelines are |
| 3 Reconstruction | Preprocessing: Geometry · Masking · Motion | Axial resampling and output space, slice-gap check, crop to mask, FFT padding, coil combination (multi-coil data); the mask recipe, custom masks, the magnitude's inhomogeneity correction and the two-pass reliable mask; motion correction and its rigid registration |
| | QSM: Compute QSM · Field map · Background · Inversion · COSMOS/STI · Reference · χ-separation · SMWI | Whether QSM is reconstructed (SMWI turns it back on and χ-separation needs it, unless a QSM is supplied); unwrapping and B0 estimation; background-field removal (inversions of a local field only); the dipole inversion; how an orientation group is reconstructed (without one, where to choose it); the χ reference; the χ-separation method and its parameters; SMWI |
| | Other outputs: Magnitude · SWI · Relaxometry · Segmentation | whether the combined magnitude is published; SWI; T2\*, R2\*, R2 and R2′ maps, the R2′ source and supplied maps; segmentation (SynthSeg or a dseg from derivatives) and per-structure statistics |
| | Export: DICOM | DICOM export: the source DICOM to inherit the study from, and a tick for each map the settings make (all ticked to begin with; a map turned on later is ticked too; `a` ticks them all) |
| 4 Execution | — | Local or SLURM, each with its own settings; debug logging, force re-run, clean-up; and a button that does what `F5` does |
| 5 Methods | Methods text · Graph | The methods text for the current settings, under a *Copy methods text* button that copies it to the clipboard (`c`, or `Enter` inside the page). Graph: the pipeline the current settings run, step by step: each step's algorithm and settings, and the steps it reads from and what it reads. Above it, the peak memory the run is estimated to need and the deep-learning models it uses, with their download sizes. It is planned on the first selected run's scan (a typical scan until one is chosen). The *Open graph* button above it (`s`, or `Enter` inside the page) saves it as an SVG drawing and opens it |

Settings appear only where and when they apply: a sub-setting sits indented under the
checkbox or choice it belongs to, and a map another output needs reads as ticked (and says why
when you try to untick it). `↑`/`↓` move, `Space` ticks a checkbox, `Enter` edits a value, `r`
resets the focused setting, `R` the whole section, and `F5` runs. No setting changes with
`←`/`→`: every choice (a method, ROMEO or Laplacian unwrapping, a mask's algorithm, how its
sections combine) shows as a button, `[ medi ▾ ]`, and `Enter` opens its list, with the current
value highlighted; `Enter` chooses, `Esc` closes it with nothing changed. A mask's numbers (BET's
fractional intensity, a refinement step's value) are typed like any other value (`Enter`), and
`+`/`-` nudge them by a step. The help line and the key hints at the bottom say what `Enter` does
("Enter: choose method"). The button at the bottom of the Execution tab runs too, and its label says what will
happen: *Run*, *Dry run*, *Write SLURM scripts* or *Submit SLURM jobs*.

| Key | Where | Does |
|---|---|---|
| `1`–`5`, `Tab`, `Shift-Tab` | anywhere | switch tab |
| `←` `→` | tab bar | previous / next tab |
| `↓`, `Enter` | tab bar | go into the tab |
| `↑` `↓`, `Home` `End` | sidebar (Reconstruction, Methods) | previous / next section, first / last (Reconstruction); `↑` from the first goes up to the tab bar |
| `→`, `Enter` | sidebar | go into the section or page |
| `←` `→` | sub-tab strip (when narrow) | previous / next page |
| `←` | a section's settings, a Methods page | back to the sidebar (or the strip), on the same section |
| `↓`, `Enter` | sub-tab strip | go into the page |
| `↑` | sub-tab strip | go up to the tab bar |
| `[` `]` | any tab with sections or sub-tabs | previous / next one |
| `↑` `↓` | settings | move; `↑` from the first setting returns to the sidebar or strip, or to the tab bar on a tab without either |
| `←` `→` | editing a value | move the cursor |
| `Space` | settings | tick a checkbox |
| `Enter` | settings | open a button's list (`[ … ▾ ]`) or edit a value; on the Execution tab's button, run |
| `+` `-` | Reconstruction › Masking | nudge a mask number (fractional intensity, a step's value) |
| `r` / `R` | settings | reset the setting / the whole section |
| `a` | Reconstruction › DICOM | tick every map to export |
| `A` | Reconstruction | show every setting (Advanced) or the basic ones (Basic) |
| `P` | settings | go to the Pipeline tab |
| `↑` `↓`, `Enter` | Pipeline | choose a pipeline; preview its changes, then Apply or Cancel |
| `s` (`Enter` inside the page) | Methods › Graph | save the pipeline as an SVG drawing and open it (the *Open graph* button) |
| `c`, `y` (`Enter` inside the page) | Methods › Methods text | copy the methods text to the clipboard (the *Copy methods text* button) |
| `F5` | anywhere | run |
| `?` | anywhere | show a tip (and turn startup tips on or off) |
| `n` | tab bar | read the announcements |
| `q`, `Esc` | anywhere | quit |

Copying goes through the terminal (the OSC 52 escape sequence), so it reaches the clipboard of
the machine you are typing on even when QSMxT runs on a remote server over SSH. Most current
terminals support it (iTerm2, kitty, WezTerm, Alacritty, foot, Windows Terminal; xterm with
`allowWindowOps`). Inside tmux, set `set -g set-clipboard on`. If the clipboard stays empty,
select the text in the Methods tab with the mouse instead, holding Shift (Option in iTerm2) while
you drag, since the TUI uses the mouse itself (see [Mouse](#mouse)).

## Mouse

Everything can also be done with the mouse:

- **Click a tab, a section in the sidebar, or a sub-tab** to switch to it. A group's heading
  in the sidebar opens its first section.
- **Click a setting** to use it: a checkbox ticks, a button (`[ … ▾ ]`) opens its list, and a
  value opens for editing. Any other row, such as *Add section* or the Execution tab's run
  button, is focused by the first click and pressed by the second.
- **In a list** (a method's list, the Pipeline tab's pipelines), the first click highlights an
  entry and the second chooses it.
- **Click the Methods tab's button** (*Copy methods text*, *Open graph*) to press it. A click
  anywhere on a Methods page goes into it.
- **Click a pop-up's button** to press it. A click outside an open pop-up closes it, as `Esc`
  does, and does nothing else.
- **The wheel** scrolls what is under the mouse. Over the methods text or the graph it scrolls
  them wherever focus is, even in the sidebar. Over a section's settings while focus is in the
  sidebar (or the sub-tabs), it scrolls the settings and focus stays where it is. Elsewhere it does
  what `↑`/`↓` do: it moves through the settings and lists, and scrolls the pop-ups that scroll.
  It stays on the page rather than moving up to the sidebar, the sub-tabs or the tabs.

With the mouse in use, most terminals select text only while you hold Shift (Option in iTerm2).
To leave the mouse to the terminal entirely, set `QSMXT_NO_MOUSE=1`.

The Methods › Graph drawing is written to the system's temporary folder. Over SSH, or with no
display, QSMxT does not try to open it and shows where it was written instead, so you can copy
it to your own computer. A run writes the same drawing next to its results (`pipeline.svg`).

The TUI builds the same configuration the CLI consumes, so anything you set up
interactively can be reproduced [noninteractively](/QSMxT/how-to/run-bids/)
with `qsmxt run --config` — or by copying the generated command.

:::note
The TUI is ideal for exploring a new dataset and dialling in parameters. Once
you're happy, capture the configuration to a TOML file and reuse it
[noninteractively](/QSMxT/how-to/run-bids/) for scripted batch
processing.
:::
