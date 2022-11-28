#!/usr/bin/env julia

using MriResearchTools
using QSM

# constants
γ = 267.52      # gyromagnetic ratio

# load 3D single-, or multi-echo data using your favourite
# package, e.g. MAT.jl, NIfTI.jl, ParXRec.jl, ...
phase_nii, mag_nii = niread("/tmp/bids-osf/sub-1/ses-1/anat/sub-1_ses-1_run-01_echo-01_part-phase_MEGRE.nii.gz"), niread("/tmp/bids-osf/sub-1/ses-1/anat/sub-1_ses-1_run-01_echo-01_part-mag_MEGRE.nii.gz")
savenii(mag_nii.raw, "mag.nii", header=mag_nii.header)
savenii(phase_nii.raw, "phase.nii", header=phase_nii.header)

B0 = 7          # main magnetic field strength
vsz = (1,1,1)    # voxel size (units??)
TEs = (0.004)#,0.012,0.020,0.028)    # echo times
bdir = (0,0,1)   # direction of B-field

# extract brain mask from last echo using FSL's bet
mask_nii = niread("/tmp/bids-osf/sub-1/ses-1/extra_data/sub-1_ses-1_run-01_brainmask.nii.gz")
mask = !=(0).(mask_nii.raw)
savenii(mask, "mask.nii", header=mag_nii.header)

# unwrap phase + harmonic background field correction
uphase = unwrap_laplacian(phase_nii.raw, mask, vsz)
savenii(uphase, "unwrap_laplacian.nii", header=phase_nii.header)

# convert units
@views for t in axes(uphase, 4)
    uphase[:,:,:,t] .*= inv(B0 * γ * TEs[t])
end

# remove non-harmonic background fields
tissue_phase, vsharp_mask = vsharp(uphase, mask, vsz)
savenii(tissue_phase, "tissue_phase.nii", header=mag_nii.header)
savenii(vsharp_mask, "vsharp_mask.nii", header=mag_nii.header)

# dipole inversion
χ = rts(tissue_phase, vsharp_mask, vsz, bdir=bdir)

savenii(χ, "chi.nii", header=phase_nii.header)
