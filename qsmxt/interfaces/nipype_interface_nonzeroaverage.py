#!/usr/bin/env python3

from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, InputMultiPath, File
from qsmxt.scripts.qsmxt_functions import extend_fname


def nonzero_average(in_files, mask_files=None, out_file=True):
    import os
    import nibabel as nib
    import numpy as np

    if len(in_files) == 1: return in_files[0]

    data = []
    for in_data_file in in_files:
        in_data_nii = nib.load(in_data_file)
        in_data = in_data_nii.get_fdata()
        data.append(in_data)
    data = np.array(data)

    if mask_files:
        mask = []
        for in_mask_file in mask_files:
            in_mask_nii = nib.load(in_mask_file)
            in_data = in_mask_nii.get_fdata()
            mask.append(in_data)
        mask = np.array(mask, dtype=int)
        mask *= abs(data) >= 5e-5
    else:
        mask = abs(data) >= 5e-5

    mask_sum = mask.sum(0)
    
    try:
        final = np.where(mask_sum == 0, 0, data.sum(0) / mask_sum)
    except ValueError:
        raise ValueError(f"Tried to average files of incompatible dimensions; data.shape[..]={[x.shape for x in data]}")

    if isinstance(out_file, bool):
        filename = extend_fname(in_files[0], "_average", out_dir=os.getcwd())
        nib.save(nib.nifti1.Nifti1Image(final, affine=in_data_nii.affine, header=in_data_nii.header), filename)
        return filename
    elif isinstance(out_file, str):
        nib.save(nib.nifti1.Nifti1Image(final, affine=in_data_nii.affine, header=in_data_nii.header), out_file)
        return out_file

    return final


class NonzeroAverageInputSpec(BaseInterfaceInputSpec):
    in_files = InputMultiPath(mandatory=True, exists=True)
    in_masks = InputMultiPath(mandatory=False, exists=True)


class NonzeroAverageOutputSpec(TraitedSpec):
    out_file = File(exists=True)


class NonzeroAverageInterface(SimpleInterface):
    input_spec = NonzeroAverageInputSpec
    output_spec = NonzeroAverageOutputSpec

    def _run_interface(self, runtime):
        self._results['out_file'] = nonzero_average(self.inputs.in_files, self.inputs.in_masks)
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
        'out_file',
        type=str
    )

    parser.add_argument(
        '--in_masks',
        nargs='*',
        default=None,
        type=str
    )

    args = parser.parse_args()
    qsm_final = nonzero_average(args.in_files, args.in_masks, args.out_file)

