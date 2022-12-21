#!/usr/bin/env python3
import argparse
import os
import nibabel as nib
import numpy as np
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, File


def twopass_nifti(in_file1, in_file2, mask=None, save_result=True, out_name=None):
    in1_nii = nib.load(in_file1)
    in2_nii = nib.load(in_file2)
    if mask: in_mask_nii = nib.load(mask)

    in1_data = in1_nii.get_fdata()
    in2_data = in2_nii.get_fdata()
    if mask: mask_data = in_mask_nii.get_fdata()

    if not mask:
        out_data = in1_data + (in2_data * (abs(in1_data) < 5e-05))
    else:
        out_data = in1_data + (in2_data * np.logical_not(mask_data))

    if save_result:
        if not out_name:
            filename = f"{os.path.splitext(os.path.splitext(os.path.split(in_file1)[1])[0])[0]}_twopass.nii"
            out_name = os.path.join(os.path.abspath(os.curdir), filename)
        nib.save(nib.nifti1.Nifti1Image(out_data, affine=in1_nii.affine, header=in1_nii.header), out_name)
        return out_name

    return out_data


class TwopassNiftiInputSpec(BaseInterfaceInputSpec):
    in_file1 = File(mandatory=True, exists=True)
    in_file2 = File(mandatory=True, exists=True)
    mask = File(mandatory=False, exists=True)


class TwopassNiftiOutputSpec(TraitedSpec):
    out_file = File()


class TwopassNiftiInterface(SimpleInterface):
    input_spec = TwopassNiftiInputSpec
    output_spec = TwopassNiftiOutputSpec

    def _run_interface(self, runtime):
        self._results['out_file'] = twopass_nifti(self.inputs.in_file1, self.inputs.in_file2, self.inputs.mask)
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

    parser.add_argument(
        '--mask',
        type=str,
        default=None
    )

    args = parser.parse_args()
    in1_nii = nib.load(args.in_file1)
    result = twopass_nifti(args.in_file1, args.in_file2, args.mask, save_result=False)
    nib.save(nib.nifti1.Nifti1Image(result, affine=in1_nii.affine, header=in1_nii.header), args.out_file)
