import os
from nipype.interfaces.base import CommandLine, TraitedSpec, traits, File, Directory, CommandLineInputSpec
from nipype.utils.filemanip import split_filename

class Nii2DcmInputSpec(CommandLineInputSpec):
    dicom_type = traits.Str(
        "MR",
        usedefault=True,
        argstr="--dicom_type %s",
    )
    ref_dicom = File(
        exists=True,
        mandatory=False,
        argstr="--ref_dicom %s"
    )
    centered = traits.Bool(
        argstr="--centered"
    )
    in_file = File(
        exists=True,
        mandatory=True,
        argstr="%s",
        position=0
    )
    out_dir = traits.Str(
        argstr="%s",
        name_source=['in_file'],
        name_template='%s_dcm',
        position=1
    )


class Nii2DcmOutputSpec(TraitedSpec):
    out_dir = traits.Directory(exists=True)


class Nii2DcmInterface(CommandLine):
    input_spec = Nii2DcmInputSpec
    output_spec = Nii2DcmOutputSpec
    _cmd = "git init && nii2dcm --dicom_type MR"

    def _format_arg(self, name, spec, value):
        if name == 'out_dir':
            path, base, _ = split_filename(self.inputs.in_file)
            path = os.path.join(os.path.abspath(path), f"{base}_dcm")
            os.makedirs(path, exist_ok=True)
            return path
        return super(Nii2DcmInterface, self)._format_arg(name, spec, value)

    def _list_outputs(self):
        outputs = self.output_spec().get()
        out_dir = self._format_arg('out_dir', self.inputs.trait('out_dir'), self.inputs.out_dir)
        outputs['out_dir'] = out_dir
        return outputs

