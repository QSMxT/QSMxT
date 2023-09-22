import os
from nipype.interfaces.base import CommandLine, CommandLineInputSpec, traits
from nipype.interfaces.base.traits_extension import isdefined

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
            cpu_count = max(4, int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
            self.inputs.environ.update({ "JULIA_NUM_THREADS" : str(cpu_count), "JULIA_CPU_THREADS" : str(cpu_count) })
        else:
            self.inputs.environ.update({ "JULIA_NUM_THREADS" : f"{self.inputs.num_threads}", "JULIA_CPU_THREADS" : f"{self.inputs.num_threads}" })

