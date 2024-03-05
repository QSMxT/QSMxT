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
    "in_file"
        help = "input phase file"
        required = true
    "out_file"
        help = "output mask"
        required = true
end
parsed_args = parse_args(ARGS, s)

phase_dir = parsed_args["in_file"]
out_dir = parsed_args["out_file"]

phase_nii = readphase(phase_dir)
hdr = header(phase_nii)

phase = dropdims(phase_nii, dims = (findall(size(phase_nii) .== 1)...,));
weights_edges = 256 .- MriResearchTools.ROMEO.calculateweights(phase[:,:,:,1]; weights=:romeo)
weights_voxel = dropdims(sum(weights_edges; dims=1); dims=1)

savenii(Float64.(weights_voxel), out_dir; header=hdr)

