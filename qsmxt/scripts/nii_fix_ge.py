#!/usr/bin/env python3

from qsmxt.scripts.qsmxt_functions import extend_fname, get_fname
import nibabel as nib
import numpy as np
import argparse
import os
import json

from qsmxt.scripts.qsmxt_functions import extend_fname

def load_json(path):
    with open(path, encoding='utf-8') as f:
        j = json.load(f)
    return j

def fix_ge_polar(mag_path, phase_path, delete_originals=True, acquisition_plane='axial'):
    if acquisition_plane == 'sagittal':
        axes = 0
    elif acquisition_plane == 'coronal':
        axes = 1
    elif acquisition_plane == 'axial':
        axes = 2

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
    complex_data_kspace = np.fft.fftshift (np.fft.fftshift (np.fft.fftn(  np.fft.fftshift(complex_data_image))), axes=axes) / scaling
    complex_data_correct_image = np.fft.fftshift(np.fft.ifftn(np.fft.fftshift(complex_data_kspace))) * scaling        

    # compute corrected phase image
    phase_corr_data = np.angle(complex_data_correct_image) * -1.0 # GE uses different handed-ness requiring inverting

    # create nifti image
    phase_nii.header.set_data_dtype(float)
    phase_corr_nii = nib.Nifti1Image(phase_corr_data, phase_nii.affine, phase_nii.header)
    
    # determine filename
    phase_corr_path = extend_fname(phase_path, "_corrected")

    # save new images to file
    nib.save(phase_corr_nii, phase_corr_path)

    # delete original images and rename if necessary
    if delete_originals:
        os.remove(phase_path)
        os.rename(phase_corr_path, phase_path)

    # return new file path
    return phase_path


def fix_ge_complex(real_nii_path, imag_nii_path, out_mag_path=None, out_phase_path=None, delete_originals=True, acquisition_plane='axial'):
    if acquisition_plane == 'sagittal':
        axes = 0
    elif acquisition_plane == 'coronal':
        axes = 1
    elif acquisition_plane == 'axial':
        axes = 2

    # ensure paths are absolute
    real_nii_path = os.path.abspath(real_nii_path)
    imag_nii_path = os.path.abspath(imag_nii_path)
    real_json_path = f"{get_fname(real_nii_path)}.json"
    imag_json_path = f"{get_fname(imag_nii_path)}.json"

    if out_mag_path is None:
        if 'part-real' in real_nii_path:
            out_mag_path = real_nii_path.replace('part-real', 'part-mag')
        elif '_real' in real_nii_path:
            out_mag_path = real_nii_path.replace('_real', '_mag')
        else:
            out_mag_path = extend_fname(real_nii_path, "_mag")
    
    if out_phase_path is None:
        if 'part-real' in real_nii_path:
            out_phase_path = real_nii_path.replace('part-real', 'part-phase')
        elif '_real' in real_nii_path:
            out_phase_path = real_nii_path.replace('_real', '_phase')
        else:
            out_phase_path = extend_fname(real_nii_path, "_phase")

    mag_json_path = f"{get_fname(out_mag_path)}.json"
    phase_json_path = f"{get_fname(out_phase_path)}.json"

    # load real data
    real_nii = nib.load(real_nii_path)
    real_data = real_nii.get_fdata()
    
    # load imaginary data
    imag_nii = nib.load(imag_nii_path)
    imag_data = imag_nii.get_fdata()

    # compute complex result in the image domain
    complex_data_image = real_data + 1j * imag_data
    scaling = np.sqrt(complex_data_image.size)
    complex_data_kspace = np.fft.fftshift(np.fft.fftshift (np.fft.fftn(  np.fft.fftshift(complex_data_image))), axes=axes) / scaling
    complex_data_correct_image = np.fft.fftshift(np.fft.ifftn(np.fft.fftshift(complex_data_kspace))) * scaling        

    # compute magnitude and phase of complex image
    phase_data = np.angle(complex_data_correct_image) * -1.0 # GE uses different handed-ness requiring inverting
    mag_data = np.abs(complex_data_correct_image)

    # create nifti images
    mag_nii = nib.Nifti1Image(mag_data, real_nii.affine, real_nii.header)
    phase_nii = nib.Nifti1Image(phase_data, real_nii.affine, real_nii.header)
    
    # save new images to file
    nib.save(mag_nii, out_mag_path)
    nib.save(phase_nii, out_phase_path)
    
    # create new json headers
    if os.path.exists(real_json_path):
        mag_json_data = load_json(real_json_path)
        mag_json_data["ImageType"] = ["MAGNITUDE" if x in ["REAL", "IMAGINARY"] else x for x in mag_json_data["ImageType"]]
        with open(mag_json_path, 'w') as mag_json:
            json.dump(mag_json_data, mag_json)

        phase_json_data = load_json(real_json_path)
        phase_json_data["ImageType"] = ["PHASE" if x in ["REAL", "IMAGINARY"] else x for x in phase_json_data["ImageType"]]
        with open(phase_json_path, 'w') as phase_json:
            json.dump(phase_json_data, phase_json)

    # delete originals
    if delete_originals:
        os.remove(real_nii_path)
        os.remove(imag_nii_path)
        os.remove(real_json_path)
        os.remove(imag_json_path)

    # return new file paths
    return out_mag_path, out_phase_path

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
    
