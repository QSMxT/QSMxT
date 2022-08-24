#!/usr/bin/env python3
import nibabel as nib
import sys
import math
import numpy as np
import os

print("Number of files: ", len(sys.argv)-1)

for fileIdx in range(1, len(sys.argv), 2):
        filename_mag = sys.argv[fileIdx]
        print(filename_mag)
        img = nib.load(filename_mag)
        mag = img.get_fdata()

        filename_phase = sys.argv[fileIdx+1]
        print(filename_phase)
        img = nib.load(filename_phase)
        phase = img.get_fdata()

        phase = phase / 4096 * np.pi

        os.remove(filename_phase)

        complex_data_image = mag * (np.cos(phase) + 1j * np.sin(phase))


        # debug
        # uncorrected_phase_data = np.angle(complex_data_image)

        # uncorrected_phase_img = nib.Nifti1Image(uncorrected_phase_data, img.affine, img.header)

        # print("writing uncorrected phase")
        # filename_phase = filename_phase[:-4]
        # nib.save(uncorrected_phase_img, filename_phase+'uncorrected.nii')


        scaling = np.sqrt(complex_data_image.size)

        complex_data_kspace = np.fft.fftshift (np.fft.fftshift (np.fft.fftn(  np.fft.fftshift(complex_data_image))), axes=2) / scaling

        complex_data_correct_image = np.fft.fftshift(np.fft.ifftn(np.fft.fftshift(complex_data_kspace))) * scaling        

        phase_data = np.angle(complex_data_correct_image)

        phase_img = nib.Nifti1Image(phase_data, img.affine, img.header)

        print("writing corrected phase")
        nib.save(phase_img, filename_phase)

