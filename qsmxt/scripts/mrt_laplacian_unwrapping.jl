#!/usr/bin/env julia
import Pkg
try
    using MriResearchTools, FFTW
catch
    Pkg.add(["MriResearchTools", "FFTW"])
    using MriResearchTools, FFTW
end

phase_path, phase_unwrapped_path = ARGS

phase_nii = niread(phase_path)
#phase = Float32.(phase_nii)
phase_unwrapped = laplacianunwrap(phase_nii.raw)
savenii(phase_unwrapped, phase_unwrapped_path; header=header(phase_nii))

