---
title: Recommendations and reference settings
description: "Recommendations, reference settings and required changes, in the TUI and on the command line."
---

Many methods only work as intended with certain other settings: the ones their authors used.
QSMxT checks the configuration against what each chosen method's reference pipeline does (the
pipeline rules in QSM.rs, each with its source) and says where it departs. It never changes a
setting on its own.

- **Required**: the method cannot run otherwise (MEDI-mSMV on fewer than three echoes). The
  run is refused, with the change that resolves it.
- **Recommended** or **discouraged**: there is a stated reason another value is better. The
  authors give it, QSM.rs measured it, or the combination is known to fail. Each is logged as
  a warning before the runs start, a dismissed one too (marked "dismissed, kept").
  `--accept-recommendations` applies them.
- **Suggested**: measured to be better on some data but not all (for example in vivo, but not on
  the simulated head). Logged with its reason like a recommendation, but `--accept-recommendations`
  leaves it, and it is not listed in the methods Note. Apply it by setting the value yourself.
- **Original authors' setting**: the method's authors used another value (their code, paper or
  README does it), and no reason is given that it is better. It is not a recommendation. It is
  logged as information, listed in the methods Note, and applied only with
  `--accept-reference-settings`. It has nothing to dismiss (`--dismiss-recommendation` refuses
  it).

## In the TUI

When a change makes a setting depart from what a chosen method recommends (or requires, or
discourages), a pop-up lists the changes on offer, under the section each setting is on: the
setting, its value now → the value offered, and what kind of advice it is. Each has a tick box.
`↑`/`↓` move between them, `Space` or `Enter` (or a click) ticks one, and the panel under the
list gives the reason for the highlighted one. **Apply** (`A`, or `↓` past the last row or `Tab`
to the buttons, then `Enter`) makes the ticked changes and does nothing while none is ticked.
**Keep mine** (`K`) keeps your settings, closes the pop-up and saves that choice in the
configuration, so the pop-up does not come back until the setting changes. `Esc` closes the
pop-up and leaves the setting marked ⚑.

What starts ticked depends on the kind of advice:

- **Recommended** (also *required* or *discouraged*): there is a stated reason another value is
  better, and it starts ticked. A required change that you untick means the run will not start;
  the panel says so.
- **Suggested**: better on some data but not all (for example in vivo, but not on the simulated
  head), such as NDI's in-mask weight normaliser. It starts unticked.
- **Reference**: what the method's authors used, with no reason given that it is better. It is
  offered so that you can reproduce their pipeline, but it is not a recommendation, so it starts
  unticked.

Where there are several ways to resolve one setting (values that disagree, or turning off the
box the advice depends on instead), they show as one choice, `( )`, and ticking one unticks the
others; where advice on one setting disagrees, nothing starts ticked. When the setting you just
changed goes against a box that is ticked (choosing QSMART with HEIDI on), the pop-up offers to
turn that box off, ticked, so your change stands; it does not offer to change your choice back.

When you choose a method that has a reference pipeline (for example MEDI, QSMART or iLSQR) and
your settings differ from it, a pop-up titled with that pipeline (*MEDI's reference pipeline*)
lists each setting that differs in the same way, with any recommendations the same change
created first. Nothing changes unless you tick it and apply it. A recommendation wins over an
authors' setting on the same setting, and two authors' settings that disagree leave that
setting as it is, marked, for you to choose. Keep mine records nothing for an authors' setting;
the pop-up comes back only when you choose the method again after switching away. If your
settings already match the reference pipeline, there is no pop-up. A list too long for the
terminal scrolls, with a scrollbar.

A setting that departs from a rule is marked with the method whose rule it is and, when that
takes only a few words, what the rule uses: for example `ⓘ MEDI: BET`, or `ⓘ MEDI, PDF: BET`
when several methods' references agree. A longer one shows only the method, such as `ⓘ QSMART`
beside a mask that differs from QSMART's in several ways. ⚑ marks a recommendation (or something required or discouraged) that is still open; ⓘ
marks a difference from an original authors' setting, or a recommendation you kept. Press `!` on a
marked setting (the row and the key bar say so when it is highlighted) to see its advice: whose
rule it is, what it uses and why, in the same checklist, with the other values the rule allows.
A recommendation starts ticked, and an authors' setting does not. The line under
the help gives each covered setting's reference value and whose it is, whether or not the setting
differs.

## On the command line

```sh
# List every recommendation for this configuration and dataset, and each covered
# setting's reference value
qsmxt run study/bids --qsm-algorithm medi --dry

# Apply every recommended value that is not dismissed (the authors' settings are left as they are)
qsmxt run study/bids --qsm-algorithm medi --accept-recommendations

# Also follow the chosen methods' original settings (here MEDI's PDF, BET and complex fit)
qsmxt run study/bids --qsm-algorithm medi --accept-recommendations --accept-reference-settings

# Keep a choice: dismiss a recommended or discouraged finding for the setting's current value
qsmxt run study/bids --qsm-algorithm fansi --fansi-te first \
  --dismiss-recommendation fansi.te-spacing=first
```

`--dry` with either flag shows the settings that would change and the resulting command. Two
equally strong suggestions that disagree on one setting are left for you to choose, and a
recommendation wins over an authors' setting on the same setting. A dismissal is saved in the configuration (`dismissed_recommendations`), and stops
applying once the setting changes. At the end of a run, the log lists what was applied,
what was dismissed, and the departures that were kept. Each run's record keeps the same, and
`methods.md` has a **Note**, before its references, that lists the departures the runs kept.

## Required changes

Some outputs need another one: per-structure statistics need a segmentation (and QSM, when
nothing else makes a map), a region reference needs a segmentation, SMWI needs QSM, χ-separation
needs the relaxometry maps its method reads (and a computed R2′ needs R2\* and R2), and
orientation grouping needs the published combined magnitude. In the TUI, choosing one of these
opens a **Required change** pop-up: *Apply required change* turns the output on, *Undo my change*
sets back the setting you just changed. Left unresolved (Esc), the output is marked.

A run turns such an output on itself when the configuration leaves it off — from the command
line or a config file as from the TUI — and says so in its log at info level ("Segmentation
turned on: Reference = ventricles needs it (...)"). The change is recorded in the
`pipeline_config.toml` it writes and in the methods text's Note:

```toml
required_changes = ["Segmentation turned on: Reference = ventricles needs it (the reference region is measured on a SynthSeg segmentation of the magnitude, which is saved with the outputs)"]
```

These are QSMxT's own requirements (how its steps link together). A method's own `required` rule
(from QSM.rs's pipeline rules: what the method cannot run with) is never applied unasked: the
run is refused, and `--accept-recommendations` applies it. A region reference that names no
region is an error.

:::tip
Rather than writing flags or TOML by hand, configure interactively in the
[TUI](/QSMxT/background/tui/) — it shows the equivalent `qsmxt run`
command live. Set things up once, then reuse the command on the CLI and on the
cluster; the `pipeline_config.toml` every run writes to `derivatives/qsmxt/` is a
config file you can pass back with `--config`.
:::
