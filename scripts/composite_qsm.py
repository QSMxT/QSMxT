#!/usr/bin/env python
import argparse
import subprocess
import os, os.path
import nibabel as nib
import numpy as np

def sys_cmd(cmd, print_output = True, print_command = True):
    if print_command:
        print(cmd)
        
    result_byte = subprocess.run(cmd, shell=True, stdout=subprocess.PIPE).stdout
    results     = result_byte.decode('UTF-8')[:-2]
    
    if print_output:
        print(results, end="")
    
    return results

if __name__ == "__main__":
    parser = argparse.ArgumentParser()

    parser.add_argument(
        'romeo_qsm',
        type=str
    )

    parser.add_argument(
        'bet_qsm',
        type=str
    )

    parser.add_argument(
        'out_file',
        type=str
    )

    args = parser.parse_args()

    romeo_qsm_nii = nib.load(args.romeo_qsm)
    bet_qsm_nii = nib.load(args.bet_qsm)

    romeo_qsm = romeo_qsm_nii.get_fdata()
    bet_qsm = bet_qsm_nii.get_fdata()

    romeo_zeros = np.array(np.abs(romeo_qsm) < 0.000001, dtype=np.int16)
    bet_mask = np.array(np.abs(bet_qsm) > 0.000001, dtype=np.int16)
    #bet_zeros = np.array(np.abs(bet_qsm) < 0.000001, dtype=np.int16)
    #romeo_mask = np.array(np.abs(romeo_qsm) > 0.000001, dtype=np.int16)
    
    bet_extras_mask = bet_mask * romeo_zeros
    #romeo_extras_mask = romeo_mask * bet_zeros
    #extras_mask = bet_extras_mask + romeo_extras_mask

    composite_qsm = bet_extras_mask * bet_qsm + romeo_qsm
    # added voxels - voxels added by ROMEO qsm and voxels added by BET QSM

    nib.save(nib.Nifti1Image(composite_qsm, None), args.out_file)
    #nib.save(nib.Nifti1Image(bet_extras_mask, None), "1_bet_extras_mask.nii")
    #nib.save(nib.Nifti1Image(bet_qsm, None), "2_bet_qsm.nii")
    #nib.save(nib.Nifti1Image(romeo_qsm, None), "3_romeo_qsm.nii")
    #nib.save(nib.Nifti1Image(extras_mask, None), "4_extras_mask.nii")
