---
title: 7 T and low field
description: "Data from field strengths other than 3 T."
---

QSMxT's defaults are not tuned to one field strength, and most methods take the field strength
as an input.

## What QSMxT does automatically

- It converts phase to ppm with the scanner's centre frequency (`ImagingFrequency`) when the
  sidecar gives it, or the field strength (`MagneticFieldStrength`) otherwise. The methods text
  says which.
- Methods whose constants depend on the field strength scale them from it (for example
  χ-separation's relaxometric constant).
- At 7 T (6.5 T and above), choosing DECOMPOSE brings up suggestions for 7 T: unwrapping its
  phase along the echoes and bounding χ from the echo spacing.
- R2PRIMEnet, which predicts R2′ when there is no spin-echo scan, is trained at 3 T only. Away
  from 3 T (by more than 0.3 T) QSMxT does not use it: asking for R2′, or a method that needs
  R2′, without a MESE is an error.

## What it doesn't do / limits

- Deep-learning methods were trained on data from particular scanners and field strengths;
  QSMxT doesn't check that your data matches. Their pages in QSM.rs say what they were
  trained on.
- At low field the phase is weaker for the same echo time, so maps are noisier; nothing in
  QSMxT compensates for that.

## What you can do

- Check that `qsmxt validate` reports the field strength you expect.
- For R2′-based source separation away from 3 T, acquire a MESE
  ([Source separation and MESE](/QSMxT/data/source-separation/)) or supply an R2′ map
  ([Non-BIDS or partial data](/QSMxT/data/non-bids/)).
  `--r2primenet-ignore-field-mismatch` runs R2PRIMEnet anyway, with a warning; treat the
  results as uncalibrated.

More: [How QSMxT reads your data](/QSMxT/background/input-handling/#field-strength-and-centre-frequency)
and [where R2′ comes from](/QSMxT/reference/source-separation/#where-r2-comes-from).
