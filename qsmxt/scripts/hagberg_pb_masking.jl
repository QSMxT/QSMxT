#!/usr/bin/env julia
import Pkg
try
    using MriResearchTools, ArgParse
catch
    Pkg.add(["MriResearchTools", "ArgParse"])
    using MriResearchTools, ArgParse
end

s = ArgParseSettings()
@add_arg_table! s begin
    "--phase"
        help = "input - phase filename"
        required = true
    "--threshold"
        help = "threshold factor"
        default = 1.0
        required = false
    "--output"
        help = "output - mask filename"
        required = true
end

args = parse_args(ARGS, s)

phase_nii = niread(args["phase"])
phase = Float32.(phase_nii)

mask = phase_based_mask(phase; filter=false)#, threshold=args["threshold"])

savenii(mask, args["output"]; header=header(phase_nii))
