#!/usr/bin/env julia

using QuantitativeSusceptibilityMappingTGV
using MriResearchTools
using ArgParse

s = ArgParseSettings()
@add_arg_table! s begin
    "--phase"
        help = "input - phase filename"
        required = true
    "--mask"
        help = "input - mask filename"
        required = true
    "--erosions"
        help = "input - erosions"
        required = false
        default = "3"
    "--TE"
        help = "input - echo time (s)"
        required = true
    "--vsz"
        help = "input - voxel size (mm)"
        default = "(1,1,1)"
    "--b0-str"
        help = "input - magnetic field strength"
        default = "3"
    "--output"
        help = "output - qsm filename"
        default = "chi.nii"
end

args = parse_args(ARGS, s)

phase = readphase(args["phase"])
mask = niread(args["mask"]) .!= 0
erosions = eval(Meta.parse(args["erosions"]))
TE = eval(Meta.parse(args["TE"]))
vsz = eval(Meta.parse(args["vsz"]))
B0 = eval(Meta.parse(args["b0-str"]))
output = args["output"]

@time chi = qsm_tgv(phase, mask, vsz; TE=TE, fieldstrength=B0, erosions=erosions);
savenii(chi, args["output"]; header=header(phase))

