#!/usr/bin/env python3

import os
import shutil
from nipype.interfaces.base import  traits, CommandLine, BaseInterfaceInputSpec, TraitedSpec, File, InputMultiPath, OutputMultiPath
from scripts.qsmxt_functions import extend_fname
from scripts import qsmxt_functions
import nibabel as nib
import numpy as np
    
def merge_multi_echo(in_paths, out_path):
    if len(in_paths) == 1: return in_paths[0]
    image4d = np.stack([nib.load(f).get_fdata() for f in in_paths], -1)
    sample_nii = nib.load(in_paths[0])
    nib.save(nib.nifti1.Nifti1Image(image4d, affine=sample_nii.affine, header=sample_nii.header), out_path)
    return out_path

def split_multi_echo(in_path, out_paths):
    image4d = nib.load(in_path).get_fdata()
    if len(image4d.shape) != 4 and len(out_paths) == 1:
        shutil.copy(in_path, out_paths[0])
        return out_paths
    sample_nii = nib.load(in_path)

    for i, out_path in enumerate(out_paths):
        echo_data = image4d[..., i]
        echo_nii = nib.nifti1.Nifti1Image(echo_data, affine=sample_nii.affine, header=sample_nii.header)
        nib.save(echo_nii, out_path)

    return out_paths

def wrap_phase(phase_path):
    phase_nii = nib.load(phase_path)
    phase = phase_nii.get_fdata()
    phase_wrapped = (phase + np.pi) % (2 * np.pi) - np.pi
    phase_wrapped_path = extend_fname(phase_path, "_wrapped", ext="nii", out_dir=os.getcwd())
    nib.save(img=nib.Nifti1Image(dataobj=phase_wrapped, affine=phase_nii.affine, header=phase_nii.header), filename=phase_wrapped_path)
    return phase_wrapped_path

class RomeoB0InputSpec(BaseInterfaceInputSpec):
    # required inputs
    phase = InputMultiPath(mandatory=True, exists=True)
    magnitude = InputMultiPath(mandatory=False, exists=True)
    TE = traits.ListFloat(mandatory=True, argstr="-t '[%s]'")
    
    # automatically filled
    combine_phase = File(exists=True, argstr="--phase %s", position=0)
    combine_mag = File(mandatory=False, exists=True, argstr="--mag %s", position=1)
    

class RomeoB0OutputSpec(TraitedSpec):
    frequency = File(exists=True)
    phase_unwrapped = OutputMultiPath(File(exists=True))
    #phase_unwrapped = File(exists=True)
    #phase_wrapped = File(exists=True)

class RomeoB0Interface(CommandLine):
    input_spec = RomeoB0InputSpec
    output_spec = RomeoB0OutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "romeo_unwrapping.jl -B --no-rescale --phase-offset-correction")

    def _run_interface(self, runtime):
        self.inputs.combine_phase = merge_multi_echo(self.inputs.phase, os.path.join(os.getcwd(), "multi-echo-phase.nii"))
        if self.inputs.combine_mag:
            self.inputs.combine_mag = merge_multi_echo(self.inputs.magnitude, os.path.join(os.getcwd(), "multi-echo-mag.nii"))
        self.inputs.TE = [TE*1000 for TE in self.inputs.TE]
        return super(RomeoB0Interface, self)._run_interface(runtime)
        
    def _list_outputs(self):
        outputs = self.output_spec().trait_get()
        
        # rename B0.nii to suitable output name
        frequency_path = extend_fname(self.inputs.phase[0], "_romeo-b0map", ext="nii", out_dir=os.getcwd())
        os.rename(os.path.join(os.getcwd(), "B0.nii"), frequency_path)
        outputs['frequency'] = frequency_path

        # rename unwrapped.nii to suitable output name
        outputs['phase_unwrapped'] = split_multi_echo("unwrapped.nii", [extend_fname(f, "_romeo-unwrapped", ext="nii", out_dir=os.getcwd()) for f in self.inputs.phase])

        return outputs

