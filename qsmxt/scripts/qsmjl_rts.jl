#!/usr/bin/env julia

using MriResearchTools
using ArgParse
using QSM

QSM.FFTW_NTHREADS[] = Threads.nthreads()

s = ArgParseSettings()
@add_arg_table! s begin
    "--qsm_algorithm"
        help = "rts | tkd | tsvd | nltv | tv"
    "--tissue-frequency"
        help = "input - tissue frequency"
        required = true
    "--mask"
        help = "input - mask"
        required = true
    "--vsz"
        help = "input - voxel size (mm)"
        default = "(1,1,1)"
    "--b0-dir"
        help = "magnetic field direction"
        default = "(0,0,1)"
    "--qsm-out"
        help = "output - qsm"
        default = "qsm.nii"
end

args = parse_args(ARGS, s)

# input parameters
vsz = Tuple(eval(Meta.parse(args["vsz"]))) # voxel size (units??)
bdir = Tuple(eval(Meta.parse(args["b0-dir"])))  # direction of B-field

# input data
tissue_freq_nii = niread(args["tissue-frequency"])
mask_nii = niread(args["mask"])
mask = !=(0).(mask_nii.raw)

# dipole inversion
χ = rts(tissue_freq_nii.raw, mask, vsz, bdir=bdir)
savenii(χ, args["qsm-out"], header=tissue_freq_nii.header)

