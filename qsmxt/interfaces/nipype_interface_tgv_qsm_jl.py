#!/usr/bin/env python3

import os

from nipype.interfaces.base import  traits, CommandLine, BaseInterfaceInputSpec, TraitedSpec, File
from qsmxt.scripts.qsmxt_functions import extend_fname, get_qsmxt_dir

class TGVQSMJlInputSpec(BaseInterfaceInputSpec):
    phase = File(mandatory=True, exists=True, argstr="--phase '%s'")
    mask = File(mandatory=True, exists=True, argstr="--mask '%s'")
    TE = traits.Float(mandatory=True, argstr="--TE %s'")
    vsz = traits.String(value="(1,1,1)", argstr="--vsz \"%s\"")
    B0 = traits.Float(default_value=3, argstr="--b0-str %s")
    qsm = File(name_source=['phase'], name_template='%s_tgvqsmjl.nii', argstr="--output %s")

class TGVQSMJlOutputSpec(TraitedSpec):
    qsm = File()

class TGVQSMJlInterface(CommandLine):
    input_spec = TGVQSMJlInputSpec
    output_spec = TGVQSMJlOutputSpec
    _cmd = os.path.join(get_qsmxt_dir(), "scripts", "tgv_qsm.jl")

