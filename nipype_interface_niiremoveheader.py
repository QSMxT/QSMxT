from nipype.interfaces.base import CommandLine, traits, TraitedSpec, File, CommandLineInputSpec


class NiiRemoveHeaderInputSpec(CommandLineInputSpec):
    in_file = File(
        exists=True,
        mandatory=True,
        argstr="%s",
        position=0
    )
    out_file = File(
        argstr="%s",
        name_source=['in_file'],
        name_template='%s_rem.nii',
        position=1
    )


class NiiRemoveHeaderOutputSpec(TraitedSpec):
    out_file = File()


class NiiRemoveHeaderInterface(CommandLine):
    input_spec = NiiRemoveHeaderInputSpec
    output_spec = NiiRemoveHeaderOutputSpec
    _cmd = "nii-remove-header.py"

