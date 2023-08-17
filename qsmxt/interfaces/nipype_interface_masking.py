#!/usr/bin/env python3

import os
import nibabel as nib
import numpy as np
from scipy.stats import norm
from scipy.ndimage import binary_fill_holes, binary_dilation, binary_erosion, gaussian_filter, binary_opening, convolve
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, File, traits, InputMultiPath, OutputMultiPath
from qsmxt.scripts.qsmxt_functions import extend_fname
from skimage import filters

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
def threshold_masking(
        in_files,
        bet_masks=None,
        user_threshold=None,
        threshold_algorithm='gaussian',
        threshold_algorithm_factor=1.0,
        filling_algorithm='both',
        num_erosions=0,
        mask_suffix="_mask",
        fill_masks=False
    ):
    
    # sort input filepaths
    in_files = sorted(in_files)

    # load data
    all_niis = [nib.load(in_file) for in_file in in_files]
    all_float_data = [nii.get_fdata() for nii in all_niis]
    
    # calculate gaussian threshold if none given
    def get_threshold(data):
        if not user_threshold:
            image_histogram = np.array(data.flatten())
            if threshold_algorithm == 'gaussian':
                threshold = _gaussian_threshold(image_histogram)
            else:
                threshold = filters.threshold_otsu(image_histogram)
            threshold *= threshold_algorithm_factor
        elif round(user_threshold) == user_threshold: # user-defined absolute threshold
            threshold = user_threshold
        else: # user-defined percentage threshold
            data_range = np.max(np.array(all_float_data)) - np.min(np.array(all_float_data))
            threshold = np.min(np.array(all_float_data)) + (user_threshold * data_range)
        return threshold

    # do masking
    thresholds = [get_threshold(data) for data in all_float_data]
    masks = [np.array(all_float_data[i] > thresholds[i], dtype=int) for i in range(len(all_float_data))]
    masks = [fill_small_holes(mask) for mask in masks]
    if fill_masks:
        if filling_algorithm in ['gaussian', 'both']:
            masks = [fill_holes_smoothing(mask) for mask in masks]
        if filling_algorithm in ['morphological', 'both']:
            masks = [fill_holes_morphological(mask) for mask in masks]
        if num_erosions:
            masks = [np.array(binary_erosion(mask, iterations=num_erosions), dtype=int) for mask in masks]
    else:
        # clean up the mask
        masks = [binary_opening(mask) for mask in masks]

        # erode the mask without expanding holes
        if num_erosions:
            # use a bet masks to inform this, if available
            if bet_masks:
                bet_masks = [nib.load(bet_mask).get_fdata() for bet_mask in bet_masks]
                masks_filled = [np.array((masks[i] + bet_masks[i]) >= 1, dtype=int) for i in range(len(masks))]
            else:
                masks_filled = [fill_holes_morphological(mask) for mask in masks]

            # erode the mask without expanding holes
            masks_filled_ero = [np.array(binary_erosion(mask, iterations=num_erosions), dtype=int) for mask in masks_filled]
            
            holes = [np.array((masks_filled_ero[i] - masks[i]) == 1, int) for i in range(len(masks))]

            masks = [np.array((masks_filled_ero[i] - holes[i]) >= 1, dtype=int) for i in range(len(masks))]

    # determine filenames
    mask_filenames = [extend_fname(in_file, mask_suffix, out_dir=os.getcwd()) for in_file in in_files]

    for i in range(len(masks)):
        # set mask datatype to uint8
        all_niis[i].header.set_data_dtype(np.uint8)

        # add threshold information to nifti description
        threshold_description = f"Threshold = {round(thresholds[i], 3)}"
        description = str(all_niis[i].header['descrip'].astype(str)).strip()
        if description == "":
            all_niis[i].header['descrip'] = threshold_description
        else:
            all_niis[i].header['descrip'] = f"{description}; {threshold_description}"
        
        # save mask to file
        nib.save(
            nib.Nifti1Image(
                dataobj=masks[i],
                header=all_niis[i].header,
                affine=all_niis[i].affine
            ),
            mask_filenames[i]
        )

    return mask_filenames, thresholds

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

