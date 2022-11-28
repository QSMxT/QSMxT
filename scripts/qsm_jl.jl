#!/usr/bin/env julia

using MriResearchTools
using ArgParse
using QSM

s = ArgParseSettings()
@add_arg_table! s begin
    "--phase"
        help = "input - phase filename"
        required = true
    "--mask"
        help = "input - mask"
        required = true
    "--TEs"
        help = "input - echo times (s)"
        arg_type = Vector{Float64}
        required = true
    "--vsz"
        help = "input - voxel size (mm)"
        default = (1, 1, 1)
    "--b0-str"
        help = "input - magnetic field strength"
        default = 3
    "--b0-dir"
        help = "magnetic field direction"
        default = (0, 0, 1)
    "--qsm-out"
        help = "output - qsm"
        default = "qsm.nii"
    "--unwrapped-phase-out"
        help = "output - unwrapped phase"
        default = "unwrapped_phase.nii"
    "--tissue-phase-out"
        help = "output - tissue phase"
        default = "tissue_phase.nii"
end

args = parse_args(ARGS, s)

# constants
γ = 267.52      # gyromagnetic ratio

# load 3D single-, or multi-echo data using your favourite
# package, e.g. MAT.jl, NIfTI.jl, ParXRec.jl, ...
phase_nii = niread(args["phase"])
savenii(phase_nii.raw, "phase.nii", header=phase_nii.header)

B0 = args["b0-str"]    # main magnetic field strength
vsz = args["vsz"]      # voxel size (units??)
TEs = args["TEs"]      #,0.012,0.020,0.028)    # echo times
bdir = args["b0-dir"]  # direction of B-field

# extract brain mask from last echo using FSL's bet
mask_nii = niread(args["mask"])
mask = !=(0).(mask_nii.raw)

# unwrap phase + harmonic background field correction
uphase = unwrap_laplacian(phase_nii.raw, mask, vsz)
savenii(uphase, args["unwrapped-phase-out"], header=phase_nii.header)

# convert units
@views for t in axes(uphase, 4)
    uphase[:,:,:,t] .*= inv(B0 * γ * TEs[t])
end

# remove non-harmonic background fields
tissue_phase, vsharp_mask = vsharp(uphase, mask, vsz)
savenii(tissue_phase, args["tissue-phase-out"], header=phase_nii.header)

# dipole inversion
χ = rts(tissue_phase, vsharp_mask, vsz, bdir=bdir)
savenii(χ, args["qsm-out"], header=phase_nii.header)
