from qsmxt.interfaces.utils import CommandLineInputSpecJulia, CommandLineJulia
from nipype.interfaces.base import  TraitedSpec, File
from qsmxt.scripts import qsmxt_functions
import os

## Laplacian wrapper
class LaplacianInputSpec(CommandLineInputSpecJulia):
    def __init__(self, **inputs): super(LaplacianInputSpec, self).__init__(**inputs)
    phase = File(position=0, mandatory=True, exists=True, argstr='%s')
    phase_unwrapped = File(position=1, name_source=['phase'], name_template='%s_laplacian-unwrapped.nii.gz', argstr="%s")

class LaplacianOutputSpec(TraitedSpec):
    phase_unwrapped = File()

class LaplacianInterface(CommandLineJulia):
    def __init__(self, **inputs): super(LaplacianInterface, self).__init__(**inputs)
    input_spec = LaplacianInputSpec
    output_spec = LaplacianOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "mrt_laplacian_unwrapping.jl")
    
