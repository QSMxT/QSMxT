import os
import sys
import shutil

from nipype.interfaces.base import CommandLine, traits, TraitedSpec, File, CommandLineInputSpec
from nipype.interfaces.base.traits_extension import isdefined
from qsmxt.scripts.qsmxt_functions import extend_fname

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
        outfile_old = os.path.join('output', 'mri', 'aparc.DKTatlas+aseg.deep.mgz')
        outfile_new = extend_fname(self.inputs.in_file, "_dseg", ext="mgz", out_dir=os.getcwd())
        try:
            shutil.copy(outfile_old, outfile_new)
        except FileNotFoundError:
            print("Expected output from FastSurfer missing! It may have been killed due to insufficient memory.", file=sys.stderr)
        outputs['out_file'] = outfile_new
        return outputs

    def _num_threads_update(self):
        self._num_threads = self.inputs.num_threads
        if self.inputs.num_threads == -1:
            cpu_count = min(8, os.environ["NCPUS"] if "NCPUS" in os.environ else str(os.cpu_count()))
            self.inputs.environ.update({ "OMP_NUM_THREADS" : cpu_count })
        else:
            self.inputs.environ.update({ "OMP_NUM_THREADS" : f"{self.inputs.num_threads}" })

   
