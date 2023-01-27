#!/usr/bin/env julia
import Pkg

using MriResearchTools
using ArgParse

s = ArgParseSettings()
@add_arg_table! s begin
    "--phase"
        help = "input - phase filename"
        required = true
    "--mag"
        help = "input - mag filename"
    "--type"
        help = """grad || grad+second"""
        default = "grad"
    "--output"
        help = "output - unwrapped phase filename"
        required = true
end

args = parse_args(ARGS, s)
optional_args = Dict{Symbol, Any}()
if !isnothing(args["mag"])
    optional_args[:mag] = Float32.(niread(args["mag"]))
end

phase_nii = readphase(args["phase"])
phase = Float32.(phase_nii)

weights = falses(6)
if contains(args["type"], "grad")
    weights[1] = true
end
if contains(args["type"], "multi-echo")
    weights[2] = true
end
if contains(args["type"], "second")
    weights[3] = true
end
if contains(args["type"], "mag_coherence")
    weights[4] = true
end
if contains(args["type"], "mag_weight")
    weights[5] = true
end
voxelquality = romeovoxelquality(phase; weights, optional_args...) * 100
voxelquality[.!isfinite.(voxelquality)] .= 0

savenii(voxelquality, args["output"]; header=header(phase_nii))
