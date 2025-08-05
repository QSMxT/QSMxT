#!/usr/bin/env pytest
import os
import json
import tempfile
import pytest
import shutil

from qsmxt.interfaces.nipype_interface_copy_json_sidecar import CopyJsonSidecarInterface


def test_copy_json_sidecar_interface():
    """Test the CopyJsonSidecarInterface directly."""
    
    # Create temporary directory for test files
    with tempfile.TemporaryDirectory() as temp_dir:
        # Create a test source JSON file
        source_json_data = {
            "EchoTime": 0.012,
            "MagneticFieldStrength": 3.0,
            "ImageType": ["ORIGINAL", "PRIMARY", "M", "NORM"]
        }
        source_json_path = os.path.join(temp_dir, "source.json")
        with open(source_json_path, 'w') as f:
            json.dump(source_json_data, f)
        
        # Create a test NIfTI file (just an empty file for this test)
        nifti_path = os.path.join(temp_dir, "test_Chimap.nii.gz")
        with open(nifti_path, 'w') as f:
            f.write("fake nifti data")
        
        # Test the interface
        interface = CopyJsonSidecarInterface()
        interface.inputs.source_json = source_json_path
        interface.inputs.target_nifti = nifti_path
        interface.inputs.additional_image_types = ["QSM"]
        
        # Run the interface
        result = interface.run()
        
        # Check outputs
        assert hasattr(result.outputs, 'out_json')
        assert hasattr(result.outputs, 'out_nifti')
        assert result.outputs.out_nifti == nifti_path
        
        # Check that JSON file was created
        expected_json_path = os.path.join(temp_dir, "test_Chimap.json")
        assert os.path.exists(expected_json_path)
        assert result.outputs.out_json == expected_json_path
        
        # Check JSON content
        with open(result.outputs.out_json, 'r') as f:
            output_json = json.load(f)
        
        # Verify that original metadata is preserved
        assert output_json["EchoTime"] == 0.012
        assert output_json["MagneticFieldStrength"] == 3.0
        
        # Verify that ImageType was modified correctly
        assert "ImageType" in output_json
        assert isinstance(output_json["ImageType"], list)
        assert "QSM" in output_json["ImageType"]
        assert "ORIGINAL" in output_json["ImageType"]  # Original values preserved
        

def test_copy_json_sidecar_interface_no_existing_imagetype():
    """Test the interface when source JSON has no ImageType field."""
    
    with tempfile.TemporaryDirectory() as temp_dir:
        # Create a test source JSON file without ImageType
        source_json_data = {
            "EchoTime": 0.020,
            "MagneticFieldStrength": 7.0
        }
        source_json_path = os.path.join(temp_dir, "source.json")
        with open(source_json_path, 'w') as f:
            json.dump(source_json_data, f)
        
        # Create a test NIfTI file
        nifti_path = os.path.join(temp_dir, "test_Chimap.nii")
        with open(nifti_path, 'w') as f:
            f.write("fake nifti data")
        
        # Test the interface
        interface = CopyJsonSidecarInterface()
        interface.inputs.source_json = source_json_path
        interface.inputs.target_nifti = nifti_path
        interface.inputs.additional_image_types = ["QSM"]
        
        # Run the interface
        result = interface.run()
        
        # Check that JSON file was created
        expected_json_path = os.path.join(temp_dir, "test_Chimap.json")
        assert os.path.exists(expected_json_path)
        
        # Check JSON content
        with open(result.outputs.out_json, 'r') as f:
            output_json = json.load(f)
        
        # Verify that ImageType was created with QSM
        assert "ImageType" in output_json
        assert isinstance(output_json["ImageType"], list)
        assert output_json["ImageType"] == ["QSM"]


def test_copy_json_sidecar_interface_string_imagetype():
    """Test the interface when source JSON has ImageType as string instead of list."""
    
    with tempfile.TemporaryDirectory() as temp_dir:
        # Create a test source JSON file with ImageType as string
        source_json_data = {
            "EchoTime": 0.015,
            "ImageType": "ORIGINAL"
        }
        source_json_path = os.path.join(temp_dir, "source.json")
        with open(source_json_path, 'w') as f:
            json.dump(source_json_data, f)
        
        # Create a test NIfTI file
        nifti_path = os.path.join(temp_dir, "test_desc-singlepass_Chimap.nii.gz")
        with open(nifti_path, 'w') as f:
            f.write("fake nifti data")
        
        # Test the interface
        interface = CopyJsonSidecarInterface()
        interface.inputs.source_json = source_json_path
        interface.inputs.target_nifti = nifti_path
        interface.inputs.additional_image_types = ["QSM"]
        
        # Run the interface
        result = interface.run()
        
        # Check that JSON file was created
        expected_json_path = os.path.join(temp_dir, "test_desc-singlepass_Chimap.json")
        assert os.path.exists(expected_json_path)
        
        # Check JSON content
        with open(result.outputs.out_json, 'r') as f:
            output_json = json.load(f)
        
        # Verify that ImageType was converted to list and QSM was added
        assert "ImageType" in output_json
        assert isinstance(output_json["ImageType"], list)
        assert "QSM" in output_json["ImageType"]
        assert "ORIGINAL" in output_json["ImageType"]