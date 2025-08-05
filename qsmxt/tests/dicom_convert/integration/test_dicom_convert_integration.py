#!/usr/bin/env python3
"""
Integration tests for the dicom_convert module.
"""

import pytest
import os
import tempfile
import shutil
import json
import pandas as pd
import nibabel as nb
import numpy as np
from unittest.mock import patch, MagicMock
from qsmxt.cli.dicom_convert import (
    convert_to_bids, fix_ge_data, merge_multicoil_data, 
    run_mcpc3ds_on_multicoil, main
)
from qsmxt.tests.dicom_convert.fixtures.mock_dicom_data import (
    create_complete_dicom_dataframe, create_mag_phase_pair, create_minimal_valid_dataframe
)
try:
    from qsmxt.tests.dicom_convert.fixtures.dicom_generator import (
        create_test_dicom_directory, create_magnitude_phase_pair, create_dicom_file
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
            # Add fields for compatibility
            'Type': 'Mag',
            'Description': '',
            'NumRuns': 1,
            'NumEchoes': 1,
            'EchoNumber': 1,
            'RunNumber': 1,
            'AcquisitionDate': ds.StudyDate,  # Use StudyDate as fallback
            'SeriesTime': ds.get('SeriesTime', '120000'),
        }
        # Add coil information if present
        if hasattr(ds, 'get') and ds.get((0x0051, 0x100f)):
            row['(0051,100F)'] = ds.get((0x0051, 0x100f)).value
        
        rows.append(row)
    
    return pd.DataFrame(rows)


class TestConvertToBIDS:
    """Integration tests for convert_to_bids function."""
    
    @patch('qsmxt.cli.dicom_convert.assign_acquisition_and_run_numbers')
    @patch('qsmxt.cli.dicom_convert.convert_and_organize')
    @patch('qsmxt.cli.dicom_convert.interactive_acquisition_selection_series')
    @patch('qsmxt.cli.dicom_convert.make_logger')
    def test_convert_to_bids_interactive(self, mock_logger, mock_interactive,
                                        mock_convert, mock_assign):
        """Test interactive BIDS conversion."""
        if not PYDICOM_AVAILABLE:
            pytest.skip("pydicom not available for creating test DICOM files")
            
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create actual DICOM files
            input_dir = os.path.join(tmpdir, "input")
            output_dir = os.path.join(tmpdir, "output")
            os.makedirs(input_dir)
            os.makedirs(output_dir)
            
            # Create magnitude/phase DICOM files
            dicom_files = []
            for i in range(5):  # 5 slices
                filename = os.path.join(input_dir, f"dicom_{i:03d}.dcm")
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
            
            # Create expected DataFrame for assign function
            mock_dicom_data = create_dataframe_from_dicom_files(
                dicom_files, 
                acquisition="gre", 
                series_description="GRE"
            )
        
            mock_assign.return_value = mock_dicom_data
            
            # Mock user selections
            mock_selections = [
                {'Acquisition': 'gre', 'SeriesDescription': 'GRE', 
                 'ImageType': ['M'], 'Type': 'Mag', 'Description': ''}
            ]
            mock_interactive.return_value = mock_selections
            
            # Test should complete without errors
            convert_to_bids(input_dir, output_dir, auto_yes=False)
            
            # Verify key functions were called (some may be skipped due to mocking)
            mock_assign.assert_called_once()
    
    @patch('qsmxt.cli.dicom_convert.load_dicom_session')
    @patch('qsmxt.cli.dicom_convert.assign_acquisition_and_run_numbers')
    @patch('qsmxt.cli.dicom_convert.convert_and_organize')
    @patch('qsmxt.cli.dicom_convert.auto_assign_initial_labels')
    @patch('qsmxt.cli.dicom_convert.make_logger')
    def test_convert_to_bids_auto(self, mock_logger, mock_auto_assign,
                                  mock_convert, mock_assign, mock_load):
        """Test automatic BIDS conversion."""
        # Setup complete mock data
        mock_dicom_data = create_complete_dicom_dataframe(
            patient_id="patient1",
            patient_name="Patient One",
            study_date="20230101",
            acquisition="gre", 
            series_description="GRE",
            image_type=['M'],
            num_instances=10,
            series_time="120000"
        )
        
        mock_load.return_value = mock_dicom_data
        mock_assign.return_value = mock_dicom_data
        
        # Mock auto_assign_initial_labels to add required columns
        def add_type_description(table_data):
            for row in table_data:
                row['Type'] = 'Mag'  # Default assignment
                row['Description'] = ''
        
        mock_auto_assign.side_effect = add_type_description
        
        with tempfile.TemporaryDirectory() as tmpdir:
            convert_to_bids(tmpdir, tmpdir, auto_yes=True)
            
            # Verify auto assignment was used
            mock_auto_assign.assert_called()
            mock_convert.assert_called_once()


