#!/usr/bin/env julia
import Pkg
try
    using ROMEO, MriResearchTools, ArgParse
catch
    Pkg.add(["ROMEO", "MriResearchTools", "ArgParse"])
    using ROMEO, MriResearchTools, ArgParse
end

@time msg = unwrapping_main(ARGS)
println(msg)
