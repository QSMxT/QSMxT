#!/usr/bin/env python3

import os, re
from nipype.interfaces.base import  traits, CommandLine, BaseInterfaceInputSpec, TraitedSpec, File, InputMultiPath, OutputMultiPath
from scripts import qsmxt_functions
import nibabel as nib
import numpy as np

def B0_unit_convert(b0map_path, te):
    from math import pi
    
    b0map_nii = nib.load(b0map_path)
    b0map = b0map_nii.get_fdata()
    
    phase_unwrapped = b0map * (2*pi * te) / (10**3)
    phase_wrapped = (phase_unwrapped + np.pi) % (2 * np.pi) - np.pi
    
    out_file_unwrapped = os.path.abspath(os.path.split(b0map_path)[1].replace(".nii", "_to-phase-unwrapped.nii"))
    out_file_wrapped = os.path.abspath(os.path.split(b0map_path)[1].replace(".nii", "_to-phase-wrapped.nii"))
    
    nib.save(nib.Nifti1Image(dataobj=phase_unwrapped, header=b0map_nii.header, affine=b0map_nii.affine), out_file_unwrapped)
    nib.save(nib.Nifti1Image(dataobj=phase_wrapped, header=b0map_nii.header, affine=b0map_nii.affine), out_file_wrapped)
    
    return out_file_wrapped, out_file_unwrapped

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
    TE = traits.ListFloat(desc='Echo Time [sec]', mandatory=True, argstr="-t [%s]")

class RomeoB0OutputSpec(TraitedSpec):
    frequency = File('B0.nii', exists=True)
    phase_wrapped = File(exists=True)
    phase_unwrapped = File(exists=True)
    magnitude = File(exists=True)
    mask = File(exists=True)
    TE = traits.Float()
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
        
        outfile_final = os.path.join(os.getcwd(), os.path.split(self.inputs.phase[0])[1].split(".")[0] + "_romeo-combined.nii")
        os.rename(os.path.join(os.getcwd(), "B0.nii"), outfile_final)
        outputs['frequency'] = outfile_final
        
        outputs['phase_wrapped'], outputs['phase_unwrapped'] = B0_unit_convert(outfile_final, np.min(self.inputs.TE))
        outputs['magnitude'] = self.inputs.magnitude[0]

        outputs['TE'] = np.min(self.inputs.TE)/1000
        if self.inputs.mask: outputs['mask'] = self.inputs.mask[0]

        return outputs
    
def save_multi_echo(in_files, fn_path):
    image4d = np.stack([nib.load(f).get_fdata() for f in in_files], -1)
    sample_nii = nib.load(in_files[0])
    nib.save(nib.nifti1.Nifti1Image(image4d, affine=sample_nii.affine, header=sample_nii.header), fn_path)
    return fn_path


if __name__ == "__main__":
    combine = RomeoB0Interface(
        phase=[
            '/neurodesktop-storage/data/bids-osf/bids/sub-1/ses-1/anat/sub-1_ses-1_run-01_echo-01_part-phase_MEGRE.nii.gz',
            '/neurodesktop-storage/data/bids-osf/bids/sub-1/ses-1/anat/sub-1_ses-1_run-01_echo-02_part-phase_MEGRE.nii.gz',
            '/neurodesktop-storage/data/bids-osf/bids/sub-1/ses-1/anat/sub-1_ses-1_run-01_echo-03_part-phase_MEGRE.nii.gz',
            '/neurodesktop-storage/data/bids-osf/bids/sub-1/ses-1/anat/sub-1_ses-1_run-01_echo-04_part-phase_MEGRE.nii.gz'
        ],
        magnitude=[
            '/neurodesktop-storage/data/bids-osf/bids/sub-1/ses-1/anat/sub-1_ses-1_run-01_echo-01_part-mag_MEGRE.nii.gz',
            '/neurodesktop-storage/data/bids-osf/bids/sub-1/ses-1/anat/sub-1_ses-1_run-01_echo-02_part-mag_MEGRE.nii.gz',
            '/neurodesktop-storage/data/bids-osf/bids/sub-1/ses-1/anat/sub-1_ses-1_run-01_echo-03_part-mag_MEGRE.nii.gz',
            '/neurodesktop-storage/data/bids-osf/bids/sub-1/ses-1/anat/sub-1_ses-1_run-01_echo-04_part-mag_MEGRE.nii.gz'
        ],
        TE=[4,12,20,28]
    )
  
    result = combine.run()

