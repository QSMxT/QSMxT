from nipype.interfaces.base import CommandLine, traits, TraitedSpec, File, CommandLineInputSpec


class MakeHomogeneousInputSpec(CommandLineInputSpec):
    in_file = File(
        exists=True,
        mandatory=True,
        argstr="%s",
        position=0
    )
    out_file = File(
        argstr="%s",
        name_source=['in_file'],
        name_template='%s_makehomogeneous.nii',
        position=1
    )


class MakeHomogeneousOutputSpec(TraitedSpec):
    out_file = File()


class MakeHomogeneousInterface(CommandLine):
    input_spec = MakeHomogeneousInputSpec
    output_spec = MakeHomogeneousOutputSpec
    _cmd = "julia /home/ashley/repos/imaging_pipelines/scripts/makehomogeneous.jl"
