"""
Created on Sun Aug  3 11:46:42 2014

@author: epracht
modified by Steffen.Bollmann@cai.uq.edu.au
"""

from __future__ import division
from nipype.interfaces.base import CommandLine, traits, TraitedSpec, File, CommandLineInputSpec, InputMultiPath
from nipype.interfaces.base.traits_extension import isdefined
from qsmxt.scripts.qsmxt_functions import extend_fname
import os, shutil

THREAD_CONTROL_VARIABLE = "OMP_NUM_THREADS"


class TGVQSMInputSpec(CommandLineInputSpec):
    phase = File(exists=True, desc='Phase image', mandatory=True, argstr="-p %s")
    mask = InputMultiPath(exists=True, desc='Image mask', mandatory=True, argstr="-m %s")
    num_threads = traits.Int(-1, usedefault=True, nohash=True, desc="Number of threads to use, by default $NCPUS")
    TE = traits.Float(desc='Echo Time [sec]', mandatory=True, argstr="-t %f")
    B0 = traits.Float(desc='Field Strength [Tesla]', mandatory=True, argstr="-f %f")
    extra_arguments = traits.String(desc='Add extra arguments. E.G. --ignore-orientation --no-resampling will ignore orientation of files and do no resampling (for cases where resampling in tgv_qsm fails)',
                               argstr="%s")
    # Only support of one alpha here!
    alpha = traits.List([0.0015, 0.0005], minlen=2, maxlen=2, desc='Regularisation alphas', usedefault=True,
                        argstr="--alpha %s")
    # We only support one iteration - easier to handle in nipype
    iterations = traits.Int(1000, desc='Number of iterations to perform', usedefault=True, argstr="-i %d")
    erosions = traits.Int(5, desc='Number of mask erosions', usedefault=True, argstr="-e %d")
    out_suffix = traits.String("_tgv", desc='Suffix for output files. Will be followed by 000 (reason - see CLI)',
                               usedefault=True, argstr="-o %s")


class TGVQSMOutputSpec(TraitedSpec):
    qsm = File(desc='Computed susceptibility map')


class TGVQSMInterface(CommandLine):
    input_spec = TGVQSMInputSpec
    output_spec = TGVQSMOutputSpec

    # We use here an interface to the CLI utility
    _cmd = "tgv_qsm"

    def __init__(self, **inputs):
        super(TGVQSMInterface, self).__init__(**inputs)
        self.inputs.on_trait_change(self._num_threads_update, 'num_threads')

        if not isdefined(self.inputs.num_threads):
            self.inputs.num_threads = self._num_threads
        else:
            self._num_threads_update()

    def _list_outputs(self):
        outputs = self.output_spec().get()
        
        # TGV-QSM doesn't output files in the current directory for some reason, so we should move it
        outfile_original = extend_fname(self.inputs.phase, "_tgv_000", ext="nii.gz")
        outfile_final = os.path.abspath(os.path.split(outfile_original)[1]).replace("_000.nii.gz", ".nii.gz")
        if not os.path.exists(outfile_final):
            shutil.move(outfile_original, outfile_final)
        
        outputs['qsm'] = outfile_final
        return outputs

    def _num_threads_update(self):
        self._num_threads = self.inputs.num_threads
        if self.inputs.num_threads == -1:
            self.inputs.environ.update({THREAD_CONTROL_VARIABLE: '$NCPUS'})
        else:
            self.inputs.environ.update({THREAD_CONTROL_VARIABLE: '%s' % self.inputs.num_threads})
