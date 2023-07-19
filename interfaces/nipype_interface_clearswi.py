from nipype.interfaces.base import CommandLine, TraitedSpec, File, CommandLineInputSpec, traits, InputMultiPath
from scripts import qsmxt_functions
import os


class ClearSwiInputSpec(CommandLineInputSpec):
    phase = InputMultiPath(
        exists=True,
        mandatory=True,
        argstr="--phase %s"
    )
    magnitude = InputMultiPath(
        exists=True,
        mandatory=True,
        argstr="--magnitude %s"
    )
    TE = traits.ListFloat(
        mandatory=True,
        argstr="--TEs '[%s]'"
    )
    swi = File(
        argstr="--swi-out %s",
        name_source=['phase'],
        name_template='%s_swi.nii'
    )
    swi_mip = File(
        argstr="--mip-out %s",
        name_source=['phase'],
        name_template='%s_swi-mip.nii'
    )

class ClearSwiOutputSpec(TraitedSpec):
    swi = File()
    swi_mip = File()

class ClearSwiInterface(CommandLine):
    input_spec = ClearSwiInputSpec
    output_spec = ClearSwiOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "mrt_clearswi.jl")

