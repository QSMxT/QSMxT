from nipype.interfaces.base import CommandLine, TraitedSpec, File, CommandLineInputSpec
from qsmxt.scripts import qsmxt_functions
import os

class MakeHomogeneousInputSpec(CommandLineInputSpec):
    magnitude = File(
        exists=True,
        mandatory=True,
        argstr="%s",
        position=0
    )
    magnitude_corrected = File(
        argstr="%s",
        name_source=['magnitude'],
        name_template='%s_makehomogeneous.nii',
        position=1
    )


class MakeHomogeneousOutputSpec(TraitedSpec):
    magnitude_corrected = File()


class MakeHomogeneousInterface(CommandLine):
    input_spec = MakeHomogeneousInputSpec
    output_spec = MakeHomogeneousOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "makehomogeneous.jl")
