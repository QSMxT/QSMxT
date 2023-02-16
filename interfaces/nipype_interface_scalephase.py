#!/usr/bin/env python3

from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, File

def scale_to_pi(in_file, out_file=None):
    import os
    from math import pi
    import nibabel as nib
    import numpy as np
    nii = nib.load(in_file)
    data = nii.get_fdata()
    if (np.round(np.min(data), 2)*-1) == np.round(np.max(data), 2) == 3.14:
        return in_file
    data = np.array(np.interp(data, (data.min(), data.max()), (-pi, +pi)), dtype=data.dtype)
    if not out_file: out_file = os.path.abspath(os.path.split(in_file)[1].replace(".nii", "_scaled.nii"))
    nib.save(nib.Nifti1Image(dataobj=data, header=nii.header, affine=nii.affine), out_file)
    return out_file


class ScalePhaseInputSpec(BaseInterfaceInputSpec):
    phase = File(mandatory=True, exists=True)
    

class ScalePhaseOutputSpec(TraitedSpec):
    phase_scaled = File(mandatory=True, exists=True)


class ScalePhaseInterface(SimpleInterface):
    input_spec = ScalePhaseInputSpec
    output_spec = ScalePhaseOutputSpec

    def _run_interface(self, runtime):
        self._results['phase_scaled'] = scale_to_pi(self.inputs.phase)
        return runtime


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()

    parser.add_argument(
        'in_file',
        type=str
    )

    parser.add_argument(
        'out_file',
        type=str
    )

    args = parser.parse_args()
    out_file = scale_to_pi(args.in_file, args.out_file)

