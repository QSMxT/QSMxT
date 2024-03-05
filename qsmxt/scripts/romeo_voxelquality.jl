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
        help = "input - phase filenames"
        required = true
        nargs = '+'
    "--TEs"
        help = "input - echo times (s)"
        required = true
    "--mag"
        help = "input - mag filenames"
        nargs = '+'
    "--type"
        help = """grad || grad+second"""
        default = "grad"
    "--output"
        help = "output - unwrapped phase filename"
        required = true
end

args = parse_args(ARGS, s)
optional_args = Dict{Symbol, Any}()

# === TEs ===
TEs = let expr = Meta.parse(args["TEs"])
    Float32.(expr.args)
end
optional_args[:TEs] = TEs

# === MAG AND PHASE FILES ===
phs_files = args["phase"]
if !isnothing(args["mag"])
    mag_files = args["mag"]
end

# determine dimensions and array size
num_images = length(phs_files)
if num_images > 1
    phs_nii = readphase(phs_files[1])
    phs_shape = size(Float32.(phs_nii))
    phs_combined_shape = tuple(phs_shape..., num_images)
    phs_combined = Array{Float32}(undef, phs_combined_shape...)

    # fill array with data
    for i in 1:num_images
        local phs_nii = readphase(phs_files[i])
        phs = Float32.(phs_nii)
        phs_combined[:, :, :, i] = phs
    end

    # === MAGNITUDE ===
    if length(args["mag"]) > 0
        mag_files = args["mag"]

        # determine dimensions and array size
        mag_combined = Array{Float32}(undef, phs_combined_shape...)

        # fill array with data
        for i in 1:num_images
            local mag_nii = readmag(mag_files[i])
            mag = Float32.(mag_nii)
            mag_combined[:, :, :, i] = mag
        end

        optional_args[:mag] = mag_combined
    end
else
    phs_combined = Float32.(readphase(phs_files[1]))

    if length(args["mag"]) > 0
        mag_combined = Float32.(readmag(mag_files[1]))
        optional_args[:mag] = mag_combined
    end
end

 # === WEIGHTS ===
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

voxelquality = romeovoxelquality(phs_combined; weights, optional_args...) * 100
voxelquality[.!isfinite.(voxelquality)] .= 0
savenii(voxelquality, args["output"]; header=header(readphase(phs_files[1])))

#mask3 = robustmask(romeovoxelquality(phase; mag)); # Using magnitude and phase

