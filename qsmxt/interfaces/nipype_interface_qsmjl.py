import os

from nipype.interfaces.base import TraitedSpec, File, traits
from qsmxt.interfaces.utils import CommandLineInputSpecJulia, CommandLineJulia
from qsmxt.scripts import qsmxt_functions


class LaplacianUnwrappingInputSpec(CommandLineInputSpecJulia):
    def __init__(self, **inputs): super(LaplacianUnwrappingInputSpec, self).__init__(**inputs)
    phase = File(
        exists=True,
        mandatory=True,
        argstr="--phase %s",
        position=0
    )
    mask = File(
        exists=True,
        mandatory=True,
        argstr="--mask %s",
        position=1
    )
    vsz = traits.ListFloat(
        argstr="--vsz '[%s]'",
        default=[1, 1, 1],
        position=2
    )
    phase_unwrapped = File(
        argstr="--unwrapped-phase-out %s",
        name_source=['phase'],
        name_template='%s_unwrapped-laplacian.nii',
        position=3
    )


class LaplacianUnwrappingOutputSpec(TraitedSpec):
    phase_unwrapped = File(exists=True)


class LaplacianUnwrappingInterface(CommandLineJulia):
    def __init__(self, **inputs): super(LaplacianUnwrappingInterface, self).__init__(**inputs)
    input_spec = LaplacianUnwrappingInputSpec
    output_spec = LaplacianUnwrappingOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "qsmjl_laplacian_unwrapping.jl")

    def _list_outputs(self):
        outputs = self._outputs().get()
        outputs["phase_unwrapped"] = qsmxt_functions.extend_fname(self.inputs.phase, "_unwrapped-laplacian", ext="nii", out_dir=os.getcwd())
        return outputs


class PhaseToFreqInputSpec(CommandLineInputSpecJulia):
    def __init__(self, **inputs): super(PhaseToFreqInputSpec, self).__init__(**inputs)
    phase = File(
        exists=True,
        mandatory=True,
        argstr="--phase %s",
        position=0
    )
    TE = traits.Float(
        argstr="--TEs [%s]",
        position=2
    )
    vsz = traits.ListFloat(
        argstr="--vsz '[%s]'",
        default=[1, 1, 1],
        position=2
    )
    B0 = traits.Float(
        argstr="--b0-str %s",
        default=3,
        position=4
    )
    frequency = File(
        argstr="--frequency-out %s",
        name_source=['phase'],
        name_template='%s_freq.nii',
        position=5
    )


class PhaseToFreqOutputSpec(TraitedSpec):
    frequency = File(exists=True)


class PhaseToFreqInterface(CommandLineJulia):
    def __init__(self, **inputs): super(PhaseToFreqInterface, self).__init__(**inputs)
    input_spec = PhaseToFreqInputSpec
    output_spec = PhaseToFreqOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "qsmjl_phase_to_frequency.jl")

    def _list_outputs(self):
        outputs = self._outputs().get()
        outputs["frequency"] = qsmxt_functions.extend_fname(self.inputs.frequency, "_freq", ext="nii", out_dir=os.getcwd())
        return outputs


class VsharpInputSpec(CommandLineInputSpecJulia):
    def __init__(self, **inputs): super(VsharpInputSpec, self).__init__(**inputs)
    frequency = File(
        exists=True,
        mandatory=True,
        argstr="--frequency %s",
        position=0
    )
    mask = File(
        exists=True,
        mandatory=True,
        argstr="--mask %s",
        position=1
    )
    vsz = traits.ListFloat(
        argstr="--vsz '[%s]'",
        default=[1, 1, 1],
        position=2
    )
    tissue_frequency = File(
        argstr="--tissue-frequency-out %s",
        name_source=['frequency'],
        name_template='%s_vsharp.nii',
        position=3
    )
    vsharp_mask = File(
        argstr="--vsharp-mask-out %s",
        name_source=['mask'],
        name_template='%s_vsharp-mask.nii',
        position=4
    )


class VsharpOutputSpec(TraitedSpec):
    tissue_frequency = File(exists=True)
    vsharp_mask = File(exists=True)


class VsharpInterface(CommandLineJulia):
    def __init__(self, **inputs): super(VsharpInterface, self).__init__(**inputs)
    input_spec = VsharpInputSpec
    output_spec = VsharpOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "qsmjl_vsharp.jl")

    def _list_outputs(self):
        outputs = self._outputs().get()
        outputs["tissue_frequency"] = qsmxt_functions.extend_fname(self.inputs.frequency, "_vsharp", ext="nii", out_dir=os.getcwd())
        outputs["vsharp_mask"] = qsmxt_functions.extend_fname(self.inputs.mask, "_vsharp-mask", ext="nii", out_dir=os.getcwd())
        return outputs


