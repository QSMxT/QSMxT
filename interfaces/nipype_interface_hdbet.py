import os
import shutil
from nipype.interfaces.base import CommandLine, TraitedSpec, traits, File, CommandLineInputSpec


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
        
        outfile_original = f"{self.inputs.in_file.split('.')[0]}_bet_mask.nii.gz"
        outfile_final = os.path.abspath(os.path.split(outfile_original)[1])
        if not os.path.exists(outfile_final):
            shutil.move(outfile_original, outfile_final)
        
        outputs['mask'] = outfile_final
        return outputs

