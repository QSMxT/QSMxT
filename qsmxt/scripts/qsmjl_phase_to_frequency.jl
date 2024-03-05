#!/usr/bin/env julia
import Pkg
try
    using MriResearchTools, ArgParse, QSM
catch
    Pkg.add(["MriResearchTools", "ArgParse", "QSM"])
    using MriResearchTools, ArgParse, QSM
end

QSM.FFTW_NTHREADS[] = Threads.nthreads()

s = ArgParseSettings()
@add_arg_table! s begin
    "--phase"
        help = "input - phase"
        required = true
    "--TEs"
        help = "input - echo times (s)"
        required = true
    "--b0-str"
        help = "input - magnetic field strength"
        default = "3"
    "--frequency-out"
        help = "output - frequency"
        default = "frequency.nii"
end

args = parse_args(ARGS, s)

# constants
γ = 42.58*10**6 # MHz/T

# input parameters
B0 = args["b0-str"]    # main magnetic field strength
TEs = let expr = Meta.parse(args["TEs"])
    @assert expr.head == :vect
    Float32.(expr.args)
end
B0 = Float32(Meta.parse(args["b0-str"]))

# input data
phase_nii = niread(args["phase"])
phase = phase_nii.raw

# convert frequency to hertz
@views for t in axes(phase, 4)
    phase[:,:,:,t] .*= inv(B0 * γ * 2*pi * TEs[t]) * 1e6
end
savenii(phase, args["frequency-out"], header=phase_nii.header)

