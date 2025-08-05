#!/usr/bin/env python3
"""
Unit tests for conversion and organization functions in dicom_convert module.
"""

import pytest
import os
import tempfile
import shutil
import json
import pandas as pd
import nibabel as nb
import numpy as np
from unittest.mock import patch, MagicMock, call
from qsmxt.cli.dicom_convert import (
    process_dicom_group, convert_and_organize, script_exit
)
from qsmxt.tests.dicom_convert.fixtures.mock_dicom_data import (
    create_complete_dicom_dataframe, create_mag_phase_pair
)
try:
    from qsmxt.tests.dicom_convert.fixtures.dicom_generator import (
        create_dicom_series, create_magnitude_phase_pair
    )
    import pydicom
    PYDICOM_AVAILABLE = True
except ImportError:
    PYDICOM_AVAILABLE = False


def create_dataframe_from_dicom_files(dicom_files, acquisition="gre", series_description="Test Series"):
    """Create a DataFrame from actual DICOM files that matches the expected format."""
    if not PYDICOM_AVAILABLE:
        pytest.skip("pydicom not available for reading DICOM files")
    
    rows = []
    for dicom_file in dicom_files:
        ds = pydicom.dcmread(dicom_file)
        
        row = {
            'PatientID': ds.PatientID,
            'PatientName': str(ds.PatientName),
            'StudyDate': ds.StudyDate,
            'SeriesDescription': series_description,
            'Acquisition': acquisition,
            'ImageType': tuple(ds.ImageType) if hasattr(ds, 'ImageType') else ('ORIGINAL', 'PRIMARY'),
            'EchoTime': float(ds.EchoTime) if hasattr(ds, 'EchoTime') else 5.0,
            'SeriesInstanceUID': ds.SeriesInstanceUID,
            'InstanceNumber': int(ds.InstanceNumber),
            'DICOM_Path': dicom_file,
            'Count': len(dicom_files),
            # Add required fields for process_dicom_group
            'Type': 'Mag',
            'Description': '',
            'NumRuns': 1,
            'NumEchoes': 1,
            'EchoNumber': 1,
        }
        rows.append(row)
    
    return pd.DataFrame(rows)


class TestScriptExit:
    """Test cases for the script_exit function."""
    
    @patch('qsmxt.cli.dicom_convert.make_logger')
    @patch('qsmxt.cli.dicom_convert.show_warning_summary')
    @patch('builtins.exit')
    def test_script_exit_default(self, mock_exit, mock_warning, mock_logger):
        """Test script exit with default code."""
        script_exit()
        
        mock_logger.assert_called_once()
        mock_warning.assert_called_once()
        mock_logger.return_value.log.assert_called_with(20, 'Finished')  # LogLevel.INFO = 20
        mock_exit.assert_called_once_with(0)
    
    @patch('qsmxt.cli.dicom_convert.make_logger')
    @patch('qsmxt.cli.dicom_convert.show_warning_summary')
    @patch('builtins.exit')
    def test_script_exit_with_code(self, mock_exit, mock_warning, mock_logger):
        """Test script exit with specific exit code."""
        script_exit(1)
        
        mock_exit.assert_called_once_with(1)


