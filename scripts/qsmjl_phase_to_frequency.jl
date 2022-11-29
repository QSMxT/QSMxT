#!/usr/bin/env julia

using MriResearchTools
using ArgParse
using QSM

s = ArgParseSettings()
@add_arg_table! s begin
    "--phase"
        help = "input - phase"
        required = true
    "--mask"
        help = "input - mask"
        required = true
    "--TEs"
        help = "input - echo times (s)"
        required = true
    "--vsz"
        help = "input - voxel size (mm)"
        default = (1, 1, 1)
    "--b0-str"
        help = "input - magnetic field strength"
        default = 3
    "--frequency-out"
        help = "output - frequency"
        default = "frequency.nii"
end

args = parse_args(ARGS, s)

# constants
γ = 267.52      # gyromagnetic ratio

# input parameters
B0 = args["b0-str"]    # main magnetic field strength
vsz = args["vsz"]      # voxel size (units??)
TEs = let expr = Meta.parse(args["TEs"])
    @assert expr.head == :vect
    Float32.(expr.args)
end

# input data
phase_nii = niread(args["phase"])
mask_nii = niread(args["mask"])
mask = !=(0).(mask_nii.raw)
phase = phase_nii.raw

# convert frequency to hertz
@views for t in axes(phase, 4)
    phase[:,:,:,t] .*= inv(B0 * γ * TEs[t])
end
savenii(phase, args["frequency-out"], header=phase_nii.header)

