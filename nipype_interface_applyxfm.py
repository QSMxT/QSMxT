from nipype.interfaces.base import CommandLine, traits, TraitedSpec, File, CommandLineInputSpec


class NiiApplyMincXfmInputSpec(CommandLineInputSpec):
    in_file = File(
        exists=True,
        mandatory=True,
        argstr="%s",
        position=0
    )
    in_like = File(
        exists=True,
        mandatory=True,
        argstr="%s",
        position=1
    )
    in_transform = File(
        exists=True,
        mandatory=True,
        argstr="%s",
        position=2
    )
    out_file = File(
        argstr="%s",
        name_source=['in_file'],
        name_template='%s_transformed.nii',
        position=3
    )
    nearest = traits.Str(
        "--nearest",
        position=4
    )


class NiiApplyMincXfmOutputSpec(TraitedSpec):
    out_file = File()


class NiiApplyMincXfmInterface(CommandLine):
    input_spec = NiiApplyMincXfmInputSpec
    output_spec = NiiApplyMincXfmOutputSpec
    _cmd = "nii-applyxfm.py"

    