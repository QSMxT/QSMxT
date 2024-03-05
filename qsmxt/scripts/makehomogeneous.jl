#!/usr/bin/env julia
import Pkg
try
    using MriResearchTools
catch
    Pkg.add(["MriResearchTools"])
    using MriResearchTools
end

in_path = ARGS[1];
out_path = ARGS[2];
out_folder = splitdir(out_path)[1]
out_file = splitext(splitdir(out_path)[2])[1]
out_ext = splitext(splitdir(out_path)[2])[2]

mag = readmag(in_path);
corrected = makehomogeneous(Float32.(mag); sigma=[20, 20, 10])
savenii(corrected, out_file * out_ext, out_folder, header(mag))
