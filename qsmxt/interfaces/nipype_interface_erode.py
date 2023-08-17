#!/usr/bin/env python3
import os
from qsmxt.scripts.qsmxt_functions import extend_fname
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, traits, File

def erosion(in_file, num_erosions=1):
    if num_erosions == 0: return in_file

    import os
    import nibabel as nib
    from scipy.ndimage import binary_erosion
    
    # load data
    nii = nib.load(in_file)
    data = nii.get_fdata()

    # erosions
    for i in range(num_erosions):
        data = binary_erosion(data).astype(int)

    # write to file
    out_file = extend_fname(in_file, "_ero", out_dir=os.getcwd())
    nib.save(nib.Nifti1Image(dataobj=data, header=nii.header, affine=nii.affine), out_file)

    return out_file


class ErosionInputSpec(BaseInterfaceInputSpec):
    in_file = File(mandatory=True, exists=True)
    num_erosions = traits.Int(default=1)


class ErosionOutputSpec(TraitedSpec):
    out_file = File(exists=True)


class ErosionInterface(SimpleInterface):
    input_spec = ErosionInputSpec
    output_spec = ErosionOutputSpec

    def _run_interface(self, runtime):
        out_file = erosion(self.inputs.in_file, self.inputs.num_erosions)
        self._results['out_file'] = out_file
        return runtime


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()

    parser.add_argument(
        'in_file',
        required=True,
        type=str
    )

    parser.add_argument(
        '--num_erosions',
        required=False,
        default=1,
        type=int
    )

    args = parser.parse_args()
    mask_files = erosion(args.in_file, args.num_erosions)

