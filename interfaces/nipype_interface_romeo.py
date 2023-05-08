#!/usr/bin/env python3

import os
from nipype.interfaces.base import  traits, CommandLine, BaseInterfaceInputSpec, TraitedSpec, File, InputMultiPath, OutputMultiPath
from scripts.qsmxt_functions import extend_fname
from scripts import qsmxt_functions
import nibabel as nib
import numpy as np
    
def save_multi_echo(in_files, fn_path):
    image4d = np.stack([nib.load(f).get_fdata() for f in in_files], -1)
    sample_nii = nib.load(in_files[0])
    nib.save(nib.nifti1.Nifti1Image(image4d, affine=sample_nii.affine, header=sample_nii.header), fn_path)
    return fn_path

## Romeo wrapper single-echo (MapNode)
class RomeoInputSpec(BaseInterfaceInputSpec):
    phase = File(mandatory=True, exists=True, argstr="--phase %s")
    #mask = File(mandatory=False, exists=True, argstr="--mask %s")
    magnitude = File(mandatory=False, exists=True, argstr="--mag %s")
    phase_unwrapped = File(name_source=['phase'], name_template='%s_romeo.nii.gz', argstr="--output %s")

class RomeoOutputSpec(TraitedSpec):
    phase_unwrapped = File(exists=True)

class RomeoInterface(CommandLine):
    input_spec = RomeoInputSpec
    output_spec = RomeoOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "romeo_unwrapping.jl --no-rescale")

## Romeo wrapper multi-echo (Node)
class RomeoB0InputSpec(BaseInterfaceInputSpec):
    phase = InputMultiPath(mandatory=True, exists=True)
    magnitude = InputMultiPath(mandatory=True, exists=True)
    mask = InputMultiPath(mandatory=False, exists=True)
    combine_phase = File(exists=True, argstr="--phase %s", position=0)
    combine_mag = File(exists=True, argstr="--mag %s", position=1)
    TE = traits.ListFloat(desc='Echo Time [sec]', mandatory=True, argstr="-t '[%s]'")

class RomeoB0OutputSpec(TraitedSpec):
    frequency = File(exists=True)
    magnitude = File(exists=True)
    mask = File(exists=True)
    # B0s = OutputMultiPath(File(exists=False))

class RomeoB0Interface(CommandLine):
    input_spec = RomeoB0InputSpec
    output_spec = RomeoB0OutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "romeo_unwrapping.jl -B --no-rescale --phase-offset-correction")

    def _run_interface(self, runtime):
        self.inputs.combine_phase = save_multi_echo(self.inputs.phase, os.path.join(os.getcwd(), "multi-echo-phase.nii"))
        self.inputs.combine_mag = save_multi_echo(self.inputs.magnitude, os.path.join(os.getcwd(), "multi-echo-mag.nii"))
        self.inputs.TE = [TE*1000 for TE in self.inputs.TE]
        return super(RomeoB0Interface, self)._run_interface(runtime)
        
    def _list_outputs(self):
        outputs = self.output_spec().get()
        
        # rename B0.nii to suitable output name
        outfile_final = extend_fname(self.inputs.phase[0], "_romeo-b0map", ext="nii")
        os.rename(os.path.join(os.getcwd(), "B0.nii"), outfile_final)
        outputs['frequency'] = outfile_final
        
        # output first-echo magnitude and mask
        outputs['magnitude'] = self.inputs.magnitude[0]
        if self.inputs.mask: outputs['mask'] = self.inputs.mask[0]

        return outputs