class TestGEDataFixes:
    """Integration tests for GE data fixing functions."""
    
    @patch('qsmxt.cli.dicom_convert.load_nifti_session')
    @patch('qsmxt.cli.dicom_convert.fix_ge_complex')
    @patch('qsmxt.cli.dicom_convert.fix_ge_polar')
    @patch('qsmxt.cli.dicom_convert.make_logger')
    def test_fix_ge_data_complex(self, mock_logger, mock_fix_polar, 
                                 mock_fix_complex, mock_load_nifti):
        """Test fixing GE complex data."""
        # Setup mock NIfTI session with complex data
        mock_session = pd.DataFrame({
            'sub': ['01', '01'],
            'ses': ['01', '01'],
            'acq': ['gre', 'gre'],
            'part': ['real', 'imag'],
            'NIfTI_Path': ['/path/real.nii', '/path/imag.nii'],
            'AcquisitionPlane': ['axial', 'axial']
        })
        
        mock_load_nifti.return_value = mock_session
        
        with tempfile.TemporaryDirectory() as tmpdir:
            fix_ge_data(tmpdir)
            
            # Verify complex fix was called
            mock_fix_complex.assert_called_once_with(
                real_nii_path='/path/real.nii',
                imag_nii_path='/path/imag.nii',
                delete_originals=True,
                acquisition_plane='axial'
            )
    
    @patch('qsmxt.cli.dicom_convert.load_nifti_session')
    @patch('qsmxt.cli.dicom_convert.fix_ge_complex')
    @patch('qsmxt.cli.dicom_convert.fix_ge_polar')
    @patch('qsmxt.cli.dicom_convert.make_logger')
    def test_fix_ge_data_polar(self, mock_logger, mock_fix_polar, 
                               mock_fix_complex, mock_load_nifti):
        """Test fixing GE polar data."""
        # Setup mock NIfTI session with polar data
        mock_session = pd.DataFrame({
            'sub': ['01', '01'],
            'ses': ['01', '01'],
            'acq': ['gre', 'gre'],
            'part': ['mag', 'phase'],
            'NIfTI_Path': ['/path/mag.nii', '/path/phase.nii'],
            'AcquisitionPlane': ['sagittal', 'sagittal'],
            'Manufacturer': ['GE', 'GE']
        })
        
        mock_load_nifti.return_value = mock_session
        
        with tempfile.TemporaryDirectory() as tmpdir:
            fix_ge_data(tmpdir)
            
            # Verify polar fix was called
            mock_fix_polar.assert_called_once_with(
                mag_nii_path='/path/mag.nii',
                phase_nii_path='/path/phase.nii',
                delete_originals=True,
                acquisition_plane='sagittal'
            )


