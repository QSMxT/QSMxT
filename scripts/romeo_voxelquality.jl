#!/usr/bin/env julia
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
        help = """grad || grad+second || grad+mag || grad+second+mag"""
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

phase_nii = niread(args["phase"])
phase = Float32.(phase_nii)

weights = falses(6)
if args["type"] == "grad+second+mag"
    weights[[1,3,4]] .= true
elseif args["type"] == "grad"
    weights[1] = true
elseif args["type"] == "second"
        weights[3] = true
elseif args["type"] == "grad+second"
    weights[[1,3]] .= true
elseif args["type"] == "grad+mag"
    weights[[1,4]] .= true
end
voxelquality = romeovoxelquality(phase; weights, optional_args...)

savenii(voxelquality, args["output"]; header=header(phase_nii))
