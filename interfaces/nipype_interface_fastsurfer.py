from nipype.interfaces.base import CommandLine, traits, TraitedSpec, File, CommandLineInputSpec, Directory
import os
import shutil


class FastSurferInputSpec(CommandLineInputSpec):
    in_file = File(
        exists=True,
        mandatory=True,
        argstr="--t1 %s",
        position=0
    )
    num_threads = traits.Int(
        mandatory=False,
        argstr="--parallel --threads %d"
    )


class FastSurferOutputSpec(TraitedSpec):
    out_file = File()


class FastSurferInterface(CommandLine):
    input_spec = FastSurferInputSpec
    output_spec = FastSurferOutputSpec
    _cmd = "run_fastsurfer.sh --sd `pwd` --seg_only --sid output --py python3.6"

    def __init__(self, **inputs):
        super(FastSurferInterface, self).__init__(**inputs)

    def _list_outputs(self):
        outputs = self.output_spec().get()
        infile_name = (os.path.split(self.inputs.in_file)[1]).split('.')[0]
        outfile_old = os.path.join('output', 'mri', 'aparc.DKTatlas+aseg.deep.mgz')
        outfile_new = os.path.join('output', 'mri', infile_name + '_segmentation.mgz')
        shutil.copy(outfile_old, outfile_new)
        outputs['out_file'] = outfile_new
        return outputs

