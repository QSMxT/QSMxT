#!/usr/bin/env python3
import nibabel as nib
import numpy as np
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, File


def mgz2nii(in_file, out_file=None):
    if not out_file: out_file = in_file.replace(".mgz", "_nii.nii")
    mgz = nib.load(in_file)

    # for some reason nibabel needs help converting the data type properly to numpy
    dtypes = { # https://surfer.nmr.mgh.harvard.edu/fswiki/FsTutorial/MghFormat
        0 : np.uint8, # UCHAR
        1 : int,      # INT
        3 : float,    # FLOAT
        4 : np.short  # SHORT
    }

    data = np.array(mgz.get_fdata(), dtype=dtypes[int(mgz.header['type'])])
    
    nii = nib.Nifti1Image(data, affine=mgz.affine, header=mgz.header)
    nib.save(nii, out_file)

    return out_file

class Mgz2NiiInputSpec(BaseInterfaceInputSpec):
    in_file = File(mandatory=True, exists=True)


class Mgz2NiiOutputSpec(TraitedSpec):
    out_file = File()


class Mgz2NiiInterface(SimpleInterface):
    input_spec = Mgz2NiiInputSpec
    output_spec = Mgz2NiiOutputSpec

    def _run_interface(self, runtime):
        self._results['out_file'] = mgz2nii(self.inputs.in_file)
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
        nargs='?',
        default=None,
        const=None,
        type=str
    )

    args = parser.parse_args()
    out_file = mgz2nii(args.in_file, args.out_file)
