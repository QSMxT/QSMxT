#!/usr/bin/env python

import os
import nibabel as nib
import numpy as np
from scipy.stats import norm
from scipy.ndimage import binary_fill_holes, binary_dilation, binary_erosion
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, File, traits, InputMultiPath, OutputMultiPath

# === HELPER FUNCTIONS ===

def _histogram(image_histogram, normalize):
    hist, bin_edges = np.histogram(image_histogram, bins=np.arange(image_histogram.min(), image_histogram.max()))
    bin_centers = (bin_edges[:-1] + bin_edges[1:]) / 2.
    std = np.std(image_histogram)
    mean = np.mean(image_histogram)
    if normalize:
        hist = hist / np.sum(hist)
    return hist, bin_centers, mean, std

def _gaussian_threshold(image_histogram):
    hist, bin, mu, std = _histogram(image_histogram, True)
    normal_distribution = norm.pdf(bin, mu, std)
    difference = [normal_distribution[i] - hist[i] if hist[i]<normal_distribution[i] else 0 for i in range(len(hist)) ]
    maxpoint = max(range(len(difference)), key=difference.__getitem__)
    #threshold_percent = bin[maxpoint]/np.amax(image_histogram)*100
    return bin[maxpoint] #threshold_percent

def _clean_histogram(image_histogram):
    p_lower = np.percentile(image_histogram, 0.05) 
    p_upper = np.percentile(image_histogram, 99.5)
    image_histogram = image_histogram[np.logical_and(image_histogram > p_lower, image_histogram < p_upper)]
    return image_histogram

# === THRESHOLD-BASED MASKING FOR TWO-PASS AND SINGLE-PASS QSM ===
def threshold_masking(in_files, threshold=None, fill_strength=1):
    # load data
    all_niis = [nib.load(in_file) for in_file in in_files]
    all_float_data = [nii.get_fdata() for nii in all_niis]
    image_histogram = np.array([data.flatten() for data in all_float_data])
    
    # calculate gaussian threshold if none given
    if not threshold:
        threshold = _gaussian_threshold(image_histogram)
    else:
        threshold = np.percentile(_clean_histogram(image_histogram), 100-threshold)

    # do masking
    masks = [np.array(data > threshold, dtype=int) for data in all_float_data]

    # erosion and dilation
    for i in range(len(masks)):
        masks[i] = binary_dilation(masks[i]).astype(int)
        masks[i] = binary_erosion(masks[i]).astype(int)

    # hole-filling
    filled_masks = [mask.copy() for mask in masks]
    for i in range(len(masks)):
        for j in range(fill_strength):
            filled_masks[i] = binary_dilation(filled_masks[i]).astype(int)
        
        filled_masks[i] = binary_fill_holes(filled_masks[i]).astype(int)
        
        for j in range(fill_strength):
            filled_masks[i] = binary_erosion(filled_masks[i]).astype(int)

    # determine filenames
    mask_filenames = [f"{os.path.abspath(os.path.split(in_file)[1].split('.')[0])}_mask.nii" for in_file in in_files]
    filled_mask_filenames = [f"{os.path.abspath(os.path.split(in_file)[1].split('.')[0])}_mask_filled.nii" for in_file in in_files]

    for i in range(len(masks)):
        nib.save(
            nib.Nifti1Image(
                dataobj=masks[i],
                header=all_niis[i].header,
                affine=all_niis[i].affine
            ),
            mask_filenames[i]
        )
        nib.save(
            nib.Nifti1Image(
                dataobj=filled_masks[i],
                header=all_niis[i].header,
                affine=all_niis[i].affine
            ),
            filled_mask_filenames[i]
        )

    return mask_filenames, filled_mask_filenames


class MaskingInputSpec(BaseInterfaceInputSpec):
    in_files = InputMultiPath(mandatory=True, exists=True)
    threshold = traits.Int(mandatory=False, default_value=None)
    fill_strength = traits.Int(mandatory=False, default_value=1)


class MaskingOutputSpec(TraitedSpec):
    masks = OutputMultiPath(File(exists=False))
    masks_filled = OutputMultiPath(File(exists=False))


class MaskingInterface(SimpleInterface):
    input_spec = MaskingInputSpec
    output_spec = MaskingOutputSpec

    def _run_interface(self, runtime):
        masks, masks_filled = threshold_masking(self.inputs.in_files, self.inputs.threshold, self.inputs.fill_strength)
        self._results['masks'] = masks
        self._results['masks_filled'] = masks_filled
        return runtime


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()

    parser.add_argument(
        '--in_files',
        nargs='+',
        required=True,
        type=str
    )

    parser.add_argument(
        '--fill_strength',
        type=int,
        default=1
    )

    parser.add_argument(
        '--threshold',
        nargs='?',
        default=None,
        type=int
    )

    args = parser.parse_args()
    mask_files = threshold_masking(args.in_files, args.threshold, args.fill_strength)

