#!/usr/bin/env python3

import json
import os
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, traits, File

class CopyJsonSidecarInputSpec(BaseInterfaceInputSpec):
    source_json = File(mandatory=True, exists=True, desc="Source JSON file to copy")
    target_nifti = File(mandatory=True, exists=True, desc="Target NIfTI file for which to create JSON sidecar")
    additional_image_types = traits.List(traits.String(), desc="Additional ImageType values to add to the list")

class CopyJsonSidecarOutputSpec(TraitedSpec):
    out_json = File(exists=True, desc="Output JSON sidecar file")
    out_nifti = File(exists=True, desc="Pass-through of the input NIfTI file")

class CopyJsonSidecarInterface(SimpleInterface):
    input_spec = CopyJsonSidecarInputSpec
    output_spec = CopyJsonSidecarOutputSpec

    def _run_interface(self, runtime):
        # Read the source JSON file
        with open(self.inputs.source_json, 'r', encoding='utf-8') as f:
            json_data = json.load(f)
        
        # Add additional ImageType values if specified
        if self.inputs.additional_image_types:
            if 'ImageType' not in json_data:
                json_data['ImageType'] = []
            elif not isinstance(json_data['ImageType'], list):
                # Convert to list if it's not already
                json_data['ImageType'] = [json_data['ImageType']]
            
            # Add new ImageType values if they're not already present
            for img_type in self.inputs.additional_image_types:
                if img_type not in json_data['ImageType']:
                    json_data['ImageType'].append(img_type)
        
        # Create output JSON filename based on target NIfTI file
        target_base = os.path.splitext(self.inputs.target_nifti)[0]
        if target_base.endswith('.nii'):
            target_base = os.path.splitext(target_base)[0]
        out_json = target_base + '.json'
        
        # Write the modified JSON data to the output file
        with open(out_json, 'w', encoding='utf-8') as f:
            json.dump(json_data, f, indent=2)
        
        self._results['out_json'] = out_json
        self._results['out_nifti'] = self.inputs.target_nifti
        return runtime