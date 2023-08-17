import os

from nipype.interfaces.base import CommandLine, TraitedSpec, File, CommandLineInputSpec, traits
from nipype.interfaces.base.traits_extension import isdefined
from qsmxt.scripts import qsmxt_functions

class CommandLineInputSpecJulia(CommandLineInputSpec):
    num_threads = traits.Int(-1, usedefault=True, desc="Number of threads to use, by default $NCPUS")
    def __init__(self, **inputs): super(CommandLineInputSpecJulia, self).__init__(**inputs)

class CommandLineJulia(CommandLine):
    def __init__(self, **inputs):
        super(CommandLineJulia, self).__init__(**inputs)
        self.inputs.on_trait_change(self._num_threads_update, 'num_threads')

        if not isdefined(self.inputs.num_threads):
            self.inputs.num_threads = self._num_threads
        else:
            self._num_threads_update()

    def _num_threads_update(self):
        self._num_threads = self.inputs.num_threads
        if self.inputs.num_threads == -1:
            cpu_count = max(4, os.environ["NCPUS"] if "NCPUS" in os.environ else str(os.cpu_count()))
            self.inputs.environ.update({ "JULIA_NUM_THREADS" : cpu_count, "JULIA_CPU_THREADS" : cpu_count })
        else:
            self.inputs.environ.update({ "JULIA_NUM_THREADS" : f"{self.inputs.num_threads}", "JULIA_CPU_THREADS" : f"{self.inputs.num_threads}" })

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
    vsz = traits.String(
        argstr="--vsz \"%s\"",
        default="(1,1,1)",
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
    vsz = traits.String(
        argstr="--vsz \"%s\"",
        default="(1,1,1)",
        position=3
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
    frequency = File()


class PhaseToFreqInterface(CommandLineJulia):
    def __init__(self, **inputs): super(PhaseToFreqInterface, self).__init__(**inputs)
    input_spec = PhaseToFreqInputSpec
    output_spec = PhaseToFreqOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "qsmjl_phase_to_frequency.jl")


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
    vsz = traits.String(
        argstr="--vsz \"%s\"",
        default="(1,1,1)",
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
    vsz = traits.String(
        argstr="--vsz \"%s\"",
        default="(1,1,1)",
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
    vsz = traits.String(
        argstr="--vsz \"%s\"",
        default="(1,1,1)",
        position=2
    )
    b0_direction = traits.String(
        argstr="--b0-dir \"%s\"",
        default="(0,0,1)",
        position=3
    )
    qsm = File(
        argstr="--qsm-out %s",
        name_source=['tissue_frequency'],
        name_template='%s_rts.nii',
        position=4
    )


class RtsQsmOutputSpec(TraitedSpec):
    qsm = File(exists=True)


class RtsQsmInterface(CommandLineJulia):
    def __init__(self, **inputs): super(RtsQsmInterface, self).__init__(**inputs)
    input_spec = RtsQsmInputSpec
    output_spec = RtsQsmOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "qsmjl_rts.jl")


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
    vsz = traits.String(
        argstr="--vsz \"%s\"",
        default="(1,1,1)",
        position=2
    )
    b0_direction = traits.String(
        argstr="--b0-dir \"%s\"",
        default="(0,0,1)",
        position=3
    )
    qsm = File(
        argstr="--qsm-out %s",
        name_source=['tissue_frequency'],
        name_template='%s_rts.nii',
        position=4
    )


class TvQsmOutputSpec(TraitedSpec):
    qsm = File(exists=True)


class TvQsmInterface(CommandLineJulia):
    def __init__(self, **inputs): super(TvQsmInterface, self).__init__(**inputs)
    input_spec = TvQsmInputSpec
    output_spec = TvQsmOutputSpec
    _cmd = os.path.join(qsmxt_functions.get_qsmxt_dir(), "scripts", "qsmjl_tv.jl")

