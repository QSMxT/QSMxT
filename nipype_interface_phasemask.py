import os
from nipype.interfaces.base import CommandLine, traits, TraitedSpec, File, CommandLineInputSpec, OutputMultiPath
from nipype.interfaces.base.traits_extension import isdefined
from nipype.utils.filemanip import fname_presuffix, split_filename


def gen_filename(fname, suffix, newpath, use_ext=True):
    return fname_presuffix(
        fname=fname,
        suffix=suffix,
        newpath=newpath,
        use_ext=use_ext
    )


class PhaseMaskInputSpec(CommandLineInputSpec):
    in_file = File(
        exists=True,
        desc='Phase image',
        mandatory=True,
        argstr="%s",
        position=0
    )
    echo_time = traits.Float(
        minlen=1,
        desc='Echo time',
        mandatory=True,
        argstr="%s",
        position=1
    )
    weights_threshold = traits.Int(
        default_value=300,
        desc='Weights threshold',
        mandatory=True,
        argstr="%s",
        position=2
    )
    out_file = File(
        argstr="%s",
        name_source=['in_file'],
        name_template='%s_phasemask.nii',
        position=3
    )


class PhaseMaskOutputSpec(TraitedSpec):
    out_file = File(
        desc='Output mask'
    )


class PhaseMaskInterface(CommandLine):
    input_spec = PhaseMaskInputSpec
    output_spec = PhaseMaskOutputSpec
    _cmd = "phase_mask.jl"

    def __init__(self, **inputs):
        super(PhaseMaskInterface, self).__init__(**inputs)

    def _list_outputs(self):
        outputs = self.output_spec().get()

        pth, fname, ext = split_filename(self.inputs.in_file)
        outfile = gen_filename(
            fname=fname + "_phasemask",
            suffix=".nii",
            newpath=os.getcwd()
        )
        outputs['out_file'] = outfile

        return outputs

    def _format_arg(self, name, spec, value):
        if name == 'echo_times':
            value = str(value)
            value = value.replace(' ', '')
            value = value.replace('[', '').replace(']', '')
            return spec.argstr % value
        return super(PhaseMaskInterface, self)._format_arg(name, spec, value)
