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
    "--magnitude"
        help = "input - magnitude files"
        required = true
        nargs = '+'
    "--TEs"
        help = "input - echo times (s)"
        required = true
    "--t2starmap"
        help = "output - t2* map"
        required = true
    "--r2starmap"
        help = "output - r2* map"
        required = true
end

args = parse_args(ARGS, s)

# parse TEs
TEs = let expr = Meta.parse(args["TEs"])
    Float32.(expr.args)
end

TEs *= 1e3

# get magnitude filenames
mag_files = args["magnitude"]

# determine dimensions and array size
mag_nii = readmag(mag_files[1])
num_images = length(mag_files)
mag_shape = size(Float32.(mag_nii))
mag_combined_shape = tuple(mag_shape..., num_images)
mag_combined = Array{Float32}(undef, mag_combined_shape...)

# fill array with data
for i in 1:num_images
    local mag_nii = readmag(mag_files[i])
    mag = Float32.(mag_nii)
    mag_combined[:, :, :, i] = mag
end

# Free memory used by t2starmap before computing r2starmap
t2starmap = nothing
GC.gc()

# create and save t2starmap
t2starmap = NumART2star(mag_combined, TEs)
savenii(t2starmap, args["t2starmap"]; header=header(mag_nii))
r2starmap = r2s_from_t2s(t2starmap)
savenii(r2starmap, args["r2starmap"]; header=header(mag_nii))

