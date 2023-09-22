#!/usr/bin/env python3

import os

from nipype.interfaces.base import (
    traits,
    TraitedSpec,
    File,
)
from qsmxt.scripts.qsmxt_functions import get_qsmxt_dir
from qsmxt.interfaces.utils import CommandLineInputSpecJulia, CommandLineJulia


class TGVQSMJlInputSpec(CommandLineInputSpecJulia):
    """Input specification for TGVQSMJlInterface."""
    def __init__(self, **inputs): super(TGVQSMJlInputSpec, self).__init__(**inputs)
    phase = File(mandatory=True, exists=True, argstr="--phase '%s'")
    mask = File(mandatory=True, exists=True, argstr="--mask '%s'")
    erosions = traits.Int(mandatory=True, argstr="--erosions %s")
    TE = traits.Float(mandatory=True, argstr="--TE %s")
    qsm = File(name_source=["phase"], name_template="%s_tgvqsmjl.nii", argstr="--output %s")
    B0 = traits.Float(default_value=3.0, argstr="--b0-str %s")
    regularization = traits.Float(2.0, argstr="--regularization %s")
    alpha = traits.ListFloat(minlen=2, maxlen=2, argstr="--alphas '[%s]'")
    iterations = traits.Int(argstr="--iterations %s")

class TGVQSMJlOutputSpec(TraitedSpec):
    """Output specification for TGVQSMJlInterface."""
    qsm = File()

class TGVQSMJlInterface(CommandLineJulia):
    """Nipype interface for TGV QSM Julia implementation."""
    def __init__(self, **inputs): super(TGVQSMJlInterface, self).__init__(**inputs)
    input_spec = TGVQSMJlInputSpec
    output_spec = TGVQSMJlOutputSpec
    _cmd = os.path.join(get_qsmxt_dir(), "scripts", "tgv_qsm.jl")

