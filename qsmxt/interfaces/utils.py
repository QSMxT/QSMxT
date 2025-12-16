import os
import shutil
from nipype.interfaces.base import CommandLine, CommandLineInputSpec, traits
from nipype.interfaces.base.traits_extension import isdefined

class CommandLineInputSpecJulia(CommandLineInputSpec):
    num_threads = traits.Int(-1, usedefault=True, desc="Number of threads to use, by default 4")
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
            self._num_threads = 4

    @property
    def cmdline(self):
        # Get the original command line from parent
        original_cmdline = super().cmdline
        # Find julia executable
        julia_exe = shutil.which("julia") or "julia"
        # Prepend julia with --threads argument
        return f"{julia_exe} --threads={self._num_threads} {original_cmdline}"

