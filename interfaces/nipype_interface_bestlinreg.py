from nipype.interfaces.base import CommandLine, TraitedSpec, File, CommandLineInputSpec


class NiiBestlinregInputSpec(CommandLineInputSpec):
    in_fixed = File(
        exists=True,
        mandatory=True,
        argstr="%s",
        position=0
    )
    in_moving = File(
        exists=True,
        mandatory=True,
        argstr="%s",
        position=1
    )
    out_transform = File(
        argstr="%s",
        name_source=['in_fixed'],
        name_template='%s_transform.xfm',
        position=2
    )


class NiiBestlinregOutputSpec(TraitedSpec):
    out_transform = File(exists=True)


class NiiBestLinRegInterface(CommandLine):
    input_spec = NiiBestlinregInputSpec
    output_spec = NiiBestlinregOutputSpec
    _cmd = "nii-bestlinreg.py"

