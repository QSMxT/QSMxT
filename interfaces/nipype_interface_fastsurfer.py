from nipype.interfaces.base import CommandLine, traits, TraitedSpec, File, CommandLineInputSpec
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
    _cmd = "run_fastsurfer.sh --sd `pwd` --seg_only --sid output --py python3.8"

    def __init__(self, **inputs):
        super(FastSurferInterface, self).__init__(**inputs)

        self.inputs.on_trait_change(self._num_threads_update, 'num_threads')

        if not isdefined(self.inputs.num_threads):
            self.inputs.num_threads = self._num_threads
        else:
            self._num_threads_update()

    def _list_outputs(self):
        outputs = self.output_spec().get()
        infile_name = (os.path.split(self.inputs.in_file)[1]).split('.')[0]
        outfile_old = os.path.join('output', 'mri', 'aparc.DKTatlas+aseg.deep.mgz')
        outfile_new = os.path.join('output', 'mri', infile_name + '_segmentation.mgz')
        shutil.copy(outfile_old, outfile_new)
        outputs['out_file'] = outfile_new
        return outputs

    def _num_threads_update(self):
        self._num_threads = self.inputs.num_threads
        if self.inputs.num_threads == -1:
            cpu_count = min(8, os.environ["NCPUS"] if "NCPUS" in os.environ else str(os.cpu_count()))
            self.inputs.environ.update({ "OMP_NUM_THREADS" : cpu_count })
        else:
            self.inputs.environ.update({ "OMP_NUM_THREADS" : f"{self.inputs.num_threads}" })

   