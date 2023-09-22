from nipype.interfaces.base import TraitedSpec, File, traits, InputMultiPath
from qsmxt.interfaces.utils import CommandLineInputSpecJulia, CommandLineJulia
from qsmxt.scripts import qsmxt_functions
import os


class T2sR2sInputSpec(CommandLineInputSpecJulia):
    def __init__(self, **inputs): super(T2sR2sInputSpec, self).__init__(**inputs)
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

class T2sR2sInterface(CommandLineJulia):
    def __init__(self, **inputs): super(T2sR2sInterface, self).__init__(**inputs)
    input_spec = T2sR2sInputSpec
    output_spec = T2sR2sOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "mrt_t2star_r2star.jl")

