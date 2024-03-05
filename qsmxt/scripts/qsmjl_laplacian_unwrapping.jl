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
        help = "input - phase filename"
        required = true
    "--mask"
        help = "input - mask"
        required = true
    "--vsz"
        help = "input - voxel size (mm)"
        default = "(1,1,1)"
    "--unwrapped-phase-out"
        help = "output - unwrapped phase"
        default = "unwrapped_phase.nii"
end

args = parse_args(ARGS, s)

# input parameters
vsz = Tuple(eval(Meta.parse(args["vsz"])))

# input data
phase_nii = niread(args["phase"])
mask_nii = niread(args["mask"])
mask = !=(0).(mask_nii.raw)

# convert types and input parameters
phase = phase_nii.raw
vsz = Tuple{Float64, Float64, Float64}(map(Float64, vsz))

# phase unwrapping #BUG: This is also background field removal
uphase = unwrap_laplacian(phase, mask, vsz)
savenii(uphase, args["unwrapped-phase-out"], header=phase_nii.header)

