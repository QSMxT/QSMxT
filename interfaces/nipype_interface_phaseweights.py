from nipype.interfaces.base import CommandLine, TraitedSpec, File, CommandLineInputSpec, traits
from nipype.utils.filemanip import fname_presuffix, split_filename
import os


class PhaseWeightsInputSpec(CommandLineInputSpec):
    in_file = File(
        exists=True,
        mandatory=True,
        argstr="%s",
        position=0
    )
    out_file = File(
        argstr="%s",
        name_source=['in_file'],
        name_template='%s_weights.nii',
        position=1
    )


class PhaseWeightsOutputSpec(TraitedSpec):
    out_file = File()


class PhaseWeightsInterface(CommandLine):
    input_spec = PhaseWeightsInputSpec
    output_spec = PhaseWeightsOutputSpec
    _cmd = "phase_weights.jl"

    def __init__(self, **inputs):
        super(PhaseWeightsInterface, self).__init__(**inputs)

    def _list_outputs(self):
        outputs = self.output_spec().get()

        _, fname, _ = split_filename(self.inputs.in_file)

        outputs['out_file'] = fname_presuffix(
            fname=fname + "_weights",
            suffix=".nii",
            newpath=os.getcwd()
        )

        return outputs


class RomeoPhaseWeightsInputSpec(CommandLineInputSpec):
    phase = File(
        exists=True,
        mandatory=True,
        argstr="--phase %s"
    )
    mag = File(
        exists=True,
        argstr="--mag %s"
    )
    weight_type = traits.Str(
        default_value="error",
        argstr="--type %s"
    )
    out_file = File(
        argstr="--output %s",
        name_source=['phase'],
        name_template='%s_romeo_voxelquality.nii'
    )

class RomeoPhaseWeightsOutputSpec(TraitedSpec):
    out_file = File()

class RomeoPhaseWeightsInterface(CommandLine):
    input_spec = RomeoPhaseWeightsInputSpec
    output_spec = RomeoPhaseWeightsOutputSpec
    _cmd = "romeo_voxelquality.jl"
    

class HagbergPhaseWeightsInputSpec(CommandLineInputSpec):
    phase = File(
        exists=True,
        mandatory=True,
        argstr="--phase %s"
    )
    out_file = File(
        argstr="--output %s",
        name_source=['phase'],
        name_template='%s_pb_mask.nii'
    )

class HagbergPhaseWeightsOutputSpec(TraitedSpec):
    out_file = File()

class HagbergPhaseWeightsInterface(CommandLine):
    input_spec = HagbergPhaseWeightsInputSpec
    output_spec = HagbergPhaseWeightsOutputSpec
    _cmd = "hagberg_pb_masking.jl"
