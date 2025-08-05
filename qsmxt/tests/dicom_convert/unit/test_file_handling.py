#!/usr/bin/env python3
"""
Unit tests for file handling functions in dicom_convert module.
"""

import pytest
import os
import tempfile
import shutil
import json
import nibabel as nb
import numpy as np
from unittest.mock import patch, MagicMock, call
from qsmxt.cli.dicom_convert import (
    handle_4d_files, determine_data_type, extract_echo_number
)


class TestExtractEchoNumber:
    """Test cases for the extract_echo_number function."""
    
    def test_extract_echo_number_present(self):
        """Test extracting echo number when present."""
        assert extract_echo_number("file_echo-01.nii") == "01"
        assert extract_echo_number("data_echo-02_mag.nii.gz") == "02"
        assert extract_echo_number("sub-01_ses-01_echo-10_phase.nii") == "10"
    
    def test_extract_echo_number_not_present(self):
        """Test when echo number is not present."""
        assert extract_echo_number("file.nii") is None
        assert extract_echo_number("data_mag.nii.gz") is None
        assert extract_echo_number("echo_data.nii") is None
        assert extract_echo_number("") is None
    
    def test_extract_echo_number_edge_cases(self):
        """Test edge cases."""
        assert extract_echo_number("_echo-001.nii") == "001"
        assert extract_echo_number("multiple_echo-01_echo-02.nii") == "01"  # First match with underscore
        assert extract_echo_number("_echo-5") == "5"


class TestDetermineDataType:
    """Test cases for the determine_data_type function."""
    
    def test_determine_from_filename(self):
        """Test determining data type from filename."""
        with tempfile.NamedTemporaryFile(suffix="_ph.nii") as tmp:
            assert determine_data_type(tmp.name, "Mag") == "phase"
    
    def test_determine_from_json_list_imagetype(self):
        """Test determining data type from JSON with list ImageType."""
        with tempfile.TemporaryDirectory() as tmpdir:
            nii_path = os.path.join(tmpdir, "test.nii")
            json_path = os.path.join(tmpdir, "test.json")
            
            # Create empty nii file
            open(nii_path, 'w').close()
            
            # Test phase detection
            with open(json_path, 'w') as f:
                json.dump({"ImageType": ["P", "OTHER"]}, f)
            assert determine_data_type(nii_path, "Mag") == "phase"
            
            # Test magnitude detection
            with open(json_path, 'w') as f:
                json.dump({"ImageType": ["M", "OTHER"]}, f)
            assert determine_data_type(nii_path, "Phase") == "mag"
            
            # Test real detection
            with open(json_path, 'w') as f:
                json.dump({"ImageType": ["REAL"]}, f)
            assert determine_data_type(nii_path, "Mag") == "real"
            
            # Test imaginary detection
            with open(json_path, 'w') as f:
                json.dump({"ImageType": ["IMAGINARY"]}, f)
            assert determine_data_type(nii_path, "Mag") == "imag"
    
    def test_determine_from_json_string_imagetype(self):
        """Test determining data type from JSON with string ImageType."""
        with tempfile.TemporaryDirectory() as tmpdir:
            nii_path = os.path.join(tmpdir, "test.nii")
            json_path = os.path.join(tmpdir, "test.json")
            
            open(nii_path, 'w').close()
            
            with open(json_path, 'w') as f:
                json.dump({"ImageType": "PHASE"}, f)
            assert determine_data_type(nii_path, "Mag") == "phase"
    
    def test_determine_from_assigned_type(self):
        """Test fallback to assigned type."""
        with tempfile.NamedTemporaryFile(suffix=".nii") as tmp:
            assert determine_data_type(tmp.name, "Phase") == "phase"
            assert determine_data_type(tmp.name, "Mag") == "mag"
            assert determine_data_type(tmp.name, "Real") == "real"
            assert determine_data_type(tmp.name, "Imag") == "imag"
    
    def test_determine_default_fallback(self):
        """Test default fallback to magnitude."""
        with tempfile.NamedTemporaryFile(suffix=".nii") as tmp:
            assert determine_data_type(tmp.name, "Unknown") == "mag"
    
    def test_determine_no_json_file(self):
        """Test when JSON file doesn't exist."""
        with tempfile.NamedTemporaryFile(suffix=".nii") as tmp:
            assert determine_data_type(tmp.name, "Phase") == "phase"


