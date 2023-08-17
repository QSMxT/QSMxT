#!/usr/bin/env python3
from qsmxt.scripts.qsmxt_functions import extend_fname
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, File, InputMultiPath

def combine_magnitude(magnitude_files, out_file=None):
    import os
    import nibabel as nib
    import numpy as np
    
    # load data
    nii = nib.load(magnitude_files[0])
    data = np.array([nib.load(magnitude_files[i]).get_fdata() for i in range(len(magnitude_files))])
    data = np.linalg.norm(data, axis=0)

    # write to file
    out_file = out_file or extend_fname(magnitude_files[0], "_combined", out_dir=os.getcwd())
    nib.save(nib.Nifti1Image(dataobj=data, header=nii.header, affine=nii.affine), out_file)

    return out_file


class CombineMagnitudeInputSpec(BaseInterfaceInputSpec):
    magnitude = InputMultiPath(mandatory=True, exists=True)


class CombineMagnitudeOutputSpec(TraitedSpec):
    magnitude_combined = File(exists=True)


class CombineMagnitudeInterface(SimpleInterface):
    input_spec = CombineMagnitudeInputSpec
    output_spec = CombineMagnitudeOutputSpec

    def _run_interface(self, runtime):
        magnitude_combined = combine_magnitude(self.inputs.magnitude)
        self._results['magnitude_combined'] = magnitude_combined
        return runtime


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()

    parser.add_argument(
        'magnitude',
        required=True,
        nargs='+',
        type=str
    )

    parser.add_argument(
        'out_file',
        required=False,
        default=None,
        type=str
    )

    args = parser.parse_args()
    mask_files = combine_magnitude(args.magnitude, args.out_file)

