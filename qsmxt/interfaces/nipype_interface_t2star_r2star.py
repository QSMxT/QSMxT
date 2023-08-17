from nipype.interfaces.base import CommandLine, TraitedSpec, File, CommandLineInputSpec, traits, InputMultiPath
from qsmxt.scripts import qsmxt_functions
import os


class T2sR2sInputSpec(CommandLineInputSpec):
    magnitude = InputMultiPath(
        exists=True,
        mandatory=True,
        argstr="--magnitude %s"
    )
    TE = traits.ListFloat(
        mandatory=True,
        argstr="--TEs '[%s]'"
    )
    t2starmap = File(
        argstr="--t2starmap %s",
        name_source=['magnitude'],
        name_template='%s_t2s.nii'
    )
    r2starmap = File(
        argstr="--r2starmap %s",
        name_source=['magnitude'],
        name_template='%s_r2s.nii'
    )

class T2sR2sOutputSpec(TraitedSpec):
    t2starmap = File()
    r2starmap = File()

class T2sR2sInterface(CommandLine):
    input_spec = T2sR2sInputSpec
    output_spec = T2sR2sOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "mrt_t2star_r2star.jl")