class PdfInputSpec(CommandLineInputSpecJulia):
    def __init__(self, **inputs): super(PdfInputSpec, self).__init__(**inputs)
    frequency = File(
        exists=True,
        mandatory=True,
        argstr="--frequency %s",
        position=0
    )
    mask = File(
        exists=True,
        mandatory=True,
        argstr="--mask %s",
        position=1
    )
    vsz = traits.ListFloat(
        argstr="--vsz '[%s]'",
        default=[1, 1, 1],
        position=2
    )
    tissue_frequency = File(
        argstr="--tissue-frequency-out %s",
        name_source=['frequency'],
        name_template='%s_pdf.nii',
        position=3
    )


class PdfOutputSpec(TraitedSpec):
    tissue_frequency = File(exists=True)


class PdfInterface(CommandLineJulia):
    def __init__(self, **inputs): super(PdfInterface, self).__init__(**inputs)
    input_spec = PdfInputSpec
    output_spec = PdfOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "qsmjl_pdf.jl")

    def _list_outputs(self):
        outputs = self._outputs().get()
        outputs["tissue_frequency"] = qsmxt_functions.extend_fname(self.inputs.frequency, "_pdf", ext="nii", out_dir=os.getcwd())
        return outputs


class RtsQsmInputSpec(CommandLineInputSpecJulia):
    def __init__(self, **inputs): super(RtsQsmInputSpec, self).__init__(**inputs)
    tissue_frequency = File(
        exists=True,
        mandatory=True,
        argstr="--tissue-frequency %s",
        position=0
    )
    mask = File(
        exists=True,
        mandatory=True,
        argstr="--mask %s",
        position=1
    )
    vsz = traits.ListFloat(
        argstr="--vsz '[%s]'",
        default=[1, 1, 1],
        position=2
    )
    b0_direction = traits.ListFloat(
        argstr="--b0-dir '[%s]'",
        default=[0,0,1],
        position=3
    )
    tol = traits.Float(
        argstr="--tol %s",
        default=1e-4,
        desc="Stopping tolerance for RTS convergence (default: 1e-4)",
        position=4
    )
    delta_threshold = traits.Float(
        argstr="--delta %s",
        default=0.15,
        desc="Threshold for ill-conditioned k-space region (default: 0.15)",
        position=5
    )
    mu_regularization = traits.Float(
        argstr="--mu %s",
        default=1e5,
        desc="Mu regularization parameter for TV minimization (default: 1e5)",
        position=6
    )
    qsm = File(
        argstr="--qsm-out %s",
        name_source=['tissue_frequency'],
        name_template='%s_rts.nii',
        position=7
    )


class RtsQsmOutputSpec(TraitedSpec):
    qsm = File(exists=True)


class RtsQsmInterface(CommandLineJulia):
    def __init__(self, **inputs): super(RtsQsmInterface, self).__init__(**inputs)
    input_spec = RtsQsmInputSpec
    output_spec = RtsQsmOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "qsmjl_rts.jl")

    def _list_outputs(self):
        outputs = self._outputs().get()
        outputs["qsm"] = qsmxt_functions.extend_fname(self.inputs.tissue_frequency, "_rts", ext="nii", out_dir=os.getcwd())
        return outputs

class TvQsmInputSpec(CommandLineInputSpecJulia):
    def __init__(self, **inputs): super(TvQsmInputSpec, self).__init__(**inputs)
    tissue_frequency = File(
        exists=True,
        mandatory=True,
        argstr="--tissue-frequency %s",
        position=0
    )
    mask = File(
        exists=True,
        mandatory=True,
        argstr="--mask %s",
        position=1
    )
    vsz = traits.ListFloat(
        argstr="--vsz '[%s]'",
        default=[1, 1, 1],
        position=2
    )
    b0_direction = traits.ListFloat(
        argstr="--b0-dir '[%s]'",
        default=[0,0,1],
        position=3
    )
    qsm = File(
        argstr="--qsm-out %s",
        name_source=['tissue_frequency'],
        name_template='%s_tv.nii',
        position=4
    )


class TvQsmOutputSpec(TraitedSpec):
    qsm = File(exists=True)


class TvQsmInterface(CommandLineJulia):
    def __init__(self, **inputs): super(TvQsmInterface, self).__init__(**inputs)
    input_spec = TvQsmInputSpec
    output_spec = TvQsmOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "qsmjl_tv.jl")

    def _list_outputs(self):
        outputs = self._outputs().get()
        outputs["qsm"] = qsmxt_functions.extend_fname(self.inputs.tissue_frequency, "_tv", ext="nii", out_dir=os.getcwd())
        return outputs

