#!/usr/bin/env python
import argparse
import os
import nibabel as nib
import numpy as np
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, InputMultiPath, File


def save_nii(data, file_path, nii_like):
    nib.save(nib.nifti1.Nifti1Image(
        data, affine=nii_like.affine, header=nii_like.header), file_path)


def nonzero_average(in_files):
    data = []
    for in_nii_file in in_files:
        in_nii = nib.load(file_path)
        in_data = in_nii.get_fdata()
        data.append(in_data)
    data = np.array(data)
    qsm_sum = data.sum(0)
    mask_sum = (data != 0).sum(0)
    qsm_final = np.true_divide(qsm_sum, mask_sum)
    filename = f"{os.path.splitext(os.path.splitext(os.path.split(in_files[0])[1])[0])[0]}_average.nii"
    fullpath = os.path.join(os.path.abspath(os.curdir), filename)
    save_nii(qsm_final, fullpath, in_nii)
    return fullpath


class NonzeroAverageInputSpec(BaseInterfaceInputSpec):
    in_files = InputMultiPath(mandatory=True, exists=True)


class NonzeroAverageOutputSpec(TraitedSpec):
    out_file = File()


class NonzeroAverageInterface(SimpleInterface):
    input_spec = NonzeroAverageInputSpec
    output_spec = NonzeroAverageOutputSpec

    def _run_interface(self, runtime):
        self._results['out_file'] = nonzero_average(self.inputs.in_files)
        return runtime
