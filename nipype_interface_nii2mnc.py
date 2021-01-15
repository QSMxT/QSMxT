from nipype.interfaces.base import CommandLine, traits, TraitedSpec, File, CommandLineInputSpec
from nipype.utils.filemanip import fname_presuffix, split_filename
import os

class Nii2MncInputSpec(CommandLineInputSpec):
    dtype = traits.Str(
        'float',
        argstr='-%s',
        usedefault=True,
        position=0
    )
    in_file = File(
        exists=True,
        mandatory=True,
        argstr="%s",
        position=1
    )
    out_file = File(
        argstr="%s",
        name_source=['in_file'],
        name_template='%s_nii2mnc.mnc',
        position=2
    )


class Nii2MncOutputSpec(TraitedSpec):
    out_file = File()


class Nii2MncInterface(CommandLine):
    input_spec = Nii2MncInputSpec
    output_spec = Nii2MncOutputSpec
    _cmd = "nii2mnc"

    def _list_outputs(self):
        outputs = self.output_spec().get()

        _, fname, _ = split_filename(self.inputs.in_file)

        outputs['out_file'] = fname_presuffix(
            fname=fname + "_nii2mnc",
            suffix=".mnc",
            newpath=os.getcwd()
        )

        return outputs
