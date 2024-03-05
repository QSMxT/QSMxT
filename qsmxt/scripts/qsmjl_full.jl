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
    "--TEs"
        help = "input - echo times (s)"
        required = true
    "--vsz"
        help = "input - voxel size (mm)"
        default = (1, 1, 1)
    "--b0-str"
        help = "input - magnetic field strength"
        default = 3
    "--b0-dir"
        help = "magnetic field direction"
        default = (0, 0, 1)
    "--qsm-out"
        help = "output - qsm"
        default = "qsm.nii"
    "--unwrapped-phase-out"
        help = "output - unwrapped phase"
        default = "unwrapped_phase.nii"
    "--frequency-out"
        help = "output - frequency"
        default = "frequency.nii"
    "--tissue-frequency-out"
        help = "output - tissue phase"
        default = "tissue_frequency.nii"
end

args = parse_args(ARGS, s)

# constants
γ = 267.52      # gyromagnetic ratio

# input parameters
B0 = args["b0-str"]    # main magnetic field strength
vsz = args["vsz"]      # voxel size (units??)
bdir = args["b0-dir"]  # direction of B-field
TEs = let expr = Meta.parse(args["TEs"])
    @assert expr.head == :vect
    Float32.(expr.args)
end

# input data
phase_nii = niread(args["phase"])
mask_nii = niread(args["mask"])
mask = !=(0).(mask_nii.raw)

# phase unwrapping
uphase = unwrap_laplacian(phase_nii.raw, mask, vsz)
savenii(uphase, args["unwrapped-phase-out"], header=phase_nii.header)

# convert phase to frequency
@views for t in axes(uphase, 4)
    uphase[:,:,:,t] .*= inv(B0 * γ * TEs[t])
end
savenii(uphase, args["frequency-out"], header=phase_nii.header)

# background field removal
tissue_freq, vsharp_mask = vsharp(uphase, mask, vsz)
savenii(tissue_freq, args["tissue-frequency-out"], header=phase_nii.header)

# dipole inversion
χ = rts(tissue_freq, vsharp_mask, vsz, bdir=bdir)
savenii(χ, args["qsm-out"], header=phase_nii.header)

