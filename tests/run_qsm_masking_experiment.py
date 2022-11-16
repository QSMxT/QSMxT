#!/usr/bin/env python3
import osfclient
import os
import tempfile
import run_2_qsm as qsm
import run_5_analysis as analysis
from scripts.sys_cmd import sys_cmd
import numpy as np
from run_test_qsm import workflow

# copied, because fixture cannot be called directly
def bids_dir(tmp_dir=tempfile.gettempdir()):
    if not os.path.exists(os.path.join(tmp_dir, 'bids-osf')):
        if not os.path.exists(os.path.join(tmp_dir, 'bids-osf.tar')):
            print("Downloading test data...")
            file_pointer = next(osfclient.OSF().project("9jc42").storage().files)
            file_handle = open(os.path.join(tmp_dir, 'bids-osf.tar'), 'wb')
            file_pointer.write_to(file_handle)
        print("Extracting test data...")
        sys_cmd(f"tar xf {os.path.join(tmp_dir, 'bids-osf.tar')} -C {tmp_dir}")
        sys_cmd(f"rm {os.path.join(tmp_dir, 'bids-osf.tar')}")
    return os.path.join(tmp_dir, 'bids-osf')

def run_qsm(data_dir, out_dir, th1=None, th2=None):
    print(f"Running QSM for {out_dir}")
    args = qsm.process_args(qsm.parse_args([
        data_dir,
        os.path.join(out_dir, "qsm")
    ]))
    run_args = None
    if th1 is not None and th2 is not None:
        run_args = { "masking_threshold" : (th1, th2) }
    elif th1 is not None:
        run_args = { "masking_threshold" : th1 }
    print(f"run_args: {run_args}")
    workflow(args, True, True, run_args)

def run_analysis(data_dir, result_dir):
    print(f"Running analysis for {result_dir}")
    args = ["--segmentations", data_dir + "/sub-1/ses-1/extra_data/sub-1_ses-1_run-01_segmentation.nii.gz",
            "--qsm_files", result_dir + "/qsm/qsm_final/sub-1_ses-1_run-01_echo-01_part-phase_MEGRE_qsm_000_twopass_average.nii",
            "--output_dir", result_dir + "/test_analysis",
            "--qsm_ground_truth", data_dir + "/sub-1/ses-1/extra_data/sub-1_ses-1_run-01_chi-interpolated.nii.gz"]
    analysis.run_analysis(analysis.parse_args(args))


if __name__ == "__main__":
    ## Need to run this first in bash to import from the current changed module instead of /opt/qsmxt:
    # export PYTHONPATH=/neurodesktop-storage/qsmxt/:$PYTHONPATH
    # export PATH=/neurodesktop-storage/qsmxt/scripts/:$PATH
    
    data_dir = bids_dir()
    out_dir = "results"
    
    # Automatic masking
    current_dir = out_dir + "/automatic_masking"
    if not os.path.isdir(current_dir + "/test_analysis"):
        run_qsm(data_dir, current_dir)
        run_analysis(data_dir, current_dir)
    
    # One mask threshold 0.1:0.05:0.8
    for thresh in np.arange(0.1, 0.8, 0.05):
        current_dir = out_dir + f"/one_thresh_masking_{thresh:.2f}"
        if not os.path.isdir(current_dir + "/test_analysis"):
            run_qsm(data_dir, current_dir, thresh)
            run_analysis(data_dir, current_dir)
    
    # Two smallMaskTh 0.1:0.1:0.8 x filledMaskTh 0.1:0.1:0.8
    for th1 in np.arange(0.1, 0.8, 0.1):
        for th2 in np.arange(0.1, 0.8, 0.1):
            current_dir = out_dir + f"/two_thresh_masking_{th1:.2f}_{th2:.2f}"
            if not os.path.isdir(current_dir + "/test_analysis"):
                run_qsm(data_dir, current_dir, th1, th2)
                run_analysis(data_dir, current_dir)
    