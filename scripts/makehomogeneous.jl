#!/bin/julia
using MriResearchTools
img = readmag(ARGS[1]);
corrected = makehomogeneous(Float32.(img); σ=[20, 20, 10]);
savenii(corrected, splitdir(ARGS[2])[2], splitdir(ARGS[2])[1], header(img));
