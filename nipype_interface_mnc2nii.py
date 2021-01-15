from nipype.interfaces.base import CommandLine, traits, TraitedSpec, File, CommandLineInputSpec
from nipype.utils.filemanip import fname_presuffix, split_filename
import os

class Mnc2NiiInputSpec(CommandLineInputSpec):
    in_file = File(
        exists=True,
        mandatory=True,
        argstr="%s",
        position=0
    )
    out_file = File(
        argstr="%s",
        name_source=['in_file'],
        name_template='%s_mnc2nii.mnc',
        position=1
    )


class Mnc2NiiOutputSpec(TraitedSpec):
    out_file = File()


class Mnc2NiiInterface(CommandLine):
    input_spec = Mnc2NiiInputSpec
    output_spec = Mnc2NiiOutputSpec
    _cmd = "mnc2nii"

    def _list_outputs(self):
        outputs = self.output_spec().get()

        _, fname, _ = split_filename(self.inputs.in_file)

        outputs['out_file'] = fname_presuffix(
            fname=fname + "_mnc2nii",
            suffix=".nii",
            newpath=os.getcwd()
        )

        return outputs

