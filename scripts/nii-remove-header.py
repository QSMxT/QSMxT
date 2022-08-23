#!/usr/bin/env python3
import nibabel as nib
import numpy as np
import sys
import argparse

def load_nii(file_path):
    try:
        return nib.load(file_path)
    except:
        raise argparse.ArgumentTypeError(f"{file_path} is not a valid nifti file!")


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

    args = parser.parse_args()
    in_file = args.in_file
    
    # keep pixel dimension for non-isotropic data
    header = nib.Nifti1Header()
    
    header['pixdim'] = in_file.header['pixdim']
    header['scl_inter'] = in_file.header['scl_inter']
    header['scl_slope'] = in_file.header['scl_slope']
    header['descrip'] = in_file.header['descrip']
    header['dim_info'] = in_file.header['dim_info']
    header['dim_info'] = in_file.header['dim_info']
    header['datatype'] = in_file.header['datatype']
    header['bitpix'] = in_file.header['bitpix']

    data = in_file.get_fdata()

    # HACK
    if in_file.header['qform_code'] == in_file.header['sform_code'] == 1:
        data = np.flip(data, axis=0)
        data = np.flip(data, axis=1)

    nib.save(nib.nifti1.Nifti1Image(data, affine=None, header=header), args.out_file)
