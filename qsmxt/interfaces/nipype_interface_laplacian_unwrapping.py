from nipype.interfaces.base import  CommandLine, TraitedSpec, File, CommandLineInputSpec
from qsmxt.scripts import qsmxt_functions
import os

## Laplacian wrapper
class LaplacianInputSpec(CommandLineInputSpec):
    phase = File(position=0, mandatory=True, exists=True, argstr='%s')
    phase_unwrapped = File(position=1, name_source=['phase'], name_template='%s_laplacian-unwrapped.nii.gz', argstr="%s")

class LaplacianOutputSpec(TraitedSpec):
    phase_unwrapped = File()

class LaplacianInterface(CommandLine):
    input_spec = LaplacianInputSpec
    output_spec = LaplacianOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "mrt_laplacian_unwrapping.jl")
    