class TestMulticoilProcessing:
    """Integration tests for multi-coil processing functions."""
    
    @patch('qsmxt.cli.dicom_convert.load_nifti_session')
    @patch('qsmxt.cli.dicom_convert.make_logger')
    def test_merge_multicoil_data(self, mock_logger, mock_load_nifti):
        """Test merging multi-coil data."""
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create test multi-coil data
            sub_dir = os.path.join(tmpdir, "sub-01", "ses-01", "anat")
            os.makedirs(sub_dir, exist_ok=True)
            
            # Create coil files
            for coil in range(1, 4):
                for part in ['mag', 'phase']:
                    filename = f"sub-01_ses-01_acq-gre_coil-{coil:02d}_part-{part}_MEGRE.nii"
                    filepath = os.path.join(sub_dir, filename)
                    # Create dummy 3D NIfTI
                    img = nb.Nifti1Image(np.ones((64, 64, 32)) * coil, np.eye(4))
                    nb.save(img, filepath)
                    # Create JSON
                    with open(filepath.replace('.nii', '.json'), 'w') as f:
                        json.dump({'EchoTime': 0.005}, f)
            
            # Setup mock session
            nifti_paths = []
            coils = []
            parts = []
            for c in [1, 2, 3]:
                for p in ['mag', 'phase']:
                    nifti_paths.append(os.path.join(sub_dir, f"sub-01_ses-01_acq-gre_coil-{c:02d}_part-{p}_MEGRE.nii"))
                    coils.append(f'{c:02d}')
                    parts.append(p)
            
            mock_session = pd.DataFrame({
                'sub': ['01'] * 6,
                'ses': ['01'] * 6,
                'acq': ['gre'] * 6,
                'run': [None] * 6,
                'coil': coils,
                'part': parts,
                'NIfTI_Path': nifti_paths
            })
            
            mock_load_nifti.return_value = mock_session
            
            merge_multicoil_data(tmpdir)
            
            # Check merged files were created
            merged_mag = os.path.join(sub_dir, "sub-01_ses-01_acq-gre_part-mag_MEGRE.nii")
            merged_phase = os.path.join(sub_dir, "sub-01_ses-01_acq-gre_part-phase_MEGRE.nii")
            
            assert os.path.exists(merged_mag)
            assert os.path.exists(merged_phase)
            
            # Check merged data has 4D shape
            merged_img = nb.load(merged_mag)
            assert merged_img.ndim == 4
            assert merged_img.shape[3] == 3  # 3 coils
    
    @patch('qsmxt.cli.dicom_convert.load_nifti_session')
    @patch('os.system')
    @patch('qsmxt.cli.dicom_convert.get_qsmxt_dir')
    @patch('qsmxt.cli.dicom_convert.make_logger')
    def test_run_mcpc3ds_on_multicoil(self, mock_logger, mock_qsmxt_dir,
                                      mock_system, mock_load_nifti):
        """Test running MCPC-3D-S on multi-coil data."""
        with tempfile.TemporaryDirectory() as tmpdir:
            # Setup paths
            sub_dir = os.path.join(tmpdir, "sub-01", "ses-01", "anat")
            os.makedirs(sub_dir, exist_ok=True)
            
            # Create 4D multi-coil files
            mag_path = os.path.join(sub_dir, "sub-01_ses-01_acq-gre_part-mag_MEGRE.nii")
            phase_path = os.path.join(sub_dir, "sub-01_ses-01_acq-gre_part-phase_MEGRE.nii")
            
            # Create 4D NIfTI (3 echoes)
            for path in [mag_path, phase_path]:
                img = nb.Nifti1Image(np.zeros((64, 64, 32, 3)), np.eye(4))
                nb.save(img, path)
                # Create JSON with echo times
                with open(path.replace('.nii', '.json'), 'w') as f:
                    json.dump({'EchoTime': 0.005}, f)
            
            # Setup mock session
            mock_session = pd.DataFrame({
                'sub': ['01', '01'],
                'ses': ['01', '01'],
                'acq': ['gre', 'gre'],
                'echo': ['01', '01'],
                'part': ['mag', 'phase'],
                'NIfTI_Path': [mag_path, phase_path],
                'NIfTI_Shape': [(64, 64, 32, 3), (64, 64, 32, 3)],
                'EchoTime': [0.005, 0.005]
            })
            
            mock_load_nifti.return_value = mock_session
            mock_qsmxt_dir.return_value = "/mock/qsmxt"
            
            # Mock successful mcpc3ds execution
            def mock_mcpc3ds_execution(cmd):
                if "mcpc3ds.jl" in cmd:
                    # Create output files
                    base = os.path.splitext(mag_path)[0]
                    nb.save(nb.Nifti1Image(np.zeros((64, 64, 32)), np.eye(4)), 
                           f"{base}_mag.nii")
                    nb.save(nb.Nifti1Image(np.zeros((64, 64, 32)), np.eye(4)), 
                           f"{base}_phase.nii")
                return 0
            
            mock_system.side_effect = mock_mcpc3ds_execution
            
            run_mcpc3ds_on_multicoil(tmpdir)
            
            # Verify mcpc3ds was called
            assert mock_system.called
            call_cmd = mock_system.call_args[0][0]
            assert "mcpc3ds.jl" in call_cmd
            assert "--mag" in call_cmd
            assert "--phase" in call_cmd
            assert "--TEs" in call_cmd


