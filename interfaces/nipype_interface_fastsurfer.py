from nipype.interfaces.base import CommandLine, traits, TraitedSpec, File, CommandLineInputSpec, Directory
import os


class FastSurferInputSpec(CommandLineInputSpec):
    in_file = File(
        exists=True,
        mandatory=True,
        argstr="--t1 %s",
        position=0
    )


class FastSurferOutputSpec(TraitedSpec):
    out_file = File()


class FastSurferInterface(CommandLine):
    input_spec = FastSurferInputSpec
    output_spec = FastSurferOutputSpec
    _cmd = "run_fastsurfer.sh --sd `pwd` --threads 16 --parallel --seg_only --sid output"

    def __init__(self, **inputs):
        super(FastSurferInterface, self).__init__(**inputs)

    def _list_outputs(self):
        outputs = self.output_spec().get()
        outputs['out_file'] = os.path.join('output', 'mri', 'aparc.DKTatlas+aseg.deep.mgz')
        return outputs

