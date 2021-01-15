#!/bin/python
import nibabel as nib
import numpy as np
import sys
import argparse

def load_nii(file_path):
    try:
        return nib.load(file_path).get_fdata()
    except:
        raise argparse.ArgumentTypeError(f"{file_path} is not a valid nifti file!")

def save_nii(data, file_path):
    nib.save(nib.nifti1.Nifti1Image(data, None), file_path)

def get_np_datatype(type_name):
    types = {
        'float' : np.float,
        'float32' : np.float32,
        'float64' : np.float64,
        'int' : np.int,
        'int8' : np.int8,
        'int16' : np.int16,
        'int32' : np.int32,
        'int64' : np.int64
    }
    return types[type_name]


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="remove the header from a nifti file."
    )

    parser.add_argument(
        'in_file',
        type=load_nii,
        help='the input file to remove the nifti header from'
    )

    parser.add_argument(
        'out_file',
        help='the desired output filename'
    )

    parser.add_argument(
        '--dtype',
        help='datatype',
        type=get_np_datatype,
        default='float'
    )

    args = parser.parse_args()
    in_file = args.in_file
    save_nii(np.array(in_file, dtype=args.dtype), args.out_file)
