#!/usr/bin/env python3
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
        'in_file',
        type=str,
        help='the input mask (nifti)'
    )

    parser.add_argument(
        'in_like',
        type=str,
        help='the input magnitude for resampling (nifti)'
    )

    parser.add_argument(
        'in_transform',
        type=str,
        help='the input transform (minc xfm)'
    )

    parser.add_argument(
        'out_file',
        type=str,
        help='the desired filename of the warped output mask (nifti)'
    )

    parser.add_argument(
        '--nearest',
        dest='nearest',
        action='store_true',
        help='use nearest neighbour sampling'
    )

    parser.add_argument(
        '--inverse',
        dest='inverse',
        action='store_true',
        help='invert transformation'
    )

    args = parser.parse_args()

    sys_cmd("mkdir .tmp")
    if ".nii.gz" in args.in_file: sys_cmd(f"gunzip -f -c {args.in_file} > .tmp/{os.path.basename(args.in_file).split(os.extsep)[0]}.nii")
    else: sys_cmd(f"cp {args.in_file} .tmp/{os.path.basename(args.in_file).split(os.extsep)[0]}.nii")
    if ".nii.gz" in args.in_like: sys_cmd(f"gunzip -f -c {args.in_like} > .tmp/{os.path.basename(args.in_like).split(os.extsep)[0]}.nii")
    else: sys_cmd(f"cp {args.in_like} .tmp/{os.path.basename(args.in_like).split(os.extsep)[0]}.nii")
    sys_cmd(f"nii2mnc .tmp/{os.path.basename(args.in_file).split(os.extsep)[0]}.nii .tmp/{os.path.basename(args.in_file).split(os.extsep)[0]}.mnc -clobber")
    sys_cmd(f"nii2mnc .tmp/{os.path.basename(args.in_like).split(os.extsep)[0]}.nii .tmp/{os.path.basename(args.in_like).split(os.extsep)[0]}.mnc -clobber")

    cmd = "mincresample "
    cmd += f"-transformation {args.in_transform} "
    cmd += "-invert_transformation " if args.inverse else ""
    cmd += "-nearest_neighbour -keep_real_range " if args.nearest else ""
    cmd += f"-like .tmp/{os.path.basename(args.in_like).split(os.extsep)[0]}.mnc "
    cmd += f".tmp/{os.path.basename(args.in_file).split(os.extsep)[0]}.mnc "
    cmd += f".tmp/{os.path.basename(args.out_file).split(os.extsep)[0]}.mnc"
    sys_cmd(cmd)
        
    sys_cmd(f"mnc2nii .tmp/{os.path.basename(args.out_file).split(os.extsep)[0]}.mnc {args.out_file.split(os.extsep)[0]}.nii")
    sys_cmd(f"rm -rf .tmp")
    
