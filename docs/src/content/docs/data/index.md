---
title: Your data
description: "What QSMxT does for particular kinds of data, and what you may want to change."
---

For most data there is nothing to do: you convert your scans to BIDS and run QSM, the same way
for every scanner ([Convert DICOM to BIDS](/QSMxT/how-to/convert-dicom/),
[Run QSM](/QSMxT/how-to/run-qsm/)). QSMxT reads what it needs from the scan and adapts to it.

These pages are for when something about your data is different. Each says what QSMxT does
automatically, what it doesn't do, and what you can do.

| Your data | Page |
|---|---|
| GE, or real and imaginary images instead of phase | [GE and real/imaginary data](/QSMxT/data/ge-real-imaginary/) |
| A separate image for each receive coil | [Uncombined coil data](/QSMxT/data/uncombined-coils/) |
| Slices tilted away from axial | [Oblique slices](/QSMxT/data/oblique/) |
| A 2D multi-slice acquisition, or gaps between slices | [2D acquisitions and slice gaps](/QSMxT/data/2d-slice-gaps/) |
| One echo | [Single-echo data](/QSMxT/data/single-echo/) |
| Echoes read out in alternating directions | [Bipolar readouts](/QSMxT/data/bipolar/) |
| 7 T, 1.5 T or lower | [7 T and low field](/QSMxT/data/field-strength/) |
| Rodents, other animals, ex vivo | [Rodent and other non-human data](/QSMxT/data/non-human/) |
| Paramagnetic and diamagnetic χ, or a multi-echo spin-echo (MESE) scan | [Source separation and MESE](/QSMxT/data/source-separation/) |
| The same head at several orientations | [Multiple head orientations](/QSMxT/data/multi-orientation/) |
| NIfTI files, missing sidecars, or maps from other tools | [Non-BIDS or partial data](/QSMxT/data/non-bids/) |

`qsmxt validate study/bids` shows what QSMxT read from each scan (echoes, echo times, field
strength, obliquity) and which outputs it can make. Start there when something is unexpected.
