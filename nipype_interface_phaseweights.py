from nipype.interfaces.base import CommandLine, traits, TraitedSpec, File, CommandLineInputSpec
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
