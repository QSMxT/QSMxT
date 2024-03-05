#!/usr/bin/env julia
import Pkg
try
    using CLEARSWI, MriResearchTools, ArgParse
catch
    Pkg.add(["CLEARSWI", "MriResearchTools", "ArgParse"])
    using CLEARSWI, MriResearchTools, ArgParse
end

s = ArgParseSettings()
@add_arg_table! s begin
    "--phase"
        help = "input - phase files"
        required = true
        nargs = '+'
    "--magnitude"
        help = "input - magnitude files"
        required = true
        nargs = '+'
    "--TEs"
        help = "input - echo times (s)"
        required = true
    "--swi-out"
        help = "output - swi"
        required = true
    "--mip-out"
        help = "output - mip"
        default = nothing
        required = false
end

args = parse_args(ARGS, s)

# parse TEs
TEs = let expr = Meta.parse(args["TEs"])
    Float32.(expr.args)
end

TEs *= 1e3

# get magnitude filenames
mag_files = args["magnitude"]
phs_files = args["phase"]

# determine dimensions and array size
mag_nii = readmag(mag_files[1])
num_images = length(mag_files)
shape = size(Float32.(mag_nii))
combined_shape = tuple(shape..., num_images)

mag_combined = Array{Float32}(undef, combined_shape...)
phs_combined = Array{Float32}(undef, combined_shape...)

# fill array with data
for i in 1:num_images
    local mag_nii = readmag(mag_files[i])
    mag = Float32.(mag_nii)
    mag_combined[:, :, :, i] = mag
    local phs_nii = readphase(phs_files[i])
    phs = Float32.(phs_nii)
    phs_combined[:, :, :, i] = phs
end

# create and save swi
data = Data(mag_combined, phs_combined, mag_nii.header, TEs);
swi = calculateSWI(data);
savenii(swi, args["swi-out"]; header=header(mag_nii))

if !isnothing(args["mip-out"])
    mip = createMIP(swi);
    savenii(mip, args["mip-out"]; header=header(mag_nii))
end

