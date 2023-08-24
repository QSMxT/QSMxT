#!/usr/bin/env python3

import json
import os
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, File, traits

class JsonInputSpec(BaseInterfaceInputSpec):
    in_dict = traits.Dict(mandatory=True)
    out_file = File()

class JsonOutputSpec(TraitedSpec):
    out_file = File()

class JsonInterface(SimpleInterface):
    input_spec = JsonInputSpec
    output_spec = JsonOutputSpec

    def _run_interface(self, runtime):
        self._results['out_file'] = os.path.abspath(self.inputs.out_file)
        with open(os.path.abspath(self.inputs.out_file), 'w', encoding='utf-8') as json_file:
            json.dump(self.inputs.in_dict, json_file)
        return runtime