class TestMainFunction:
    """Integration tests for the main function."""
    
    @patch('sys.argv', ['dicom-convert', '/input', '/output'])
    @patch('qsmxt.cli.dicom_convert.convert_to_bids')
    @patch('qsmxt.cli.dicom_convert.merge_multicoil_data')
    @patch('qsmxt.cli.dicom_convert.run_mcpc3ds_on_multicoil')
    @patch('qsmxt.cli.dicom_convert.fix_ge_data')
    @patch('qsmxt.cli.dicom_convert.script_exit')
    @patch('shutil.which')
    @patch('qsmxt.cli.dicom_convert.make_logger')
    def test_main_function(self, mock_logger, mock_which, mock_exit,
                          mock_fix_ge, mock_mcpc3ds, mock_merge, mock_convert):
        """Test main function execution."""
        # Mock dcm2niix availability
        mock_which.return_value = "/usr/bin/dcm2niix"
        
        with tempfile.TemporaryDirectory() as tmpdir:
            with patch('sys.argv', ['dicom-convert', tmpdir, tmpdir]):
                main()
                
                # Verify pipeline execution order
                mock_convert.assert_called_once()
                mock_merge.assert_called_once()
                mock_mcpc3ds.assert_called_once()
                mock_fix_ge.assert_called_once()
                mock_exit.assert_called_once_with()
    
    @patch('sys.argv', ['dicom-convert', '/input', '/output', '--auto_yes'])
    @patch('qsmxt.cli.dicom_convert.convert_to_bids')
    @patch('qsmxt.cli.dicom_convert.merge_multicoil_data')
    @patch('qsmxt.cli.dicom_convert.run_mcpc3ds_on_multicoil')
    @patch('qsmxt.cli.dicom_convert.fix_ge_data')
    @patch('shutil.which')
    @patch('qsmxt.cli.dicom_convert.make_logger')
    def test_main_auto_yes(self, mock_logger, mock_which, mock_fix_ge, 
                          mock_mcpc3ds, mock_merge, mock_convert):
        """Test main function with --auto_yes flag."""
        mock_which.return_value = "/usr/bin/dcm2niix"
        
        with tempfile.TemporaryDirectory() as tmpdir:
            with patch('sys.argv', ['dicom-convert', tmpdir, tmpdir, '--auto_yes']):
                with patch('qsmxt.cli.dicom_convert.script_exit'):
                    main()
                    
                    # Verify auto_yes was passed
                    args = mock_convert.call_args[1]
                    assert args['auto_yes'] is True
    
    @patch('shutil.which')
    @patch('qsmxt.cli.dicom_convert.script_exit')
    @patch('qsmxt.cli.dicom_convert.make_logger')
    def test_main_no_dcm2niix(self, mock_logger, mock_exit, mock_which):
        """Test main function when dcm2niix is not available."""
        if not PYDICOM_AVAILABLE:
            pytest.skip("pydicom not available for creating test DICOM files")
            
        mock_which.return_value = None
        
        # Make script_exit actually exit to prevent further execution
        def side_effect(code=0):
            raise SystemExit(code)
        mock_exit.side_effect = side_effect
        
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create actual DICOM files
            input_dir = os.path.join(tmpdir, "input")
            output_dir = os.path.join(tmpdir, "output")
            os.makedirs(input_dir)
            os.makedirs(output_dir)
            
            # Create test DICOM data
            create_magnitude_phase_pair(
                output_dir=input_dir,
                num_slices=5,
                patient_id="TEST001"
            )
            
            with patch('sys.argv', ['dicom-convert', input_dir, output_dir]):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                
                # Should exit with error since dcm2niix is not available
                assert exc_info.value.code == 1
                mock_exit.assert_called_once_with(1)