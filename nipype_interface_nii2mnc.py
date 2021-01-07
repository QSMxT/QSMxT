from nipype.interfaces.base import CommandLine, traits, TraitedSpec, File, CommandLineInputSpec


class Nii2MncInputSpec(CommandLineInputSpec):
    in_file = File(
        exists=True,
        mandatory=True,
        argstr="%s",
        position=0
    )
    out_file = File(
        argstr="%s",
        name_source=['in_file'],
        name_template='%s_nii2mnc.mnc',
        position=1
    )


class Nii2MncOutputSpec(TraitedSpec):
    out_file = File()


class Nii2MncInterface(CommandLine):
    input_spec = Nii2MncInputSpec
    output_spec = Nii2MncOutputSpec
    _cmd = "nii2mnc"
