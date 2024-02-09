import os

import numpy as np
import nibabel as nib

from nipype.interfaces.base import traits, SimpleInterface, CommandLine, BaseInterfaceInputSpec, TraitedSpec, File
from nipype.utils.filemanip import fname_presuffix, split_filename
from qsmxt.scripts.qsmxt_functions import extend_fname


## NeXtQSM wrapper
class NextqsmInputSpec(BaseInterfaceInputSpec):
    phase = File(mandatory=True, exists=True, argstr="%s", position=0)
    mask = File(mandatory=False, exists=True, argstr="%s", position=1)
    qsm = File(argstr="%s", name_source=['phase'], name_template='%s_nextqsm.nii.gz', position=2)
    #out_suffix = traits.String("_qsm_recon", desc='Suffix for output files. Will be followed by 000 (reason - see CLI)',
    #                           usedefault=True, argstr="-o %s")

class NextqsmOutputSpec(TraitedSpec):
    qsm = File()

class NextqsmInterface(CommandLine):
    input_spec = NextqsmInputSpec
    output_spec = NextqsmOutputSpec
    _cmd = "nextqsm"


## Normalize input data for NeXtQSM
def save_nii(data, file_path, nii_like):
    nib.save(nib.nifti1.Nifti1Image(data, affine=nii_like.affine, header=nii_like.header), file_path)

# fieldStrength in [T], TE in [s]
def normalize(phase, fieldStrength, TE, filename=None):
    centre_freq = 127736254 / 3 * fieldStrength # in [Hz]
    phase_nii = nib.load(phase)
    phase = phase_nii.get_fdata()
    normalized = phase / (2 * np.pi * TE * centre_freq) * 1e6
    
    if filename is not None:
        save_nii(normalized, filename, phase_nii)
        return filename
    
    return normalized
    

class NormalizeInputSpec(BaseInterfaceInputSpec):
    phase = File(mandatory=True, exists=True)
    TE = traits.Float(desc='Echo Time [sec]', mandatory=True, argstr="-t %f")
    fieldStrength = traits.Float(desc='Field Strength [Tesla]', mandatory=True, argstr="-f %f")
    out_suffix = traits.String("_normalized_phase", desc='Suffix for output files. Will be followed by 000 (reason - see CLI)',
                               usedefault=True, argstr="-o %s")


class NormalizeOutputSpec(TraitedSpec):
    out_file = File(desc='Phase normalized for NeXtQSM')


class NormalizeInterface(SimpleInterface):
    input_spec = NormalizeInputSpec
    output_spec = NormalizeOutputSpec
    
    def _run_interface(self, runtime):
        _, fname, _ = split_filename(self.inputs.phase)
        filename = fname_presuffix(fname=fname + self.inputs.out_suffix, suffix=".nii.gz", newpath=os.getcwd())
        self._results['out_file'] = normalize(self.inputs.phase, self.inputs.fieldStrength, self.inputs.TE, filename)
        return runtime


# fieldstrength in [T]
def normalizeB0(B0_file, fieldStrength, filename=None):
    centre_freq = 127736254 / 3 * fieldStrength # in [Hz]
    B0_nii = nib.load(B0_file)
    B0 = B0_nii.get_fdata() # in [Hz]
    normalized = B0 / centre_freq * 1e3
    
    if not filename:
        filename = extend_fname(B0_file, "_normalize", out_dir=os.getcwd())

    save_nii(normalized, filename, B0_nii)

    return filename

class NormalizeB0InputSpec(BaseInterfaceInputSpec):
    B0_file = File(mandatory=True, exists=True)
    fieldStrength = traits.Float(desc='Field Strength [Tesla]', mandatory=True, argstr="-f %f")
    out_suffix = traits.String("_normalized_B0", desc='Suffix for output files. Will be followed by 000 (reason - see CLI)',
                               usedefault=True, argstr="-o %s")


class NormalizeB0OutputSpec(TraitedSpec):
    out_file = File(desc='B0 normalized for NeXtQSM')


class NormalizeB0Interface(SimpleInterface):
    input_spec = NormalizeB0InputSpec
    output_spec = NormalizeB0OutputSpec
    
    def _run_interface(self, runtime):
        _, fname, _ = split_filename(self.inputs.B0_file)
        filename = fname_presuffix(fname=fname + self.inputs.out_suffix, suffix=".nii.gz", newpath=os.getcwd())
        self._results['out_file'] = normalizeB0(self.inputs.B0_file, self.inputs.fieldStrength, filename)
        return runtime
