from nipype.interfaces.base import CommandLine, TraitedSpec, traits, File, CommandLineInputSpec


class Bet2InputSpec(CommandLineInputSpec):
    in_file = File(
        exists=True,
        mandatory=True,
        argstr="%s",
        position=0
    )
    out_file = File(
        argstr="%s",
        name_source=['in_file'],
        name_template='%s_bet.nii.gz',
        position=1,
        exists=False
    )
    mask = File(
        argstr="-m %s",
        name_source=['in_file'],
        name_template='%s_bet-mask.nii.gz',
        position=2,
        exists=False
    )
    fractional_intensity = traits.Float(
        mandatory=False,
        argstr="-f %f",
        default=0.5
    )


class Bet2OutputSpec(TraitedSpec):
    out_file = File(exists=True)
    mask = File(exists=True)


class Bet2Interface(CommandLine):
    input_spec = Bet2InputSpec
    output_spec = Bet2OutputSpec
    _cmd = "bet"

