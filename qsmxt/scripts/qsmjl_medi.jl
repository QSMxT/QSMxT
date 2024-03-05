#!/usr/bin/env julia
import Pkg
try
    using MriResearchTools, ArgParse, QSM
catch
    Pkg.add(["MriResearchTools", "ArgParse", "QSM"])
    using MriResearchTools, ArgParse, QSM
end

QSM.FFTW_NTHREADS[] = Threads.nthreads()

# input data
mag_nii = niread("/home/ashley/neurodesktop-storage/data/bids/sub-1/anat/sub-1_echo-1_part-mag_MEGRE.nii")
tissue_freq_nii = niread("/home/ashley/neurodesktop-storage/data/qsm/workflow/sub-1/sub-1/qsmxt/qsm_workflow/qsmjl_vsharp/sub-1_echo-1_part-phase_MEGRE_B0_normalized_vsharp.nii")
mask_nii = niread("/home/ashley/neurodesktop-storage/data/qsm/workflow/sub-1/sub-1/qsmxt/qsm_workflow/qsmjl_vsharp/sub-1_echo-1_part-mag_MEGRE_combined_bet-mask_ero_vsharp-mask.nii")
mask = !=(0).(mask_nii.raw)

# dipole inversion
x, cost_reg_history, cost_data_history = medi(tissue_freq_nii.raw, mag_nii.raw, mask)
#savenii(χ, args["qsm-out"], header=tissue_freq_nii.header)

