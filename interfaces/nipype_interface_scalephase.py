#!/usr/bin/env python3

from math import pi
import os
import nibabel as nib
import numpy as np
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, File

def scale_to_pi(in_file):
    nii = nib.load(in_file)
    data = nii.get_fdata()
    data = np.array(np.interp(data, (data.min(), data.max()), (-pi, +pi)), dtype=data.dtype)
    out_file = os.path.abspath(os.path.split(in_file)[1].replace(".nii", "_scaled.nii"))
    nib.save(nib.Nifti1Image(dataobj=data, header=nii.header, affine=nii.affine), out_file)
    return out_file


class ScalePhaseInputSpec(BaseInterfaceInputSpec):
    in_file = File(mandatory=True, exists=True)
    

class ScalePhaseOutputSpec(TraitedSpec):
    out_file = File(mandatory=True, exists=True)


class ScalePhaseInterface(SimpleInterface):
    input_spec = ScalePhaseInputSpec
    output_spec = ScalePhaseOutputSpec

    def _run_interface(self, runtime):
        self._results['out_file'] = scale_to_pi(self.inputs.in_file)
        return runtime


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()

    parser.add_argument(
        'in_file',
        required=True,
        type=str
    )

    args = parser.parse_args()
    out_file = scale_to_pi(args.in_file)

