#!/usr/bin/env julia
using MriResearchTools

phase_dir = ARGS[1]
TEs = [parse(Float64, x) for x in split(ARGS[2], ',')]
weights_threshold = parse(Int, ARGS[3])
#TEs = [5.84,10.63,15.42,20.21,25]
out_dir = ARGS[4]

phase_nii = readphase(phase_dir)
hdr = header(phase_nii)

phase = dropdims(phase_nii, dims = (findall(size(phase_nii) .== 1)...,));
weights_edges = 256 .- MriResearchTools.ROMEO.calculateweights(phase[:,:,:,1]; weights=:romeo, phase2=phase[:,:,:,2], TEs=TEs)
weights_voxel = dropdims(sum(weights_edges; dims=1); dims=1)
mask = Float64.(weights_voxel .> weights_threshold)

savenii(mask, out_dir; header=hdr)
