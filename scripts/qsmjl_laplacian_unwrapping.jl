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
    "--vsz"
        help = "input - voxel size (mm)"
        default = (1, 1, 1)
    "--unwrapped-phase-out"
        help = "output - unwrapped phase"
        default = "unwrapped_phase.nii"
end

args = parse_args(ARGS, s)

# input parameters
vsz = args["vsz"]      # voxel size (units??)

# input data
phase_nii = niread(args["phase"])
mask_nii = niread(args["mask"])
mask = !=(0).(mask_nii.raw)

# phase unwrapping
uphase = unwrap_laplacian(phase_nii.raw, mask, vsz)
savenii(uphase, args["unwrapped-phase-out"], header=phase_nii.header)

