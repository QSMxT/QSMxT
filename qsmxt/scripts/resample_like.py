#!/usr/bin/env python

import nibabel as nib
import nilearn.image
import numpy as np
import argparse

def resample_to_reference(source_file, reference_file, output_file, interpolation='continuous'):
    # Load NIfTI files
    source_nii = nib.load(source_file)
    reference_nii = nib.load(reference_file)

    # Resample the source image to match the reference image
    resampled_nii = nilearn.image.resample_img(
        source_nii, 
        target_affine=reference_nii.affine, 
        target_shape=np.array(reference_nii.header.get_data_shape()),
        interpolation=interpolation
    )
    
    # Save the resampled image
    nib.save(resampled_nii, output_file)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Resample a NIfTI image to match the dimensions, resolution, and orientation of a reference image.")
    parser.add_argument("source_file", help="Path to the source NIfTI file to be resampled.")
    parser.add_argument("reference_file", help="Path to the reference NIfTI file.")
    parser.add_argument("output_file", help="Path to save the resampled NIfTI file.")
    parser.add_argument("--interpolation", choices=['nearest', 'continuous'], default='continuous', 
                        help="Type of interpolation for resampling ('nearest' or 'continuous'). Default is 'continuous'.")
    args = parser.parse_args()

    resample_to_reference(args.source_file, args.reference_file, args.output_file, args.interpolation)

