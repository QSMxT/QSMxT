#!/usr/bin/env python
import nibabel as nib
import numpy as np
from scipy.stats import norm
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, File, traits, InputMultiPath


def histogram(image_histogram, normalize):
    hist, bin_edges = np.histogram(image_histogram, bins=np.arange(image_histogram.min(), image_histogram.max()))
    bin_centers = (bin_edges[:-1] + bin_edges[1:]) / 2.
    std = np.std(image_histogram)
    mean = np.mean(image_histogram)
    if normalize:
        hist = hist / np.sum(hist)
    return hist, bin_centers, mean, std

def thresholding(in_files, op_string=None):
    image_histogram = []
    for echo_file in in_files:
        in_nii = nib.load(echo_file)
        in_data = in_nii.get_fdata()
        in_data = in_data.flatten()
        image_histogram.append(in_data)

    try:
        image_histogram = np.array(image_histogram)

    except ValueError:
        sizes = [x.shape for x in image_histogram]
        raise ValueError(f"Tried to average files of incompatible dimensions; {sizes}")

    hist, bin, mu, std = histogram(image_histogram, True)
    normal_distribution = norm.pdf(bin, mu, std)
    difference = [normal_distribution[i] - hist[i] if hist[i]<normal_distribution[i] else 0 for i in range(len(hist)) ]
    maxpoint = max(range(len(difference)), key=difference.__getitem__)
    threshold = bin[maxpoint]/np.amax(image_histogram)*100

    op_string =  '-thrp {threshold} -bin -ero'.format(threshold=threshold)
    iter_op_string = [op_string]*len(in_files)
    return iter_op_string


class ThresholdInputSpec(BaseInterfaceInputSpec):
    # in_file = File(mandatory=True, exists=True)
    in_files = InputMultiPath(mandatory=True, exists=True)


class ThresholdOutputSpec(TraitedSpec):
    op_string = traits.List(traits.String())


class ThresholdInterface(SimpleInterface):
    input_spec = ThresholdInputSpec
    output_spec = ThresholdOutputSpec

    def _run_interface(self, runtime):
        self._results['op_string'] = thresholding(self.inputs.in_files)
        return runtime


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()

    parser.add_argument(
        'in_files',
        nargs='+',
        type=str
    )

    parser.add_argument(
        'op_string',
        nargs='?',
        default=None,
        type=str
    )

    args = parser.parse_args()
    op_string = thresholding(args.in_file, args.op_string)
    