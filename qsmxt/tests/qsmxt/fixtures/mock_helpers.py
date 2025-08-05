"""Common mocking utilities."""

from unittest.mock import MagicMock, patch
import numpy as np
import contextlib


@contextlib.contextmanager
def mock_nibabel_io(data_array, header=None, affine=None):
    """Context manager for mocking nibabel file I/O."""
    if affine is None:
        affine = np.eye(4)
    
    mock_nii = MagicMock()
    mock_nii.get_fdata.return_value = data_array
    mock_nii.header = header or MagicMock()
    mock_nii.affine = affine
    
    # Create a mock for the created image that stores the data
    mock_created_img = MagicMock()
    
    def mock_nifti1_image(dataobj=None, header=None, affine=None):
        mock_created_img.dataobj = dataobj
        mock_created_img.header = header
        mock_created_img.affine = affine
        return mock_created_img
    
    with patch('nibabel.load', return_value=mock_nii) as mock_load, \
         patch('nibabel.save') as mock_save, \
         patch('nibabel.Nifti1Image', side_effect=mock_nifti1_image) as mock_img:
        
        yield mock_load, mock_save, mock_img


def create_mock_interface_inputs(**kwargs):
    """Create mock Nipype interface inputs."""
    mock_inputs = MagicMock()
    for key, value in kwargs.items():
        setattr(mock_inputs, key, value)
    return mock_inputs


def create_mock_nifti_header(voxel_sizes=(1.0, 1.0, 1.0), 
                           shape=(64, 64, 32),
                           datatype=np.float32):
    """Create a mock NIfTI header with realistic properties."""
    mock_header = MagicMock()
    mock_header.get_zooms.return_value = voxel_sizes
    mock_header.get_data_shape.return_value = shape
    mock_header.get_data_dtype.return_value = datatype
    return mock_header


@contextlib.contextmanager
def mock_file_operations():
    """Mock common file operations for testing."""
    with patch('os.path.exists', return_value=True) as mock_exists, \
         patch('os.makedirs') as mock_makedirs, \
         patch('shutil.copy2') as mock_copy:
        yield mock_exists, mock_makedirs, mock_copy


def assert_array_properties(array, expected_shape=None, expected_dtype=None, 
                          finite_only=True, non_negative=False):
    """Assert common array properties for neuroimaging data."""
    if expected_shape is not None:
        assert array.shape == expected_shape, f"Expected shape {expected_shape}, got {array.shape}"
    
    if expected_dtype is not None:
        assert array.dtype == expected_dtype, f"Expected dtype {expected_dtype}, got {array.dtype}"
    
    if finite_only:
        assert np.all(np.isfinite(array[~np.isnan(array)])), "Array contains infinite values"
    
    if non_negative:
        assert np.all(array[~np.isnan(array)] >= 0), "Array contains negative values"


def create_mock_subprocess_result(returncode=0, stdout="", stderr=""):
    """Create a mock subprocess result."""
    mock_result = MagicMock()
    mock_result.returncode = returncode
    mock_result.stdout = stdout
    mock_result.stderr = stderr
    return mock_result