#!/usr/bin/env python

import nibabel as nib
import nilearn.image
import numpy as np
import argparse
import os
from qsmxt.scripts.qsmxt_functions import extend_fname
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, File, traits, InputMultiPath

def resample_to_reference(in_file, ref_file, out_file=None, interpolation='continuous'):
    # Load NIfTI files
    in_nii = nib.load(in_file)
    ref_nii = nib.load(ref_file)

    # Check if the input image is already aligned with the reference image
    if np.array_equal(in_nii.affine, ref_nii.affine):
        return in_file

    # Resample the source image to match the reference image
    resampled_nii = nilearn.image.resample_img(
        in_nii, 
        target_affine=ref_nii.affine, 
        target_shape=np.array(ref_nii.header.get_data_shape()),
        interpolation=interpolation
    )

    # Save the resampled image
    out_file = out_file or extend_fname(in_file, "_resampled", out_dir=os.getcwd())
    nib.save(resampled_nii, out_file)

    return out_file

class ResampleLikeInputSpec(BaseInterfaceInputSpec):
    in_file = File(mandatory=True, exists=True)
    ref_file = InputMultiPath(File, mandatory=True, exists=True)
    interpolation = traits.Enum("continuous", "nearest", usedefault=True, default="continuous")

class ResampleLikeOutputSpec(TraitedSpec):
    out_file = File()

class ResampleLikeInterface(SimpleInterface):
    input_spec = ResampleLikeInputSpec
    output_spec = ResampleLikeOutputSpec

    def _run_interface(self, runtime):
        if isinstance(self.inputs.ref_file, list):
            ref_file = self.inputs.ref_file[0]
        else:
            ref_file = self.inputs.ref_file

        out_file = resample_to_reference(
            in_file=self.inputs.in_file,
            ref_file=ref_file,
            interpolation=self.inputs.interpolation
        )

        if out_file is not None:
            self._results['out_file'] = out_file
        else:
            raise RuntimeError("Resampling failed, out_file is None")

        return runtime



if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Resample a NIfTI image to match the dimensions, resolution, and orientation of a reference image.")
    parser.add_argument("in_file", help="Path to the source NIfTI file to be resampled.")
    parser.add_argument("ref_file", help="Path to the reference NIfTI file.")
    parser.add_argument("out_file", help="Path to save the resampled NIfTI file.")
    parser.add_argument("--interpolation", choices=['nearest', 'continuous'], default='continuous', 
                        help="Type of interpolation for resampling ('nearest' or 'continuous'). Default is 'continuous'.")
    args = parser.parse_args()

    resample_to_reference(args.in_file, args.ref_file, args.out_file, args.interpolation)

