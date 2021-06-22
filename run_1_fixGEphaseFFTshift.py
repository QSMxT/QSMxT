#!/usr/bin/env python3
import nibabel as nib
import sys
import math
import numpy as np
import os

print("Number of files: ", len(sys.argv)-1)

for fileIdx in range(1,len(sys.argv)):
        filename = sys.argv[fileIdx]
        print(filename)
        img = nib.load(filename)
        real = img.get_fdata()
        os.remove(filename)

        img = nib.load(filename.replace('run-1','run-2'))
        imag = img.get_fdata()
        os.remove(filename.replace('run-1','run-2'))
        print("incorrect files deleted!!!")
        os.rename(filename.replace('run-1','run-2').replace('nii.gz','json'),filename.replace('nii.gz','json').replace('_magnitude','_phase'))


        complex_data_image = real + 1j * imag
        scaling = np.sqrt(complex_data_image.size)

        complex_data_kspace = np.fft.fftshift (np.fft.fftshift (np.fft.fftn(  np.fft.fftshift(complex_data_image))), axes=2) / scaling

        complex_data_correct_image = np.fft.fftshift(np.fft.ifftn(np.fft.fftshift(complex_data_kspace))) * scaling        

        phase_data = np.angle(complex_data_correct_image)
        mag_data = np.abs(complex_data_correct_image)

        phase_img = nib.Nifti1Image(phase_data, img.affine, img.header)
        mag_img = nib.Nifti1Image(mag_data, img.affine, img.header)

        print("writing corrected phase and magnitude data")
        nib.save(mag_img, filename)
        nib.save(phase_img, filename.replace('_magnitude','_phase'))

