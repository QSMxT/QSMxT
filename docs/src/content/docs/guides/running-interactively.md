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

## Recommendations

When a change makes a setting depart from what a chosen method recommends (or requires, or
discourages), a pop-up gives the reason and offers **Apply** (the recommended value, selected),
the other values the method allows, and **Keep mine**. `w` shows the source. **Keep mine** keeps
your choice and saves it in the configuration, so the pop-up does not come back until the
setting changes. `Esc` closes the pop-up and leaves the setting marked ⚑.

Suggestions come in two kinds, labelled differently:

- **Recommended** (also *Required* or *Discouraged*): there is a stated reason another value is
  better. The pop-up gives the reason, and the recommended value is preselected.
- **Original authors' setting**: what the method's authors used, with no reason given that it is
  better. It is shown so that you can reproduce their pipeline, but it is not a recommendation,
  so its value is never preselected: **Keep mine** is.

When you choose a method that has a reference pipeline (for example MEDI, QSMART or iLSQR) and
your settings differ from it, a pop-up titled with that pipeline (*MEDI's reference pipeline*)
lists each setting that differs: your value, the authors' value, why they use it, and the source
(the paper, or the file and line in the authors' code). If the same change also creates
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

A setting that departs from a rule is marked with the method whose rule it is and what that
rule uses, for example `ⓘ MEDI: BET`, or `ⓘ MEDI, PDF: BET` when several methods' references
agree. ⚑ marks a recommendation (or something required or discouraged) that is still open; ⓘ
marks a difference from an original authors' setting, or a recommendation you kept. Press `!` on a
marked setting (the row and the key bar say so when it is highlighted) to see its advice: whose
rule it is, what it uses, why, the source, and **Apply** with the other values it allows. A
recommendation's value is preselected, and for an authors' setting **Keep mine** is. The line under
the help gives each covered setting's reference value and its source, whether or not the setting
differs.

## Layout

The top row holds the tabs; press a digit (`1`–`8`) or `Tab`/`Shift-Tab` to switch. You can
also press `↑` at the top of a tab to put focus on the tab bar (the current tab turns yellow).
`←`/`→` then move between tabs, and `↓` or `Enter` go into the tab. A tab
with more than one page shows a strip of sub-tabs along the top of its panel, and opens with
focus on that strip (the current sub-tab turns yellow): `←`/`→` choose a sub-tab, and `↓` or
`Enter` go into its settings. Preprocessing opens on Masking; the others open on their first
sub-tab. `↑` from the first setting goes back up to the strip, and `↑`
from the strip goes up to the tab bar. `[` and `]` switch sub-tabs from anywhere on the tab. A
tab without sub-tabs opens on its first setting.

| Tab | Sub-tabs | What is there |
|---|---|---|
| 1 Input | — | Input mode, dataset/output/config paths, include/exclude patterns and the run tree, number of echoes, and the orientation groups: a pattern, or groups given to runs with ←/→ in the run tree (one of them the reference), each run marked with its group |
| 2 Pipeline | — | The pipeline to start from: QSMxT's defaults or an original toolbox's, and what choosing it changes (see [Pipelines](#pipelines)) |
| 3 Preprocessing | Geometry · Masking · Motion | Axial resampling and output space, inhomogeneity correction, slice-gap check, crop to mask, FFT padding, coil combination (multi-coil data); the mask recipe, custom masks and the two-pass reliable mask; motion correction and its rigid registration |
| 4 QSM | Field map · Background · Inversion · COSMOS/STI · Reference | *Compute QSM*, then one page per stage, shown only when that stage runs: unwrapping and B0 estimation; background-field removal (inversions of a local field only); the dipole inversion; how an orientation group is reconstructed (without one, where to choose it); the χ reference |
| 5 χ-separation | — | χ-separation method and its parameters |
| 6 Other outputs | SWI/SMWI · Relaxometry · Segmentation · Export | SWI and SMWI; T2\*, R2\*, R2 and R2′ maps, the R2′ source and supplied maps; segmentation (SynthSeg or a dseg from derivatives) and per-structure statistics; DICOM export |
| 7 Execution | — | Local or SLURM, each with its own settings; debug logging, force re-run, clean-up; and a button that does what `F5` does |
| 8 Methods | Methods text · Graph | The methods text for the current settings; `Enter` (or `c`) copies it to the clipboard. Graph: the pipeline the current settings run, step by step: each step's algorithm and settings, and the steps it reads from and what it reads. Above it, the peak memory the run is estimated to need and the deep-learning models it uses, with their download sizes. It is planned on the first selected run's scan (a typical scan until one is chosen). `Enter` (or `s`) saves it as an SVG drawing and opens it |

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

- **Click a tab or a sub-tab** to switch to it.
- **Click a setting** to use it: a checkbox ticks, a button (`[ … ▾ ]`) opens its list, and a
  value opens for editing. A short choice (`◀ … ▶`) is focused by the first click and takes the
  next value on the next. Any other row, such as *Add section* or the Execution tab's run
  button, is focused by the first click and pressed by the second.
- **In a list** (a method's list, the Pipeline tab's pipelines), the first click highlights an
  entry and the second chooses it.
- **Click a pop-up's button** to press it. While a pop-up is open, the rest of the screen does
  not answer clicks, just as it does not answer keys.
- **The wheel** does what `↑`/`↓` do: it moves through the settings and lists, and scrolls the
  methods text, the graph and the pop-ups that scroll. It stays on the page rather than moving up
  to the sub-tabs or the tabs.

With the mouse in use, most terminals select text only while you hold Shift (Option in iTerm2).
To leave the mouse to the terminal entirely, set `QSMXT_NO_MOUSE=1`.

The Pipeline sub-tab's drawing is written to the system's temporary folder. Over SSH, or with no
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
