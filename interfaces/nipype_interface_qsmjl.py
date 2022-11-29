from nipype.interfaces.base import CommandLine, TraitedSpec, File, CommandLineInputSpec, traits
from scripts import qsmxt_functions
import os

class LaplacianUnwrappingInputSpec(CommandLineInputSpec):
    in_phase = File(
        exists=True,
        mandatory=True,
        argstr="--phase %s",
        position=0
    )
    in_mask = File(
        exists=True,
        mandatory=True,
        argstr="--mask %s",
        position=1
    )
    in_vsz = traits.Tuple(
        argstr="--vsz %s",
        default=(1, 1, 1),
        position=2
    )
    out_unwrapped = File(
        argstr="--unwrapped-phase-out %s",
        name_source=['in_phase'],
        name_template='%s_laplacian-unwrapped.nii',
        position=3
    )


class LaplacianUnwrappingOutputSpec(TraitedSpec):
    out_unwrapped = File()


class LaplacianUnwrappingInterface(CommandLine):
    input_spec = LaplacianUnwrappingInputSpec
    output_spec = LaplacianUnwrappingOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "qsmjl_laplacian_unwrapping.jl")


class PhaseToFreqInputSpec(CommandLineInputSpec):
    in_phase = File(
        exists=True,
        mandatory=True,
        argstr="--phase %s",
        position=0
    )
    in_mask = File(
        exists=True,
        mandatory=True,
        argstr="--mask %s",
        position=1
    )
    in_TEs = traits.Tuple(
        argstr="--TEs [%s]",
        default=(1, 1, 1),
        position=2
    )
    in_vsz = traits.Tuple(
        argstr="--vsz %s",
        default=(1, 1, 1),
        position=3
    )
    in_b0str = traits.Float(
        argstr="--b0-str %s",
        default=3,
        position=4
    )
    out_frequency = File(
        argstr="--frequency_out %s",
        name_source=['in_phase'],
        name_template='%s_freq.nii',
        position=5
    )


class PhaseToFreqOutputSpec(TraitedSpec):
    out_frequency = File()


class PhaseToFreqInterface(CommandLine):
    input_spec = PhaseToFreqInputSpec
    output_spec = PhaseToFreqOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "qsmjl_phase_to_frequency.jl")


class VsharpInputSpec(CommandLineInputSpec):
    in_frequency = File(
        exists=True,
        mandatory=True,
        argstr="--frequency %s",
        position=0
    )
    in_mask = File(
        exists=True,
        mandatory=True,
        argstr="--mask %s",
        position=1
    )
    in_vsz = traits.Tuple(
        argstr="--vsz %s",
        default=(1, 1, 1),
        position=2
    )
    out_freq = File(
        argstr="--frequency_out %s",
        name_source=['in_freq'],
        name_template='%s_freq.nii',
        position=5
    )
    out_mask = File(
        argstr="--frequency_out %s",
        name_source=['in_mask'],
        name_template='%s_vsharp.nii',
        position=6
    )


class VsharpOutputSpec(TraitedSpec):
    out_freq = File()
    out_mask = File()


class PhaseToFreqInterface(CommandLine):
    input_spec = VsharpInputSpec
    output_spec = VsharpOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "qsmjl_vsharp.jl")


class QsmInputSpec(CommandLineInputSpec):
    in_frequency = File(
        exists=True,
        mandatory=True,
        argstr="--tissue-frequency %s",
        position=0
    )
    in_mask = File(
        exists=True,
        mandatory=True,
        argstr="--mask %s",
        position=1
    )
    in_vsz = traits.Tuple(
        argstr="--vsz %s",
        default=(1, 1, 1),
        position=2
    )
    in_b0dir = traits.Float(
        argstr="--b0-dir %s",
        default=(0, 0, 1),
        position=3
    )
    out_qsm = File(
        argstr="--qsm-out %s",
        name_source=['in_frequency'],
        name_template='%s_qsm.nii',
        position=4
    )


class QsmOutputSpec(TraitedSpec):
    out_qsm = File()


class QsmInterface(CommandLine):
    input_spec = QsmInputSpec
    output_spec = QsmOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "qsmjl_inversion.jl")

