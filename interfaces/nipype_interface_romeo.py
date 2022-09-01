import os, re
from nipype.interfaces.base import  traits, CommandLine, BaseInterfaceInputSpec, TraitedSpec, File, InputMultiPath, OutputMultiPath
from scripts import qsmxt_functions
import nibabel as nib
import numpy as np

def B0_unit_convert(B0_map, te):
    from math import pi
    nii = nib.load(B0_map)
    sim_phase = nii.get_fdata()
    sim_phase = sim_phase * (2*pi * te)/(10**3) # shortest te in s
    sim_phase = (sim_phase + np.pi) % (2 * np.pi) - np.pi
    out_file = os.path.abspath(os.path.split(B0_map)[1].replace(".nii", "_scaled.nii"))
    nib.save(nib.Nifti1Image(dataobj=sim_phase, header=nii.header, affine=nii.affine), out_file)
    return out_file

## Romeo wrapper single-echo (MapNode)
class RomeoInputSpec(BaseInterfaceInputSpec):
    phase = File(mandatory=True, exists=True, argstr="--phase %s")
    #mask = File(mandatory=False, exists=True, argstr="--mask %s")
    mag = File(mandatory=False, exists=True, argstr="--mag %s")
    TE = traits.Float(desc='Echo Time [sec]', mandatory=True, argstr="-t %s")
    out_file = File(name_source=['phase'], name_template='%s_romeo-unwrapped.nii.gz', argstr="--output %s")

class RomeoOutputSpec(TraitedSpec):
    out_file = File()

class RomeoInterface(CommandLine):
    input_spec = RomeoInputSpec
    output_spec = RomeoOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "romeo_unwrapping.jl --no-rescale")

    def _list_outputs(self):
        outputs = self.output_spec().get()
        outputs['out_file'] = os.path.join(os.getcwd(), os.path.split(self.inputs.phase)[1].split(".")[0] + "_romeo-unwrapped.nii.gz")
        outputs['out_file'] = B0_unit_convert(outputs['out_file'], self.inputs.TE)
        return outputs

## Romeo wrapper multi-echo (Node)
class RomeoB0InputSpec(BaseInterfaceInputSpec):
    phase = InputMultiPath(mandatory=True, exists=True)
    mag = InputMultiPath(mandatory=True, exists=True)
    combine_phase = File(exists=True, argstr="--phase %s", position=0)
    combine_mag = File(exists=True, argstr="--mag %s", position=1)
    TE = traits.ListFloat(desc='Echo Time [sec]', mandatory=True, argstr="-t [%s]")

class RomeoB0OutputSpec(TraitedSpec):
    B0 = File('B0.nii', exists=True)
    # B0s = OutputMultiPath(File(exists=False))

class RomeoB0Interface(CommandLine):
    input_spec = RomeoB0InputSpec
    output_spec = RomeoB0OutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "romeo_unwrapping.jl -B --no-rescale --phase-offset-correction")

    def _run_interface(self, runtime):
        self.inputs.combine_phase = save_multi_echo(self.inputs.phase, os.path.join(os.getcwd(), "multi-echo-phase.nii"))
        self.inputs.combine_mag = save_multi_echo(self.inputs.mag, os.path.join(os.getcwd(), "multi-echo-mag.nii"))
        
        return super(RomeoB0Interface, self)._run_interface(runtime)
        
        
    def _list_outputs(self):
        outputs = self.output_spec().get()
        outputs['B0'] = os.path.join(os.getcwd(), "B0.nii")
        outfile_final = os.path.join(os.getcwd(), os.path.split(self.inputs.phase[0])[1].split(".")[0] + "_romeoB0-unwrapped.nii")

        os.rename(outputs['B0'], outfile_final)
        outputs['B0'] = outfile_final

        outputs['B0'] = B0_unit_convert(outputs['B0'], np.min(self.inputs.TE))
        return outputs
    
def save_multi_echo(in_files, fn_path):
    image4d = np.stack([nib.load(f).get_fdata() for f in in_files], -1)
    sample_nii = nib.load(in_files[0])
    nib.save(nib.nifti1.Nifti1Image(image4d, affine=sample_nii.affine, header=sample_nii.header), fn_path)
    return fn_path


if __name__ == "__main__":
    combine = RomeoB0Interface(phase=['/neurodesktop-storage/QSMxT/qsmxt-test-battery-bids/bids/sub-qsmchallenge2019-sim1/ses-1/anat/sub-1_ses-1_run-1_echo-1_part-phase_MEGRE.nii.gz', '/neurodesktop-storage/QSMxT/qsmxt-test-battery-bids/bids/sub-qsmchallenge2019-sim1/ses-1/anat/sub-1_ses-1_run-1_echo-2_part-phase_MEGRE.nii.gz', '/neurodesktop-storage/QSMxT/qsmxt-test-battery-bids/bids/sub-qsmchallenge2019-sim1/ses-1/anat/sub-1_ses-1_run-1_echo-3_part-phase_MEGRE.nii.gz', '/neurodesktop-storage/QSMxT/qsmxt-test-battery-bids/bids/sub-qsmchallenge2019-sim1/ses-1/anat/sub-1_ses-1_run-1_echo-4_part-phase_MEGRE.nii.gz'],
                                mag=['/neurodesktop-storage/QSMxT/qsmxt-test-battery-bids/bids/sub-qsmchallenge2019-sim1/ses-1/anat/sub-1_ses-1_run-1_echo-1_part-mag_MEGRE.nii.gz', '/neurodesktop-storage/QSMxT/qsmxt-test-battery-bids/bids/sub-qsmchallenge2019-sim1/ses-1/anat/sub-1_ses-1_run-1_echo-2_part-mag_MEGRE.nii.gz', '/neurodesktop-storage/QSMxT/qsmxt-test-battery-bids/bids/sub-qsmchallenge2019-sim1/ses-1/anat/sub-1_ses-1_run-1_echo-3_part-mag_MEGRE.nii.gz', '/neurodesktop-storage/QSMxT/qsmxt-test-battery-bids/bids/sub-qsmchallenge2019-sim1/ses-1/anat/sub-1_ses-1_run-1_echo-4_part-mag_MEGRE.nii.gz'],
                                TE=[4,12,20,28])
    # combine = RomeoInterface(phase='/neurodesktop-storage/QSMxT/qsmxt-test-battery-bids/bids/sub-basel-3depi-P001/ses-1/anat/sub-basel-3depi-P001_ses-1_run-1_part-phase_T2starw.nii',
    #                             mag='/neurodesktop-storage/QSMxT/qsmxt-test-battery-bids/bids/sub-basel-3depi-P001/ses-1/anat/sub-basel-3depi-P001_ses-1_run-1_part-mag_T2starw.nii',
    #                             )
  
    result = combine.run()
    # print(result.runtime)