import os
import shutil
from nipype.interfaces.base import CommandLine, TraitedSpec, traits, File, CommandLineInputSpec
from qsmxt.scripts.qsmxt_functions import extend_fname


class HDBETInputSpec(CommandLineInputSpec):
    in_file = File(
        exists=True,
        mandatory=True,
        argstr="-i %s",
        position=0
    )


class HDBETOutputSpec(TraitedSpec):
    mask = File(exists=True)


class HDBETInterface(CommandLine):
    input_spec = HDBETInputSpec
    output_spec = HDBETOutputSpec
    _cmd = "hd-bet -device cpu -mode fast -tta 0"

    def _list_outputs(self):
        outputs = self.output_spec().get()
        outfile_original = extend_fname(self.inputs.in_file, "_bet_mask", ext="nii.gz")
        outfile_final = extend_fname(self.inputs.in_file, "_bet_mask", ext="nii.gz", out_dir=os.getcwd())
        if not os.path.exists(outfile_final):
            shutil.move(outfile_original, outfile_final)
        
        outputs['mask'] = outfile_final
        return outputs

