# -*- coding: utf-8 -*-
"""
Created on Sun Aug  3 11:46:42 2014

@author: epracht
"""
from __future__ import division

import os

from nipype.interfaces.base import BaseInterface, traits, TraitedSpec, File, CommandLineInputSpec
from nipype.interfaces.traits_extension import isdefined
from nipype.utils.filemanip import fname_presuffix

# TODO DB Change this to a command line interface and just call the other functions

THREAD_CONTROL_VARIABLE = "OMP_NUM_THREADS"


def gen_filename(fname, suffix, newpath=os.getcwd(), use_ext=True):
    return fname_presuffix(fname, suffix=suffix, newpath=newpath, use_ext=use_ext)


class QSMappingInputSpec(CommandLineInputSpec):
    # TODO This is incomplete and just gives some basic parameters
    file_phase = File(exists=True, desc='Phase image', mandatory=True, argstr="-p %s")
    file_mask = File(exists=True, desc='Image mask', mandatory=True, argstr="-m %s")
    num_threads = traits.Int(1, usedefault=True, nohash=True, desc="Number of threads to use")
    TE = traits.Float(desc='Echo Time [sec]', mandatory=True, argstr="-t %f")
    b0 = traits.Float(desc='Field Strength [Tesla]', mandatory=True, argstr="-f %f")
    # Only support of one alpha here!
    alpha = traits.List([0.0015, 0.0005], minlen=2, maxlen=2, desc='Regularisation alphas', usedefault=True,
                        argstr="-p %s")
    # We only support one iteration - easier to handle in nipype
    iterations = traits.Int(1000, desc='Number of iterations to perform', usedefault=True, argstr="-i %d")
    erosions = traits.Int(5, desc='Number of mask erosions', usedefault=True, argstr="-e %d")
    out_suffix = traits.String("_qsm_recon", desc='Suffix for output files. Will be followed by 000 (reason - see CLI)',
                               usedefault=True, argstr="-o %s")

class QSMappingOutputSpec(TraitedSpec):
    out_qsm = File(desc='Computed susceptibility map')


class QSMappingInterface(BaseInterface):

    input_spec = QSMappingInputSpec
    output_spec = QSMappingOutputSpec

    # We use here an interface to the CLI utility
    cmd = "tgv_qsm"

    def __init__(self, **inputs):
        super(QSMappingInterface, self).__init__(**inputs)
        self.inputs.on_trait_change(self._num_threads_update, 'num_threads')

        if not isdefined(self.inputs.num_threads):
            self.inputs.num_threads = self._num_threads
        else:
            self._num_threads_update()


    def _list_outputs(self):
        outputs = self.output_spec().get()
        outputs['out_qsm'] = gen_filename(self.inputs.file_magnitude, suffix=self.inputs.out_suffix + "000",
                                          newpath=os.getcwd())

        return outputs

    def _num_threads_update(self):
        self._num_threads = self.inputs.num_threads
        if (self.inputs.num_threads == -1):
            pass  # System default
        else:
            self.inputs.environ.update({THREAD_CONTROL_VARIABLE:
                                            '%s' % self.inputs.num_threads})
