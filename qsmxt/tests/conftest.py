"""Pytest configuration and shared fixtures for QSMxT testing."""

import pytest
import numpy as np
import tempfile
import os
from pathlib import Path
from unittest.mock import MagicMock


@pytest.fixture
def temp_dir():
    """Temporary directory for test files."""
    with tempfile.TemporaryDirectory() as tmp:
        yield Path(tmp)


@pytest.fixture
def synthetic_3d_array():
    """3D array simulating brain image data."""
    return np.random.rand(64, 64, 32).astype(np.float32)


@pytest.fixture
def synthetic_phase_data():
    """Phase data with realistic MRI characteristics."""
    shape = (64, 64, 32)
    # Create phase data with wrapped regions
    x, y, z = np.meshgrid(np.linspace(-np.pi, np.pi, shape[0]),
                          np.linspace(-np.pi, np.pi, shape[1]),
                          np.linspace(-np.pi, np.pi, shape[2]))
    return (x + 0.5*y + 0.25*z) % (2*np.pi) - np.pi


@pytest.fixture
def synthetic_brain_data():
    """3D brain-like image data."""
    # Create realistic brain-shaped data
    shape = (64, 64, 32)
    x, y, z = np.meshgrid(np.linspace(-1, 1, shape[0]),
                          np.linspace(-1, 1, shape[1]),
                          np.linspace(-1, 1, shape[2]))
    
    # Brain-like ellipsoid
    brain_mask = (x**2/0.8**2 + y**2/0.8**2 + z**2/0.6**2) < 1
    
    # Add realistic intensity variations
    data = np.zeros(shape)
    data[brain_mask] = np.random.normal(100, 15, np.sum(brain_mask))
    data[~brain_mask] = np.random.normal(20, 5, np.sum(~brain_mask))
    
    return data.astype(np.float32)


@pytest.fixture
def mock_nifti_file():
    """Mock nibabel NIfTI file."""
    mock_nii = MagicMock()
    mock_nii.header = MagicMock()
    mock_nii.affine = np.eye(4)
    return mock_nii


@pytest.fixture(autouse=True)
def setup_test_environment():
    """Setup test environment variables."""
    os.environ['QSMXT_TEST_MODE'] = '1'
    yield
    os.environ.pop('QSMXT_TEST_MODE', None)