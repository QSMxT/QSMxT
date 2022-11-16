#!/usr/bin/env python3
import os
import run_2_qsm as qsm
import numpy as np
from run_test_qsm import workflow

def run_qsm(data_dir, out_dir, th1=None, th2=None):
    print(f"Running QSM for {out_dir}")
    args = qsm.process_args(qsm.parse_args([
        data_dir,
        os.path.join(out_dir, "qsm"),
        "--tgvqsm_iterations", "1" ## TODO remove when everything seems fine!
    ]))
    run_args = None
    if th1 is not None and th2 is not None:
        run_args = { "masking_threshold" : (th1, th2) }
    elif th1 is not None:
        run_args = { "masking_threshold" : th1 }
    print(f"run_args: {run_args}")
    workflow(args, True, True, run_args)

if __name__ == "__main__":
    ## Need to run this first in bash to import from the current changed module instead of /opt/qsmxt:
    # export PYTHONPATH=/neurodesktop-storage/qsmxt/:$PYTHONPATH
    # export PATH=/neurodesktop-storage/qsmxt/scripts/:$PATH
    
    data_dir = "/neurodesktop-storage/qsm_josef_data/bids"
    out_dir = "results_josef_data"
    
    # Automatic masking
    current_dir = out_dir + "/automatic_masking"
    if not os.path.isdir(current_dir + "/qsm/qsm_final"):
        run_qsm(data_dir, current_dir)
    
    # One mask threshold 0.1:0.05:0.8
    for thresh in np.arange(0.15, 0.8, 0.05):
        current_dir = out_dir + f"/one_thresh_masking_{thresh:.2f}"
        if not os.path.isdir(current_dir + "/qsm/qsm_final"):
            run_qsm(data_dir, current_dir, thresh)
    
    # Two smallMaskTh 0.1:0.1:0.8 x filledMaskTh 0.1:0.1:0.8
    # for th1 in np.arange(0.1, 0.8, 0.1):
    #     for th2 in np.arange(0.1, 0.8, 0.1):
    #         current_dir = out_dir + f"/two_thresh_masking_{th1:.2f}_{th2:.2f}"
    #         if not os.path.isdir(current_dir + "/test_analysis"):
    #             run_qsm(data_dir, current_dir, th1, th2)
    