from nipype.interfaces.base import  CommandLine, TraitedSpec, File, CommandLineInputSpec
from nipype.utils.filemanip import fname_presuffix, split_filename


## Laplacian wrapper
class LaplacianInputSpec(CommandLineInputSpec):
    phase = File(position=0, mandatory=True, exists=True, argstr='%s')
    out_file = File(position=1, name_source=['phase'], name_template='%s_laplacian_unwrapped.nii.gz', argstr="%s")

class LaplacianOutputSpec(TraitedSpec):
    out_file = File()

class LaplacianInterface(CommandLine):
    input_spec = LaplacianInputSpec
    output_spec = LaplacianOutputSpec
    _cmd = "laplacian_unwrapping.jl"
    