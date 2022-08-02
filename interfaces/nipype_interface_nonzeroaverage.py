#!/usr/bin/env python3
import os
import nibabel as nib
import numpy as np
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, InputMultiPath, File


def save_nii(data, file_path, nii_like):
    nib.save(nib.nifti1.Nifti1Image(data, affine=nii_like.affine, header=nii_like.header), file_path)


def nonzero_average(in_files, save_result=True):
    data = []
    for in_nii_file in in_files:
        in_nii = nib.load(in_nii_file)
        in_data = in_nii.get_fdata()
        data.append(in_data)
    try:
        data = np.array(data)
        mask = abs(data) >= 0.0001
    except ValueError:
        sizes = [x.shape for x in data]
        raise ValueError(f"Tried to average files of incompatible dimensions; {sizes}")
    final = np.divide(data.sum(0), mask.sum(0), out=np.zeros_like(data.sum(0)), where=mask.sum(0)!=0)
    #final = data.sum(0) / mask.sum(0)
    if save_result:
        filename = f"{os.path.abspath(os.path.split(in_files[0])[1].split('.')[0])}_average.nii"
        save_nii(final, filename, in_nii)
        return filename
    return final


class NonzeroAverageInputSpec(BaseInterfaceInputSpec):
    in_files = InputMultiPath(mandatory=True, exists=True)


class NonzeroAverageOutputSpec(TraitedSpec):
    out_file = File(exists=True)


class NonzeroAverageInterface(SimpleInterface):
    input_spec = NonzeroAverageInputSpec
    output_spec = NonzeroAverageOutputSpec

    def _run_interface(self, runtime):
        self._results['out_file'] = nonzero_average(self.inputs.in_files)
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

    args = parser.parse_args()
    qsm_final = nonzero_average(args.in_files, False);
    save_nii(qsm_final, args.out_file, nib.load(args.in_files[0]))
