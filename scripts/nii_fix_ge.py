#!/usr/bin/env python3

import nibabel as nib
import numpy as np
import argparse
import os

def fix_ge_polar(mag_path, phase_path, delete_originals=True):

    # ensure paths are absolute
    mag_path = os.path.abspath(mag_path)
    phase_path = os.path.abspath(phase_path)

    # load magnitude data
    mag_nii = nib.load(mag_path)
    mag_data = mag_nii.get_fdata()

    # load phase data
    phase_nii = nib.load(phase_path)
    phase_data = phase_nii.get_fdata()

    # compute complex result in the image domain
    phase_data_scaled = phase_data / 4096 * np.pi
    complex_data_image = mag_data * (np.cos(phase_data_scaled) + 1j * np.sin(phase_data_scaled))
    scaling = np.sqrt(complex_data_image.size)
    complex_data_kspace = np.fft.fftshift (np.fft.fftshift (np.fft.fftn(  np.fft.fftshift(complex_data_image))), axes=2) / scaling
    complex_data_correct_image = np.fft.fftshift(np.fft.ifftn(np.fft.fftshift(complex_data_kspace))) * scaling        

    # compute corrected phase image
    phase_corr_data = np.angle(complex_data_correct_image)

    # create nifti image
    phase_nii.header.set_data_dtype(np.float32)
    phase_corr_nii = nib.Nifti1Image(phase_corr_data, phase_nii.affine, phase_nii.header)
    
    # determine filename
    extension = ".".join(phase_path.split('.')[1:])
    phase_corr_path = f"{phase_path.split('.')[0]}_corrected.{extension}"

    # save new images to file
    nib.save(phase_corr_nii, phase_corr_path)

    # delete original images and rename if necessary
    if delete_originals:
        os.remove(phase_path)
        os.rename(phase_corr_path, phase_path)


def fix_ge_complex(real_path, imag_path, delete_originals=False):

    # ensure paths are absolute
    real_path = os.path.abspath(real_path)
    imag_path = os.path.abspath(imag_path)

    # load real data
    real_nii = nib.load(real_path)
    real_data = real_nii.get_fdata()
    
    # load imaginary data
    imag_nii = nib.load(imag_path)
    imag_data = imag_nii.get_fdata()

    # compute complex result in the image domain
    complex_data_image = real_data + 1j * imag_data
    scaling = np.sqrt(complex_data_image.size)
    complex_data_kspace = np.fft.fftshift(np.fft.fftshift (np.fft.fftn(  np.fft.fftshift(complex_data_image))), axes=2) / scaling
    complex_data_correct_image = np.fft.fftshift(np.fft.ifftn(np.fft.fftshift(complex_data_kspace))) * scaling        

    # compute magnitude and phase of complex image
    phase_data = np.angle(complex_data_correct_image)
    mag_data = np.abs(complex_data_correct_image)

    # create nifti images
    mag_nii = nib.Nifti1Image(mag_data, real_nii.affine, real_nii.header)
    phase_nii = nib.Nifti1Image(phase_data, real_nii.affine, real_nii.header)
    
    # determine filenames
    extension = ".".join(real_path.split('.')[1:])
    mag_path = f"{real_path.split('.')[0]}_mag.{extension}"
    phase_path =  f"{real_path.split('.')[0]}_phase.{extension}"
    
    # save new images to file
    nib.save(mag_nii, mag_path)
    nib.save(phase_nii, phase_path)
    
    # delete original images
    if delete_originals:
        os.remove(real_path)
        os.remove(imag_path)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Fix GE data"
    )

    parser.add_argument(
        'in_files',
        type=str,
        nargs=2,
        help='Input NIfTI files to correct - either magnitude and phase, OR real and imaginary'
    )

    parser.add_argument(
        '--is_complex',
        type=bool,
        help='Indicates that the input data are real and imaginary components rather than magnitude and phase'
    )

    parser.add_argument(
        '--delete_originals',
        type=bool,
        help='Indicates that the original files should be deleted and replaced by corrected ones'
    )

    args = parser.parse_args()

    if args.is_complex: fix_ge_complex(args.in_files[0], args.in_files[1], args.delete_originals)
    else: fix_ge_polar(args.in_files[0], args.in_files[1], args.delete_originals)
    
