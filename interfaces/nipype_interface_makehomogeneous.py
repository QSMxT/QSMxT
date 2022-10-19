from nipype.interfaces.base import CommandLine, TraitedSpec, File, CommandLineInputSpec
from scripts import qsmxt_functions
import os

class MakeHomogeneousInputSpec(CommandLineInputSpec):
    in_file = File(
        exists=True,
        mandatory=True,
        argstr="%s",
        position=0
    )
    out_file = File(
        argstr="%s",
        name_source=['in_file'],
        name_template='%s_makehomogeneous.nii',
        position=1
    )


class MakeHomogeneousOutputSpec(TraitedSpec):
    out_file = File()


class MakeHomogeneousInterface(CommandLine):
    input_spec = MakeHomogeneousInputSpec
    output_spec = MakeHomogeneousOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "makehomogeneous.jl")