class TestProcessDicomGroup:
    """Test cases for the process_dicom_group function."""
    
    @patch('qsmxt.cli.dicom_convert.make_logger')
    @patch('os.system')
    @patch('qsmxt.cli.dicom_convert.handle_4d_files')
    @patch('qsmxt.cli.dicom_convert.determine_data_type')
    def test_process_dicom_group_basic(self, mock_determine_type, mock_handle_4d, 
                                      mock_system, mock_logger):
        """Test basic DICOM group processing."""
        if not PYDICOM_AVAILABLE:
            pytest.skip("pydicom not available for creating test DICOM files")
            
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create actual DICOM files with specific date
            dicom_dir = os.path.join(tmpdir, "dicoms")
            os.makedirs(dicom_dir, exist_ok=True)
            from qsmxt.tests.dicom_convert.fixtures.dicom_generator import create_dicom_file
            
            dicom_files = []
            for i in range(2):
                filename = os.path.join(dicom_dir, f"dicom_{i:03d}.dcm")
                create_dicom_file(
                    filename=filename,
                    patient_id="patient1",
                    patient_name="Patient^One",
                    study_date="20230101",
                    series_description="GRE",
                    image_type=['ORIGINAL', 'PRIMARY', 'M'],
                    instance_number=i+1,
                    series_number=1
                )
                dicom_files.append(filename)
            
            # Create DataFrame from actual DICOM files
            grp_data = create_dataframe_from_dicom_files(
                dicom_files, 
                acquisition="gre", 
                series_description="GRE"
            )
            
            # Mock file operations
            mock_determine_type.return_value = "mag"
            
            # Create mock NIfTI files after "conversion"
            def create_nifti_side_effect(cmd):
                # Extract output directory from command
                if "dcm2niix" in cmd:
                    nii_path = os.path.join(tmpdir, "temp_convert", "temp_output.nii")
                    os.makedirs(os.path.dirname(nii_path), exist_ok=True)
                    # Create dummy NIfTI
                    img = nb.Nifti1Image(np.zeros((64, 64, 32)), np.eye(4))
                    nb.save(img, nii_path)
                    # Create JSON
                    with open(nii_path.replace('.nii', '.json'), 'w') as f:
                        json.dump({}, f)
                return 0
            
            mock_system.side_effect = create_nifti_side_effect
            
            # Run the function
            process_dicom_group(grp_data, tmpdir, "dcm2niix")
            
            # Verify dcm2niix was called
            assert mock_system.called
            dcm2niix_call = mock_system.call_args_list[0][0][0]
            assert "dcm2niix" in dcm2niix_call
            assert "-o" in dcm2niix_call
            assert "-f" in dcm2niix_call
            
            # Verify output structure was created
            expected_output = os.path.join(tmpdir, "sub-patient1", "ses-20230101", "anat")
            assert os.path.exists(expected_output)
    
    @patch('qsmxt.cli.dicom_convert.make_logger')
    @patch('os.system')
    @patch('qsmxt.cli.dicom_convert.handle_4d_files')
    def test_process_dicom_group_t1w(self, mock_handle_4d, mock_system, mock_logger):
        """Test processing T1w data."""
        if not PYDICOM_AVAILABLE:
            pytest.skip("pydicom not available for creating test DICOM files")
            
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create actual T1w DICOM file
            dicom_dir = os.path.join(tmpdir, "dicoms")
            os.makedirs(dicom_dir, exist_ok=True)
            from qsmxt.tests.dicom_convert.fixtures.dicom_generator import create_dicom_file
            
            filename = os.path.join(dicom_dir, "t1w.dcm")
            create_dicom_file(
                filename=filename,
                patient_id="patient1",
                patient_name="Patient^One",
                study_date="20230101",
                series_description="T1_MPRAGE",
                image_type=['ORIGINAL', 'PRIMARY', 'M', 'UNI'],  # UNI indicates T1w
                instance_number=1,
                series_number=1
            )
            
            # Create DataFrame from actual DICOM file
            grp_data = create_dataframe_from_dicom_files(
                [filename], 
                acquisition="mprage", 
                series_description="T1_MPRAGE"
            )
            grp_data['Type'] = 'T1w'  # Override type for T1w
            grp_data['RunNumber'] = 1
            grp_data['NumRuns'] = 1
            
            # Mock file creation
            def create_nifti_side_effect(cmd):
                if "dcm2niix" in cmd:
                    nii_path = os.path.join(tmpdir, "temp_convert", "temp_output.nii")
                    os.makedirs(os.path.dirname(nii_path), exist_ok=True)
                    img = nb.Nifti1Image(np.zeros((256, 256, 176)), np.eye(4))
                    nb.save(img, nii_path)
                return 0
            
            mock_system.side_effect = create_nifti_side_effect
            
            process_dicom_group(grp_data, tmpdir, "dcm2niix")
            
            # Check that output directory was created (actual naming depends on the conversion)
            expected_output = os.path.join(tmpdir, "sub-patient1", "ses-20230101", "anat")
            assert os.path.exists(expected_output)
    
    @patch('qsmxt.cli.dicom_convert.make_logger')
    @patch('os.system')
    @patch('qsmxt.cli.dicom_convert.handle_4d_files')
    def test_process_dicom_group_multi_echo(self, mock_handle_4d, mock_system, mock_logger):
        """Test processing multi-echo data."""
        if not PYDICOM_AVAILABLE:
            pytest.skip("pydicom not available for creating test DICOM files")
            
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create actual multi-echo DICOM file
            dicom_dir = os.path.join(tmpdir, "dicoms")
            os.makedirs(dicom_dir, exist_ok=True)
            from qsmxt.tests.dicom_convert.fixtures.dicom_generator import create_dicom_file
            
            filename = os.path.join(dicom_dir, "echo1.dcm")
            create_dicom_file(
                filename=filename,
                patient_id="patient1",
                patient_name="Patient^One",
                study_date="20230101",
                series_description="GRE",
                image_type=['ORIGINAL', 'PRIMARY', 'M'],
                echo_time=10.0,  # Second echo
                instance_number=1,
                series_number=1
            )
            
            # Create DataFrame from actual DICOM file
            grp_data = create_dataframe_from_dicom_files(
                [filename], 
                acquisition="gre", 
                series_description="GRE"
            )
            grp_data['NumEchoes'] = 3
            grp_data['EchoNumber'] = 2
            grp_data['RunNumber'] = 1
            grp_data['NumRuns'] = 1
            
            # Mock file creation
            def create_nifti_side_effect(cmd):
                if "dcm2niix" in cmd:
                    nii_path = os.path.join(tmpdir, "temp_convert", "temp_output.nii")
                    os.makedirs(os.path.dirname(nii_path), exist_ok=True)
                    img = nb.Nifti1Image(np.zeros((64, 64, 32)), np.eye(4))
                    nb.save(img, nii_path)
                return 0
            
            mock_system.side_effect = create_nifti_side_effect
            
            process_dicom_group(grp_data, tmpdir, "dcm2niix")
            
            # Check echo number in filename
            expected_pattern = "echo-02"
            output_dir = os.path.join(tmpdir, "sub-patient1", "ses-20230101", "anat")
            files = os.listdir(output_dir) if os.path.exists(output_dir) else []
            assert any(expected_pattern in f for f in files)
    
    @patch('qsmxt.cli.dicom_convert.make_logger')
    @patch('os.system')
    @patch('qsmxt.cli.dicom_convert.handle_4d_files')
    def test_process_dicom_group_with_coil(self, mock_handle_4d, mock_system, mock_logger):
        """Test processing data with coil information."""
        if not PYDICOM_AVAILABLE:
            pytest.skip("pydicom not available for creating test DICOM files")
            
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create actual DICOM file with coil information
            dicom_dir = os.path.join(tmpdir, "dicoms")
            os.makedirs(dicom_dir, exist_ok=True)
            from qsmxt.tests.dicom_convert.fixtures.dicom_generator import create_dicom_file
            
            filename = os.path.join(dicom_dir, "coil.dcm")
            create_dicom_file(
                filename=filename,
                patient_id="patient1",
                patient_name="Patient^One",
                study_date="20230101",
                series_description="GRE",
                image_type=['ORIGINAL', 'PRIMARY', 'M'],
                instance_number=1,
                series_number=1,
                coil_string="C32"
            )
            
            # Create DataFrame from actual DICOM file
            grp_data = create_dataframe_from_dicom_files(
                [filename], 
                acquisition="gre", 
                series_description="GRE"
            )
            grp_data['RunNumber'] = 1
            grp_data['NumRuns'] = 1
            grp_data['(0051,100F)'] = 'C32'  # Add coil information
            
            # Mock file creation
            def create_nifti_side_effect(cmd):
                if "dcm2niix" in cmd:
                    nii_path = os.path.join(tmpdir, "temp_convert", "temp_output.nii")
                    os.makedirs(os.path.dirname(nii_path), exist_ok=True)
                    img = nb.Nifti1Image(np.zeros((64, 64, 32)), np.eye(4))
                    nb.save(img, nii_path)
                return 0
            
            mock_system.side_effect = create_nifti_side_effect
            
            process_dicom_group(grp_data, tmpdir, "dcm2niix")
            
            # Check coil number in filename
            expected_pattern = "coil-32"
            output_dir = os.path.join(tmpdir, "sub-patient1", "ses-20230101", "anat")
            files = os.listdir(output_dir) if os.path.exists(output_dir) else []
            assert any(expected_pattern in f for f in files)