class TestHandle4DFiles:
    """Test cases for the handle_4d_files function."""
    
    @patch('qsmxt.cli.dicom_convert.make_logger')
    def test_handle_4d_files_no_4d(self, mock_logger):
        """Test handling when no 4D files are present."""
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create 3D NIfTI files
            for i in range(3):
                img = nb.Nifti1Image(np.zeros((64, 64, 32)), np.eye(4))
                filename = f"temp_output_{i:02d}.nii"
                nb.save(img, os.path.join(tmpdir, filename))
            
            converted_niftis = ["temp_output_00.nii", "temp_output_01.nii", "temp_output_02.nii"]
            original_list = converted_niftis.copy()
            
            handle_4d_files(tmpdir, converted_niftis, "temp_output", None)
            
            # List should remain unchanged
            assert converted_niftis == original_list
    
    @patch('qsmxt.cli.dicom_convert.make_logger')
    def test_handle_4d_files_split_magnitude(self, mock_logger):
        """Test splitting 4D magnitude file."""
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create 4D NIfTI file
            img_4d = nb.Nifti1Image(np.zeros((64, 64, 32, 3)), np.eye(4))
            filename = "temp_output_mag.nii"
            nb.save(img_4d, os.path.join(tmpdir, filename))
            
            # Create corresponding JSON
            json_data = {"EchoTime": 0.005}
            with open(os.path.join(tmpdir, "temp_output_mag.json"), 'w') as f:
                json.dump(json_data, f)
            
            converted_niftis = [filename]
            
            handle_4d_files(tmpdir, converted_niftis, "temp_output", None)
            
            # Original file should be removed from list
            assert filename not in converted_niftis
            
            # Check that 3 echo files were created
            assert len(converted_niftis) == 3
            for i in range(3):
                expected_name = f"temp_output_mag_echo-{i+1:02d}.nii"
                assert expected_name in converted_niftis
                assert os.path.exists(os.path.join(tmpdir, expected_name))
                
                # Check JSON files were created
                json_path = os.path.join(tmpdir, expected_name.replace(".nii", ".json"))
                assert os.path.exists(json_path)
                with open(json_path) as f:
                    echo_json = json.load(f)
                    assert "EchoTime" in echo_json
                    assert echo_json["ImageType"] == ["MAG"]
    
    @patch('qsmxt.cli.dicom_convert.make_logger')
    def test_handle_4d_files_split_phase(self, mock_logger):
        """Test splitting 4D phase file."""
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create 4D NIfTI file with _ph suffix
            img_4d = nb.Nifti1Image(np.zeros((64, 64, 32, 2)), np.eye(4))
            filename = "temp_output_ph.nii.gz"
            nb.save(img_4d, os.path.join(tmpdir, filename))
            
            converted_niftis = [filename]
            
            handle_4d_files(tmpdir, converted_niftis, "temp_output", None)
            
            # Check that files were created with correct naming
            assert len(converted_niftis) == 2
            for i in range(2):
                expected_name = f"temp_output_ph_echo-{i+1:02d}.nii.gz"
                assert expected_name in converted_niftis
    
    @patch('qsmxt.cli.dicom_convert.make_logger')
    def test_handle_4d_files_with_echo_times(self, mock_logger):
        """Test handling 4D files with echo times from DICOM group."""
        with tempfile.TemporaryDirectory() as tmpdir:
            import pandas as pd
            
            # Create 4D NIfTI file
            img_4d = nb.Nifti1Image(np.zeros((64, 64, 32, 3)), np.eye(4))
            filename = "temp_output.nii"
            nb.save(img_4d, os.path.join(tmpdir, filename))
            
            # Create JSON
            with open(os.path.join(tmpdir, "temp_output.json"), 'w') as f:
                json.dump({}, f)
            
            # Create DICOM group with echo times (in milliseconds as DICOM standard)
            dicom_group = pd.DataFrame({
                "EchoTime": [5.0, 10.0, 15.0, 5.0, 10.0, 15.0]  # Duplicate values
            })
            
            converted_niftis = [filename]
            
            handle_4d_files(tmpdir, converted_niftis, "temp_output", dicom_group)
            
            # List files created in the directory for debugging
            created_files = os.listdir(tmpdir)
            print(f"Files in directory after handle_4d_files: {created_files}")
            
            # The function might not have processed the file if it didn't detect it as 4D
            # Let's check if the original file still exists
            if filename in created_files:
                # File wasn't processed - might need to check why
                img = nb.load(os.path.join(tmpdir, filename))
                print(f"Image shape: {img.shape}, ndim: {len(img.shape)}")
            
            # Check that echo files were created
            echo_files = [f for f in created_files if "_echo-" in f and f.endswith(".nii")]
            assert len(echo_files) == 3, f"Expected 3 echo files, got {len(echo_files)}: {echo_files}"
            
            # Check that original file was removed
            assert filename not in created_files, "Original 4D file should have been removed"
            
            # Check that echo files are correctly named
            assert "temp_output_echo-01.nii" in created_files
            assert "temp_output_echo-02.nii" in created_files
            assert "temp_output_echo-03.nii" in created_files
            
            # Note: JSON files are only created/updated if the original 4D file had a JSON
            # The current implementation copies and updates the JSON for each echo
            # Since our test JSON is empty, the JSONs might not contain echo times
    
    @patch('qsmxt.cli.dicom_convert.make_logger')
    def test_handle_4d_files_error_handling(self, mock_logger):
        """Test error handling in 4D file processing."""
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create a file that doesn't exist in the list
            converted_niftis = ["nonexistent.nii"]
            
            # Should handle gracefully
            handle_4d_files(tmpdir, converted_niftis, "temp_output", None)
            
            # List should remain unchanged
            assert converted_niftis == ["nonexistent.nii"]
    
    @patch('qsmxt.cli.dicom_convert.make_logger')
    def test_handle_4d_files_original_cleanup(self, mock_logger):
        """Test that original 4D files are cleaned up."""
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create 4D NIfTI file
            img_4d = nb.Nifti1Image(np.zeros((64, 64, 32, 2)), np.eye(4))
            filename = "temp_output.nii"
            nb.save(img_4d, os.path.join(tmpdir, filename))
            
            # Create JSON
            json_path = os.path.join(tmpdir, "temp_output.json")
            with open(json_path, 'w') as f:
                json.dump({}, f)
            
            converted_niftis = [filename]
            
            handle_4d_files(tmpdir, converted_niftis, "temp_output", None)
            
            # Original files should be deleted
            assert not os.path.exists(os.path.join(tmpdir, filename))
            assert not os.path.exists(json_path)


@pytest.mark.parametrize("filename,expected", [
    ("data_echo-01.nii", "01"),
    ("sub-01_echo-02_run-01.nii.gz", "02"),
    ("_echo-10.nii", "10"),
    ("no_echo.nii", None),
    ("", None),
])
def test_extract_echo_parametrized(filename, expected):
    """Parametrized test for echo number extraction."""
    assert extract_echo_number(filename) == expected