from nipype.interfaces.base import TraitedSpec, File, traits, InputMultiPath
from qsmxt.interfaces.utils import CommandLineInputSpecJulia, CommandLineJulia
from qsmxt.scripts import qsmxt_functions
import os

class PbMaskingInputSpec(CommandLineInputSpecJulia):
    def __init__(self, **inputs): super(PbMaskingInputSpec, self).__init__(**inputs)
    phase = File(
        exists=True,
        mandatory=True,
        argstr="--phase %s"
    )
    mask = File(
        argstr="--output %s",
        name_source=['phase'],
        name_template='%s_pb_mask.nii'
    )

class PbMaskingOutputSpec(TraitedSpec):
    mask = File()

class PbMaskingInterface(CommandLineJulia):
    def __init__(self, **inputs): super(PbMaskingInterface, self).__init__(**inputs)
    input_spec = PbMaskingInputSpec
    output_spec = PbMaskingOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "hagberg_pb_masking.jl")


class RomeoMaskingInputSpec(CommandLineInputSpecJulia):
    def __init__(self, **inputs): super(RomeoMaskingInputSpec, self).__init__(**inputs)
    phase = InputMultiPath(
        exists=True,
        mandatory=True,
        argstr="--phase %s"
    )
    magnitude = InputMultiPath(
        mandatory=False,
        exists=True,
        argstr="--mag %s"
    )
    TEs = traits.ListFloat(
        mandatory=False,
        argstr="--TEs '[%s]'"
    )
    TE = traits.Float(
        mandatory=False,
        argstr="--TEs '[%s]'"
    )
    weight_type = traits.Str(
        default_value="grad+second",
        argstr="--type %s"
    )
    quality_map = File(
        argstr="--output %s",
        name_source=['phase'],
        name_template='%s_romeo_voxelquality.nii'
    )

class RomeoMaskingOutputSpec(TraitedSpec):
    quality_map = File()

class RomeoMaskingInterface(CommandLineJulia):
    def __init__(self, **inputs): super(RomeoMaskingInterface, self).__init__(**inputs)
    input_spec = RomeoMaskingInputSpec
    output_spec = RomeoMaskingOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "romeo_voxelquality.jl")

    def _format_arg(self, name, trait_spec, value):
        if name == 'TEs' or name == 'TE':
            if self.inputs.TEs is None and self.inputs.TE is None:
                raise ValueError("Either TEs or TE must be provided")
        return super(RomeoMaskingInterface, self)._format_arg(name, trait_spec, value)
    
