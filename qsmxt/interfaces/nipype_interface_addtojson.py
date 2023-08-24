#!/usr/bin/env python3

import json
import os
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, traits, File

def load_json(path):
    with open(path, encoding='utf-8') as f:
        j = json.load(f)
    return j


class AddToJsonInputSpec(BaseInterfaceInputSpec):
    in_file = File(mandatory=True, exists=True)
    in_key = traits.String(mandatory=True)
    in_str_value = traits.String()
    in_num_value = traits.Float()
    in_obj_value = traits.Dict()
    in_arr_value = traits.Array()
    in_bool_value = traits.Bool()
    in_isnull_value = traits.Bool()


class AddToJsonOutputSpec(TraitedSpec):
    out_file = traits.File()


class AddToJsonInterface(SimpleInterface):
    input_spec = AddToJsonInputSpec
    output_spec = AddToJsonOutputSpec

    def _run_interface(self, runtime):
        json_dict = load_json(self.inputs.in_file)
        
        key = self.inputs.in_key
        json_dict[key] = None
        if self.inputs.in_str_value:
            json_dict[key] = self.inputs.in_str_value
            val_type = "string"
        elif self.inputs.in_num_value:
            json_dict[key] = self.inputs.in_num_value
            val_type = "number"
        elif self.inputs.in_obj_value:
            json_dict[key] = self.inputs.in_obj_value
            val_type = "object"
        elif len(self.inputs.in_arr_value):
            json_dict[key] = [val for val in self.inputs.in_arr_value]
            val_type = "array"
        elif self.inputs.in_bool_value:
            json_dict[key] = self.inputs.in_bool_value
            val_type = "bool"
        else:
            val_type = "null"

        out_file = f"{os.path.abspath(os.path.splitext(os.path.split(self.inputs.in_file)[1])[0])}_add-{val_type}.json"

        with open(out_file, 'w', encoding='utf-8') as json_file:
            json.dump(json_dict, json_file)

        self._results['out_file'] = out_file

        return runtime

