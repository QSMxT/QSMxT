import os
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
    out_file = File(name_source=['phase'], name_template='%s_romeo.nii.gz', argstr="--output %s")

class RomeoOutputSpec(TraitedSpec):
    out_file = File()

class RomeoInterface(CommandLine):
    input_spec = RomeoInputSpec
    output_spec = RomeoOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "romeo_unwrapping.jl --no-rescale")

## Romeo wrapper multi-echo (Node)
class RomeoB0InputSpec(BaseInterfaceInputSpec):
    phase = InputMultiPath(mandatory=True, exists=True)
    mag = InputMultiPath(mandatory=True, exists=True)
    TE = traits.ListFloat(desc='Echo Time [sec]', mandatory=True, argstr="-t [%s]")

class RomeoB0OutputSpec(TraitedSpec):
    B0 = File('B0.nii', exists=True)
    # B0s = OutputMultiPath(File(exists=False))

class RomeoB0Interface(CommandLine):
    input_spec = RomeoB0InputSpec
    output_spec = RomeoB0OutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "romeo_unwrapping.jl -B --no-rescale --phase-offset-correction --phase multi-echo-phase.nii --mag multi-echo-mag.nii")

    def _run_interface(self, runtime):
        self.inputs.combine_phase = save_multi_echo(self.inputs.phase, os.path.join(os.getcwd(), "multi-echo-phase.nii"))
        self.inputs.combine_mag = save_multi_echo(self.inputs.mag, os.path.join(os.getcwd(), "multi-echo-mag.nii"))
        
        return super(RomeoB0Interface, self)._run_interface(runtime)
        
        
    def _list_outputs(self):
        outputs = self.output_spec().get()
        #fn_unwrapped_phase = os.path.abspath(os.path.join('.', 'unwrapped.nii'))
        #outputs['unwrapped_phase'] = save_individual_echo(fn_unwrapped_phase, os.getcwd())
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
    
def save_individual_echo(in_file, pth):
    image4d_nii = nib.load(in_file)
    image4d = image4d_nii.get_fdata()
    if image4d.ndim == 3:
        image4d = image4d.reshape((*image4d.shape, 1))
        
    output_names = []
    n_eco = image4d.shape[3]
    for i in range(0, n_eco):
        file_without_ext = in_file.replace(".nii.gz", ".nii").replace(".nii", "")
        fn =  os.path.join(pth, f"{file_without_ext}_echo{i}.nii.gz")
        nib.save(nib.nifti1.Nifti1Image(image4d[:,:,:,i], affine=image4d_nii.affine, header=image4d_nii.header), fn)
        output_names.append(fn)
    return output_names
