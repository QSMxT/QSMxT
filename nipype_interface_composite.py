#!/usr/bin/env python
import argparse
import os
import nibabel as nib
import numpy as np
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, File


def composite_nifti(in_file1, in_file2, save_result=True):
    in1_nii = nib.load(in_file1)
    in2_nii = nib.load(in_file2)

    in1_data = in1_nii.get_fdata()
    in2_data = in2_nii.get_fdata()

    out_data = in1_data + in2_data * (in1_data == 0)

    if save_result:
        filename = f"{os.path.splitext(os.path.splitext(os.path.split(in_file1)[1])[0])[0]}_composite.nii"
        fullpath = os.path.join(os.path.abspath(os.curdir), filename)
        nib.save(nib.nifti1.Nifti1Image(out_data, affine=in1_nii.affine, header=in1_nii.header), fullpath)
        return fullpath

    return out_data


class CompositeNiftiInputSpec(BaseInterfaceInputSpec):
    in_file1 = File(mandatory=True, exists=True)
    in_file2 = File(mandatory=True, exists=True)


class CompositeNiftiOutputSpec(TraitedSpec):
    out_file = File()


class CompositeNiftiInterface(SimpleInterface):
    input_spec = CompositeNiftiInputSpec
    output_spec = CompositeNiftiOutputSpec

    def _run_interface(self, runtime):
        self._results['out_file'] = composite_nifti(self.inputs.in_file1, self.inputs.in_file2)
        return runtime


if __name__ == "__main__":
    parser = argparse.ArgumentParser()

    parser.add_argument(
        'in_file1',
        type=str
    )

    parser.add_argument(
        'in_file2',
        type=str
    )

    parser.add_argument(
        'out_file',
        type=str
    )

    args = parser.parse_args()
    in1_nii = nib.load(args.in_file1)
    result = composite_nifti(args.in_file1, args.in_file2, save_result=False)
    nib.save(nib.nifti1.Nifti1Image(result, affine=in1_nii.affine, header=in1_nii.header), args.out_file)
