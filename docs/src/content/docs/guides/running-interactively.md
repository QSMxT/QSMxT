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
  Tick *Keep DICOM UIDs* above *Convert to BIDS* to keep the study's UIDs for a
  later DICOM export ([not for sharing](/QSMxT/guides/running-noninteractively/#keeping-the-studys-dicom-uids)).
- **Point at a dataset** and let QSMxT discover the phase/magnitude runs.
- **Download an example dataset** — no data of your own to hand? Set
  **Input mode → Example dataset**, tick one or more real in-vivo acquisitions,
  and press Download. The TUI fetches them, writes a BIDS dataset, and drops you
  straight into BIDS mode pointed at it. (See
  [Fetch an example](/QSMxT/getting-started/quick-start/#no-data-yet-fetch-an-example).)
- **Start from a pipeline** — QSMxT's defaults, or an original toolbox's whole
  pipeline (the MEDI toolbox, STI Suite, QSMART, ...) on the Pipeline tab. (See
  [Pipelines](#pipelines).)
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

## Updates and announcements

When a newer release is available, the TUI says so in a pop-up when it starts: `Enter` updates
QSMxT and `Esc` closes the pop-up until the next start. Announcements from the QSMxT team (for
example a problem found in a release, and the version that fixes it) pop up once; one about a
problem that can give wrong results pops up each time the TUI starts until you update past it.
Only one pop-up opens at a time: important announcements first, then the update, then other
announcements. In an announcement, `↑`/`↓` scroll, `c` copies its link and `Enter` closes it.
Press `n` on the tab bar to read every announcement, newest first, or run `qsmxt news`. Both
checks run in the background and never delay startup; without an internet connection the
announcements saved last time are shown. See
[Keeping up to date](/QSMxT/getting-started/installation/#keeping-up-to-date).

## Pipelines

The Pipeline tab lists QSMxT's defaults and the original toolboxes' pipelines: the MEDI
toolbox, STI Suite (iLSQR and STAR-QSM), QSMART, the HEIDI package, the χ-separation toolbox
(iLSQR, MEDI and χ-sepnet) and DECOMPOSE. Next to the list, the highlighted pipeline is
compared with your settings: each setting choosing it would change, from your value to the
pipeline's.

`Enter` opens a preview of those changes. **Apply** makes them; `Esc` or **Cancel** changes
nothing. A toolbox pipeline is its method and the original authors' settings that method
brings with it (the same ones its reference pipeline pop-up offers, see
[Recommendations](#recommendations)), so choosing the MEDI toolbox sets MEDI with BET, the
complex fit, region-growing unwrapping and PDF. The stages it covers start from QSMxT's
defaults first, so a value you changed there earlier is reset too; every other setting stays as
it is. A reasoned recommendation is not part of a pipeline: it is shown in the preview and
offered as usual afterwards.

The header shows the pipeline your settings follow, on every tab: `Pipeline: MEDI toolbox`.
Change a setting afterwards and it reads `MEDI toolbox · 1 departure`; change the method itself
and it reads `Custom`. Press `P` on any settings page to come back to the Pipeline tab. The
command preview starts with `--pipeline medi-toolbox`, followed only by the flags where your
settings depart from it.

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

## Recommendations

When a change makes a setting depart from what a chosen method recommends (or requires, or
discourages), a pop-up gives the reason and offers **Apply** (the recommended value, selected),
the other values the method allows, and **Keep mine**. **Keep mine** keeps
your choice and saves it in the configuration, so the pop-up does not come back until the
setting changes. `Esc` closes the pop-up and leaves the setting marked ⚑.

Suggestions come in two kinds, labelled differently:

- **Recommended** (also *Required* or *Discouraged*): there is a stated reason another value is
  better. The pop-up gives the reason, and the recommended value is preselected.
- **Suggested**: measured to be better on some data but not all (for example in vivo, but not
  on the simulated head), such as NDI's in-mask weight normaliser. The pop-up gives the reason,
  but **Keep mine** is preselected.
- **Original authors' setting**: what the method's authors used, with no reason given that it is
  better. It is shown so that you can reproduce their pipeline, but it is not a recommendation,
  so its value is never preselected: **Keep mine** is.

When you choose a method that has a reference pipeline (for example MEDI, QSMART or iLSQR) and
your settings differ from it, a pop-up titled with that pipeline (*MEDI's reference pipeline*)
lists each setting that differs: your value, the authors' value, and why they use it. If the same change also creates
recommendations, they are listed first. Nothing changes unless you apply it:

- **Apply recommended** (preselected when there are recommendations) applies the
  recommendations and keeps your values where only the authors' setting differs.
- **Apply all, with the authors' settings** (**Apply the authors' settings** when there are no
  recommendations) also sets every authors' setting. A recommendation wins over an authors'
  setting on the same setting, and two authors' settings that disagree leave that setting as it
  is, marked, for you to choose.
- **Choose per setting** lists the settings again with a choice on each (`↑`/`↓` to move,
  `←`/`→` to choose a value or to keep yours); `Enter` applies them. Recommendations start on
  the recommended value, authors' settings on keeping yours.
- **Keep mine** keeps your values and closes the pop-up. An authors' setting has nothing to
  dismiss, so nothing is saved; the pop-up comes back only when you choose the method again
  after switching away.

When the pop-up holds only authors' settings, **Keep mine** is preselected: press `←`/`→` to pick
another button. The pop-up opens once per method choice. If your settings already match the
reference pipeline, there is no pop-up. A pop-up too long for the terminal scrolls above its
buttons, with a scrollbar: `↑`/`↓` or `PgUp`/`PgDn`.

A setting that departs from a rule is marked with the method whose rule it is and, when that
takes only a few words, what the rule uses: for example `ⓘ MEDI: BET`, or `ⓘ MEDI, PDF: BET`
when several methods' references agree. A longer one shows only the method, such as `ⓘ QSMART`
beside a mask that differs from QSMART's in several ways. ⚑ marks a recommendation (or something required or discouraged) that is still open; ⓘ
marks a difference from an original authors' setting, or a recommendation you kept. Press `!` on a
marked setting (the row and the key bar say so when it is highlighted) to see its advice: whose
rule it is, what it uses, why, and **Apply** with the other values it allows. A
recommendation's value is preselected, and for an authors' setting **Keep mine** is. The line under
the help gives each covered setting's reference value and whose it is, whether or not the setting
differs.

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
has advice offered as a change, and `ⓘ` for information only (see [Recommendations](#recommendations)). On a short
terminal the sidebar drops its blank lines, then turns its headings into rules, then scrolls
around the current section (`▲`/`▼`). Narrower than 78 columns, the sections become a strip of
sub-tabs along the top of the panel instead, chosen with `←`/`→`, with `↓` going into them and
`←` from a setting coming back to them. The
Methods tab's two pages, Methods text and Graph, are a sidebar too (a strip when narrow), and the
Pipeline tab's list of pipelines is drawn the same way.

| Tab | Pages | What is there |
|---|---|---|
| 1 Input | — | Input mode, dataset/output/config paths, include/exclude patterns and the run tree, number of echoes, and the orientation groups: a pattern, or groups given to runs with ←/→ in the run tree (one of them the reference), each run marked with its group |
| 2 Pipeline | — | The pipeline to start from: QSMxT's defaults or an original toolbox's, and what choosing it changes (see [Pipelines](#pipelines)) |
| 3 Reconstruction | Preprocessing: Geometry · Masking · Motion | Axial resampling and output space, slice-gap check, crop to mask, FFT padding, coil combination (multi-coil data); the mask recipe, custom masks, the magnitude's inhomogeneity correction and the two-pass reliable mask; motion correction and its rigid registration |
| | QSM: Compute QSM · Field map · Background · Inversion · COSMOS/STI · Reference · χ-separation · SMWI | Whether QSM is reconstructed (SMWI turns it back on and χ-separation needs it, unless a QSM is supplied); unwrapping and B0 estimation; background-field removal (inversions of a local field only); the dipole inversion; how an orientation group is reconstructed (without one, where to choose it); the χ reference; the χ-separation method and its parameters; SMWI |
| | Other outputs: Magnitude · SWI · Relaxometry · Segmentation | whether the combined magnitude is published; SWI; T2\*, R2\*, R2 and R2′ maps, the R2′ source and supplied maps; segmentation (SynthSeg or a dseg from derivatives) and per-structure statistics |
| | Export: DICOM | DICOM export: the source DICOM to inherit the study from, and a tick for each map the settings make (all ticked to begin with; a map turned on later is ticked too; `a` ticks them all) |
| 4 Execution | — | Local or SLURM, each with its own settings; debug logging, force re-run, clean-up; and a button that does what `F5` does |
| 5 Methods | Methods text · Graph | The methods text for the current settings; `Enter` (or `c`) copies it to the clipboard. Graph: the pipeline the current settings run, step by step: each step's algorithm and settings, and the steps it reads from and what it reads. Above it, the peak memory the run is estimated to need and the deep-learning models it uses, with their download sizes. It is planned on the first selected run's scan (a typical scan until one is chosen). `Enter` (or `s`) saves it as an SVG drawing and opens it |

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
| `Enter`, `s` | Methods › Graph | save the pipeline as an SVG drawing and open it |
| `Enter`, `c`, `y` | Methods › Methods text | copy the methods text to the clipboard |
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
- **Click a pop-up's button** to press it. A click outside an open pop-up closes it, as `Esc`
  does, and does nothing else.
- **The wheel** does what `↑`/`↓` do: it moves through the settings and lists, and scrolls the
  methods text, the graph and the pop-ups that scroll. It stays on the page rather than moving up
  to the sidebar, the sub-tabs or the tabs.

With the mouse in use, most terminals select text only while you hold Shift (Option in iTerm2).
To leave the mouse to the terminal entirely, set `QSMXT_NO_MOUSE=1`.

The Methods › Graph drawing is written to the system's temporary folder. Over SSH, or with no
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
