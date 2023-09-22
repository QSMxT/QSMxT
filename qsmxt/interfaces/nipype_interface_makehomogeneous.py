from qsmxt.interfaces.utils import CommandLineInputSpecJulia, CommandLineJulia
from nipype.interfaces.base import TraitedSpec, File
from qsmxt.scripts import qsmxt_functions
import os

class MakeHomogeneousInputSpec(CommandLineInputSpecJulia):
    def __init__(self, **inputs): super(MakeHomogeneousInputSpec, self).__init__(**inputs)
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


class MakeHomogeneousInterface(CommandLineJulia):
    def __init__(self, **inputs): super(MakeHomogeneousInterface, self).__init__(**inputs)
    input_spec = MakeHomogeneousInputSpec
    output_spec = MakeHomogeneousOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "makehomogeneous.jl")
