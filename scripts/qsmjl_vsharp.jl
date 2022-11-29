#!/usr/bin/env julia

using MriResearchTools
using ArgParse
using QSM

s = ArgParseSettings()
@add_arg_table! s begin
    "--frequency"
        help = "input - frequency image"
        required = true
    "--mask"
        help = "input - mask"
        required = true
    "--vsz"
        help = "input - voxel size (mm)"
        default = (1, 1, 1)
    "--tissue-frequency-out"
        help = "output - tissue frequency"
        default = "tissue_frequency.nii"
    "--vsharp-mask-out"
        help = "output - vsharp mask"
        default = "vsharp_mask.nii"
end

args = parse_args(ARGS, s)

# input parameters
vsz = args["vsz"]

# input data
frequency_nii = niread(args["frequency"])
mask_nii = niread(args["mask"])
mask = !=(0).(mask_nii.raw)

# background field removal
tissue_phase, vsharp_mask = vsharp(frequency_nii.raw, mask, vsz)
savenii(tissue_phase, args["tissue-frequency-out"], header=frequency_nii.header)
savenii(vsharp_mask, args["vsharp-mask-out"], header=frequency_nii.header)

