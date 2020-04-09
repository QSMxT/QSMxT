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

class RomeoInputSpec(CommandLineInputSpec):
    mag_file = File(
        exists=True,
        desc='Magnitude image',
        mandatory=True,
        argstr="-m %s",
        position=0
    )
    output_folder = traits.String(
        value="romeo",
        desc='ROMEO output directory',
        usedefault=True,
        argstr="-o %s",
        position=1
    )
    echo_times = traits.List(
        minlen=2,
        desc='Echo times',
        mandatory=True,
        argstr="-t %s",
        position=2
    )
    phase_file = File(
        exists=True,
        desc='Phase image',
        mandatory=True,
        argstr="%s",
        position=3
    )



class RomeoOutputSpec(TraitedSpec):
    mask_file = File(desc='Output mask')
    mask_files = OutputMultiPath(
        File(desc='Output masks')
    )


class RomeoInterface(CommandLine):
    input_spec = RomeoInputSpec
    output_spec = RomeoOutputSpec
    _cmd = "/home/ashley/repos/ROMEO_compiled_20190920/bqunwrap"

    def __init__(self, **inputs):
        super(RomeoInterface, self).__init__(**inputs)

    def _list_outputs(self):
        outputs = self.output_spec().get()

        outputs['mask_file'] = gen_filename(
            fname=self.inputs.output_folder + '/mask',
            suffix='.nii',
            newpath=self.inputs.output_folder
        )

        # NOTE: workaround for multi-echo data - the QSM node requires one mask per phase image
        outputs['mask_files'] = [outputs['mask_file'] for x in range(len(self.inputs.echo_times))]

        return outputs

    def _format_arg(self, name, spec, value):
        if name == 'echo_times':
            value = str(value)
            value = value.replace(' ', '')
            return spec.argstr%value
        return super(RomeoInterface, self)._format_arg(name, spec, value)
