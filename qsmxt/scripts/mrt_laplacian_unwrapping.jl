#!/usr/bin/env julia
import Pkg
try
    using MriResearchTools, FFTW
catch
    Pkg.add(["MriResearchTools", "FFTW"])
    using MriResearchTools, FFTW
end

phase_path, phase_unwrapped_path = ARGS

# Load the phase data
phase_nii = readphase(phase_path)

# Ensure that the phase data remains in floating-point format
phase_data = phase_nii

# Perform the Laplacian unwrapping on the floating-point data
phase_unwrapped = laplacianunwrap(phase_data)

# Save the result
savenii(phase_unwrapped, phase_unwrapped_path; header=header(phase_nii))
