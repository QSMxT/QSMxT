import os
from nipype.interfaces.base import  traits, CommandLine, BaseInterfaceInputSpec, TraitedSpec, File, InputMultiPath, OutputMultiPath
import nibabel as nib
import numpy as np


## Romeo wrapper single-echo (MapNode)
class RomeoInputSpec(BaseInterfaceInputSpec):
    phase = File(mandatory=True, exists=True, argstr="--phase %s")
    #mask = File(mandatory=False, exists=True, argstr="--mask %s")
    mag = File(mandatory=False, exists=True, argstr="--mag %s")
    out_file = File(name_source=['phase'], name_template='%s_unwrapped.nii.gz', argstr="--output %s")

class RomeoOutputSpec(TraitedSpec):
    out_file = File()

class RomeoInterface(CommandLine):
    input_spec = RomeoInputSpec
    output_spec = RomeoOutputSpec
    _cmd = "romeoApp.jl --no-rescale"


## Romeo wrapper multi-echo (Node)
class RomeoB0InputSpec(BaseInterfaceInputSpec):
    phase = InputMultiPath(mandatory=True, exists=True)
    mag = InputMultiPath(mandatory=True, exists=True)
    TE = traits.ListFloat(desc='Echo Time [sec]', mandatory=True, argstr="-t %s")

class RomeoB0OutputSpec(TraitedSpec):
    B0 = File('B0.nii', usedefault=True)
    unwrapped_phase = OutputMultiPath()

class RomeoB0Interface(CommandLine):
    input_spec = RomeoB0InputSpec
    output_spec = RomeoB0OutputSpec
    _cmd = "romeoApp.jl -B --no-rescale --phase-offset-correction --phase multi-echo-phase.nii --mag multi-echo-mag.nii"

    def _run_interface(self, runtime):
        save_multi_echo(self.inputs.phase, "multi-echo-phase.nii")
        save_multi_echo(self.inputs.mag, "multi-echo-mag.nii")
        super(RomeoB0Interface, self)._run_interface(runtime)
        
    def _list_outputs(self):
        outputs = self.output_spec().get()
        fn_unwrapped_phase = outputs['B0'].replace('B0', 'unwrapped')
        outputs['unwrapped_phase'] = save_individual_echo(fn_unwrapped_phase, os.getcwd())
        return outputs
    
def save_multi_echo(in_files, fn_path):
    image4d = np.stack([nib.load(f).get_fdata() for f in in_files], -1)
    sample_nii = nib.load(in_files[0])
    nib.save(nib.nifti1.Nifti1Image(image4d, affine=sample_nii.affine, header=sample_nii.header), fn_path)
    return fn_path
    
def save_individual_echo(in_file, pth):
    image4d_nii = nib.load(in_file)
    image4d = image4d_nii.get_fdata()
    if image4d.ndim == 3:
        image4d = image4d.reshape((*image4d.shape, 1))
        
    output_names = []
    n_eco = image4d.shape[3]
    for i in range(0, n_eco):
        file_without_ext = in_file.replace(".nii.gz", ".nii").replace(".nii", "")
        fn = pth + '/' + file_without_ext + "_echo{}.nii.gz".format(i)
        
        nib.save(nib.nifti1.Nifti1Image(image4d[:,:,:,i], affine=image4d_nii.affine, header=image4d_nii.header), fn)
        output_names.append(fn)
    return output_names
