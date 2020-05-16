#!/usr/bin/env python
import argparse
import subprocess
import os, os.path

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
        'in_fixed',
        type=str,
        help='the input fixed image (nifti)'
    )

    parser.add_argument(
        'in_moving',
        type=str,
        help='the input moving image (nifti)'
    )

    parser.add_argument(
        'out_transform',
        type=str,
        help='the desired filename of the output transformation (minc xfm)'
    )

    args = parser.parse_args()

    # convert to mnc
    sys_cmd("mkdir .tmp")
    if ".nii.gz" in args.in_fixed: sys_cmd(f"gunzip -f -c {args.in_fixed} > .tmp/{os.path.basename(args.in_fixed).split(os.extsep)[0]}.nii")
    else: sys_cmd(f"cp {args.in_fixed} .tmp/{os.path.basename(args.in_fixed).split(os.extsep)[0]}.nii")
    if ".nii.gz" in args.in_moving: sys_cmd(f"gunzip -f -c {args.in_moving} > .tmp/{os.path.basename(args.in_moving).split(os.extsep)[0]}.nii")
    else: sys_cmd(f"cp {args.in_moving} .tmp/{os.path.basename(args.in_moving).split(os.extsep)[0]}.nii")
    sys_cmd(f"nii2mnc .tmp/{os.path.basename(args.in_fixed).split(os.extsep)[0]}.nii .tmp/{os.path.basename(args.in_fixed).split(os.extsep)[0]}.mnc -clobber")
    sys_cmd(f"nii2mnc .tmp/{os.path.basename(args.in_moving).split(os.extsep)[0]}.nii .tmp/{os.path.basename(args.in_moving).split(os.extsep)[0]}.mnc -clobber")

    # do registration
    sys_cmd(
        f"bestlinreg \
            .tmp/{os.path.basename(args.in_moving).split(os.extsep)[0]}.mnc \
            .tmp/{os.path.basename(args.in_fixed).split(os.extsep)[0]}.mnc \
            {args.out_transform.split(os.extsep)[0]}.xfm \
            -clobber"
    )

    # remove temporary files
    sys_cmd(f"rm -rf .tmp")
    
