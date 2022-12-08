#!/usr/bin/env julia
using MriResearchTools
using FFTW

in_path, out_path = ARGS

phase_nii = niread(in_path)
phase = Float32.(phase_nii)
laplacianunwrap!(phase)
savenii(phase, out_path; header=header(phase_nii))

