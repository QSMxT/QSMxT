#!/usr/bin/env python3

import os
import nibabel as nib
import numpy as np
from scipy.stats import norm
from scipy.ndimage import binary_fill_holes, binary_dilation, binary_erosion, gaussian_filter, binary_opening
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, File, traits, InputMultiPath, OutputMultiPath

# === HELPER FUNCTIONS ===

def _histogram(image_histogram, normalize):
    bin_edges = np.histogram_bin_edges(image_histogram, bins='fd')
    hist, _ = np.histogram(image_histogram, bins=bin_edges)
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
def threshold_masking(in_files, threshold=None, fill_strength=0):
    # sort input filepaths
    in_files = sorted(in_files)

    # load data
    all_niis = [nib.load(in_file) for in_file in in_files]
    all_float_data = [nii.get_fdata() for nii in all_niis]
    image_histogram = np.array([data.flatten() for data in all_float_data])
    
    # calculate gaussian threshold if none given
    if not threshold: threshold = _gaussian_threshold(image_histogram)

    # do masking
    masks = [np.array(data > threshold, dtype=int) for data in all_float_data]

    # remove noisy background voxels (applied to masks only)
    small_masks = [binary_opening(mask) for mask in masks]

    # hole-filling (applied to filled_masks only)
    filled_masks = [fill_holes_smoothing(mask) for mask in masks]

    # determine filenames
    small_mask_filenames = [f"{os.path.abspath(os.path.split(in_file)[1].split('.')[0])}_mask.nii" for in_file in in_files]
    filled_mask_filenames = [f"{os.path.abspath(os.path.split(in_file)[1].split('.')[0])}_mask_filled.nii" for in_file in in_files]

    for i in range(len(masks)):
        all_niis[i].header.set_data_dtype(np.uint8)
        nib.save(
            nib.Nifti1Image(
                dataobj=small_masks[i],
                header=all_niis[i].header,
                affine=all_niis[i].affine
            ),
            small_mask_filenames[i]
        )
        nib.save(
            nib.Nifti1Image(
                dataobj=filled_masks[i],
                header=all_niis[i].header,
                affine=all_niis[i].affine
            ),
            filled_mask_filenames[i]
        )

    return small_mask_filenames, filled_mask_filenames, threshold

# The smoothing removes background noise and closes small holes
# A smaller threshold grows the mask
def fill_holes_smoothing(mask, sigma=[5,5,5], threshold=0.4):
    smoothed = gaussian_filter(mask * 1.0, sigma, truncate=2.0) # truncate reduces the kernel size: less precise but faster
    return np.array(smoothed > threshold, dtype=int)

# original morphological operation
def fill_holes_morphological(mask, fill_strength=0):
    filled_mask = mask.copy()
    for j in range(fill_strength):
        filled_mask = binary_dilation(filled_mask).astype(int)
    filled_mask = binary_fill_holes(filled_mask).astype(int)
    for j in range(fill_strength):
        filled_mask = binary_erosion(filled_mask).astype(int)
    return filled_mask



class MaskingInputSpec(BaseInterfaceInputSpec):
    in_files = InputMultiPath(mandatory=True, exists=True)
    threshold = traits.Float(mandatory=False, default_value=None)
    fill_strength = traits.Int(mandatory=False, default_value=1)


class MaskingOutputSpec(TraitedSpec):
    masks = OutputMultiPath(File(exists=False))
    masks_filled = OutputMultiPath(File(exists=False))
    threshold = traits.Float()


class MaskingInterface(SimpleInterface):
    input_spec = MaskingInputSpec
    output_spec = MaskingOutputSpec

    def _run_interface(self, runtime):
        masks, masks_filled, threshold = threshold_masking(self.inputs.in_files, self.inputs.threshold, self.inputs.fill_strength)
        self._results['masks'] = masks
        self._results['masks_filled'] = masks_filled
        self._results['threshold'] = threshold
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
        required=False,
        default=0
    )

    parser.add_argument(
        '--threshold',
        nargs='?',
        default=None,
        type=float
    )

    args = parser.parse_args()
    mask_files = threshold_masking(args.in_files, args.threshold, args.fill_strength)

