#!/usr/bin/env python

import nibabel as nib
import numpy as np
import argparse
import os
from qsmxt.scripts.qsmxt_functions import extend_fname
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, File, traits

def reference_susceptibility(in_qsm, in_seg=None, in_seg_values=None, out_qsm=None):
    qsm_nii = nib.load(in_qsm)
    qsm = np.array(qsm_nii.get_fdata(), dtype=np.float64)

    full_mask = abs(qsm) >= 5e-5
    if in_seg and in_seg_values:
        seg = np.array(nib.load(in_seg).get_fdata(), dtype=np.int64)
        ref_mask = np.zeros_like(seg, dtype=np.int64)
        for value in in_seg_values:
            ref_mask = np.logical_or(ref_mask, seg.astype(np.int64) == np.int64(value))
    else:
        ref_mask = full_mask
    
    nib.save(nib.Nifti1Image(ref_mask, qsm_nii.affine, qsm_nii.header), extend_fname(in_qsm, "_ref_mask", out_dir=os.getcwd()))

    qsm[full_mask] = qsm[full_mask] - np.mean(qsm[ref_mask == 1])

    out_qsm = out_qsm or extend_fname(in_qsm, "_ref", out_dir=os.getcwd())
    nib.save(nib.Nifti1Image(qsm, qsm_nii.affine, qsm_nii.header), out_qsm)
    return out_qsm


class ReferenceQSMInputSpec(BaseInterfaceInputSpec):
    in_qsm = File(mandatory=True, exists=True)
    in_seg = File(mandatory=False, exists=True)
    in_seg_values = traits.Either(traits.ListInt, None, mandatory=False, default=None)


class ReferenceQSMOutputSpec(TraitedSpec):
    out_file = File(exists=True)


class ReferenceQSMInterface(SimpleInterface):
    input_spec = ReferenceQSMInputSpec
    output_spec = ReferenceQSMOutputSpec

    def _run_interface(self, runtime):
        self._results['out_file'] = reference_susceptibility(
            in_qsm=self.inputs.in_qsm,
            in_seg=self.inputs.in_seg,
            in_seg_values=self.inputs.in_seg_values
        )
        return runtime


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Reference a susceptibility map to the mean of the values within a segmented region.")
    parser.add_argument("in_qsm", help="Path to the input susceptibility map.")
    parser.add_argument("--in_seg", help="Optional path to the input segmentation. By default, any non-zero value will be considered a part of the mask.")
    parser.add_argument("--in_seg_values", default=[1], nargs='+', help="Optional values to use for the segmentation mask.")
    parser.add_argument("out_qsm", help="Path to save the output referenced susceptibility map.")
    args = parser.parse_args()

    reference_susceptibility(args.in_qsm, args.in_seg, args.in_seg_values, args.out_qsm)