class TestConvertAndOrganize:
    """Test cases for the convert_and_organize function."""
    
    @patch('qsmxt.cli.dicom_convert.make_logger')
    @patch('qsmxt.cli.dicom_convert.process_dicom_group')
    def test_convert_and_organize_empty(self, mock_process, mock_logger):
        """Test with empty DICOM session."""
        dicom_session = pd.DataFrame({
            'Type': ['Skip', 'Skip'],
            'PatientID': ['p1', 'p2']
        })
        
        with pytest.raises(SystemExit):
            convert_and_organize(dicom_session, "/output", "dcm2niix")
    
    @patch('qsmxt.cli.dicom_convert.make_logger')
    @patch('qsmxt.cli.dicom_convert.process_dicom_group')
    def test_convert_and_organize_basic(self, mock_process, mock_logger):
        """Test basic conversion and organization."""
        dicom_session = pd.DataFrame({
            'Type': ['Mag', 'Phase'],
            'PatientID': ['patient1', 'patient1'],
            'PatientName': ['Patient One', 'Patient One'],
            'StudyDate': ['20230101', '20230101'],
            'Acquisition': ['gre', 'gre'],
            'SeriesDescription': ['GRE_MAG', 'GRE_PHASE'],
            'RunNumber': [1, 1],
            'DICOM_Path': ['/path/1.dcm', '/path/2.dcm'],
            'SeriesInstanceUID': ['1.2.3', '1.2.4']
        })
        
        convert_and_organize(dicom_session, "/output", "dcm2niix")
        
        # Verify process_dicom_group was called
        assert mock_process.called
    
    @patch('qsmxt.cli.dicom_convert.make_logger')
    @patch('qsmxt.cli.dicom_convert.process_dicom_group')
    def test_convert_and_organize_multiple_types(self, mock_process, mock_logger):
        """Test with multiple acquisition types in same group."""
        dicom_session = pd.DataFrame({
            'Type': ['Mag', 'Phase', 'Mag', 'Phase'],
            'PatientID': ['p1', 'p1', 'p1', 'p1'],
            'PatientName': ['P1', 'P1', 'P1', 'P1'],
            'StudyDate': ['20230101', '20230101', '20230101', '20230101'],
            'Acquisition': ['gre', 'gre', 'gre', 'gre'],
            'SeriesDescription': ['GRE', 'GRE', 'GRE', 'GRE'],
            'RunNumber': [1, 1, 1, 1],
            'DICOM_Path': ['/p/1.dcm', '/p/2.dcm', '/p/3.dcm', '/p/4.dcm'],
            'SeriesInstanceUID': ['1', '2', '3', '4'],
            'EchoNumber': [1, 1, 2, 2]
        })
        
        convert_and_organize(dicom_session, "/output", "dcm2niix")
        
        # Should group by Type and potentially by EchoNumber
        assert mock_process.call_count >= 2
    
    @patch('qsmxt.cli.dicom_convert.make_logger')
    @patch('qsmxt.cli.dicom_convert.process_dicom_group')
    def test_convert_and_organize_missing_fields(self, mock_process, mock_logger):
        """Test handling of missing fields."""
        dicom_session = pd.DataFrame({
            'Type': ['Mag'],
            'Acquisition': ['gre'],
            'DICOM_Path': ['/path/1.dcm']
            # Missing PatientID, PatientName, StudyDate, etc.
        })
        
        # Add minimal required columns
        dicom_session['PatientID'] = pd.NA
        dicom_session['PatientName'] = pd.NA
        dicom_session['StudyDate'] = pd.NA
        dicom_session['SeriesDescription'] = pd.NA
        dicom_session['RunNumber'] = pd.NA
        
        convert_and_organize(dicom_session, "/output", "dcm2niix")
        
        # Should handle NA values by filling with defaults
        assert mock_process.called
        call_args = mock_process.call_args[0][0]
        assert call_args['PatientID'].iloc[0] == "NA"


@pytest.mark.parametrize("exit_code", [0, 1, 2, 255])
def test_script_exit_codes(exit_code):
    """Test script_exit with various exit codes."""
    with patch('builtins.exit') as mock_exit:
        with patch('qsmxt.cli.dicom_convert.make_logger'):
            with patch('qsmxt.cli.dicom_convert.show_warning_summary'):
                script_exit(exit_code)
                mock_exit.assert_called_once_with(exit_code)