def fill_small_holes(mask):
    mask = mask.copy()
    kernel = np.array([
        [[1, 1, 1], [1, 1, 1], [1, 1, 1]],
        [[1, 1, 1], [1, 0, 1], [1, 1, 1]],
        [[1, 1, 1], [1, 1, 1], [1, 1, 1]]
    ])
    weights = convolve(mask, kernel, mode='constant', cval=0.0)
    mask[weights >= 27 - 2] = 1
    return mask

class MaskingInputSpec(BaseInterfaceInputSpec):
    in_files = InputMultiPath(mandatory=True, exists=True)
    bet_masks = InputMultiPath(mandatory=False, exists=True)
    threshold = traits.Float(mandatory=False, default_value=None)
    fill_masks = traits.Bool(mandatory=False, default_value=True)
    mask_suffix = traits.String(mandatory=False, value="_mask")
    threshold_algorithm = traits.String(mandatory=False, value="otsu")
    threshold_algorithm_factor = traits.Float(mandatory=False, default_value=1.0)
    filling_algorithm = traits.String(mandatory=False, value='both')
    num_erosions = traits.Int(mandatory=False, default_value=0)


class MaskingOutputSpec(TraitedSpec):
    mask = OutputMultiPath(File(exists=False))
    threshold = traits.List()


class MaskingInterface(SimpleInterface):
    input_spec = MaskingInputSpec
    output_spec = MaskingOutputSpec

    def _run_interface(self, runtime):
        mask, threshold = threshold_masking(
            in_files=self.inputs.in_files,
            bet_masks=self.inputs.bet_masks,
            user_threshold=self.inputs.threshold,
            threshold_algorithm=self.inputs.threshold_algorithm,
            threshold_algorithm_factor=self.inputs.threshold_algorithm_factor,
            filling_algorithm=self.inputs.filling_algorithm,
            num_erosions=self.inputs.num_erosions,
            mask_suffix=f"_{self.inputs.mask_suffix}",
            fill_masks=self.inputs.fill_masks,
        )
        self._results['mask'] = mask
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
        '--threshold_value',
        type=float,
        default=None,
        help='Masking threshold for when --masking_algorithm is set to threshold. Values between 0 and 1'+
             'represent a percentage of the multi-echo input range. Values greater than 1 represent an '+
             'absolute threshold value. Lower values will result in larger masks. If no threshold is '+
             'provided, the --threshold_algorithm is used to select one automatically.'
    )

    parser.add_argument(
        '--threshold_algorithm',
        default='otsu',
        choices=['otsu', 'gaussian'],
        help='Algorithm used to select a threshold for threshold-based masking if --threshold_value is '+
             'left unspecified. The gaussian method is based on doi:10.1016/j.compbiomed.2012.01.004 '+
             'from Balan AGR. et al. The otsu method is based on doi:10.1109/TSMC.1979.4310076 from Otsu '+
             'et al.'
    )

    parser.add_argument(
        '--filling_algorithm',
        default='both',
        choices=['morphological', 'gaussian', 'both', 'none'],
        help='Algorithm used to fill holes for threshold-based masking. By default, a gaussian smoothing '+
             'operation is applied first prior to a morphological hole-filling operation. Note that gaussian '+
             'smoothing may fill some unwanted regions (e.g. connecting the skull and brain tissue), whereas '+
             'morphological hole-filling alone may fail to fill desired regions if they are not fully enclosed.'
    )

    parser.add_argument(
        '--num_erosions',
        default=1,
        type=int,
        help='Number of times to erode the mask.'
    )

    parser.add_argument(
        '--threshold_algorithm_factor',
        default=1.0,
        type=float,
        help='Factor to multiply the algorithmically-determined threshold by. Larger factors will create '+
             'smaller masks.'
    )


    args = parser.parse_args()
    mask_files = threshold_masking(
        in_files=args.in_files,
        user_threshold=args.threshold_value,
        threshold_algorithm=args.threshold_algorithm,
        threshold_algorithm_factor=args.threshold_algorithm_factor,
        filling_algorithm=args.filling_algorithm,
        num_erosions=args.num_erosions,
        fill_masks=args.filling_algorithm != 'none'
    )

