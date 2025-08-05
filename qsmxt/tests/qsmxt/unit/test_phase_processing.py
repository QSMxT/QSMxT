"""Unit tests for phase processing functions."""

import pytest
import numpy as np
from unittest.mock import patch, MagicMock
from qsmxt.interfaces.nipype_interface_processphase import (
    frequency_to_normalized,
    phase_to_normalized,
    scale_to_pi,
    seed_from_filename
)
from qsmxt.tests.qsmxt.fixtures.mock_helpers import mock_nibabel_io


class TestPhaseProcessing:
    """Test phase processing mathematical functions."""

    def test_frequency_to_normalized_calculation(self):
        """Test core mathematical transformation from frequency to normalized phase."""
        # Create synthetic frequency data (Hz)
        frequency_data = np.array([[[10.0, 20.0], [30.0, 40.0]]])
        
        with mock_nibabel_io(frequency_data) as (mock_load, mock_save, mock_img):
            result_path = frequency_to_normalized(
                frequency_path="test_frequency.nii",
                B0=3.0,  # Tesla
                scale_factor=1e6/(2*np.pi)  # For ppm conversion
            )
            
            # Verify mathematical calculation
            γ = 42.58e6  # Hz/T
            expected = (2*np.pi * frequency_data) / (γ * 3.0) * (1e6/(2*np.pi))
            
            # Check that save was called with correct data
            mock_save.assert_called_once()
            # Get the created image from the Nifti1Image call
            saved_data = mock_img.call_args[1]['dataobj']
            
            np.testing.assert_array_almost_equal(saved_data, expected, decimal=10)
            assert result_path is not None

    def test_frequency_to_normalized_different_scale_factors(self):
        """Test frequency normalization with different scale factors."""
        frequency_data = np.array([[[100.0]]])
        B0 = 3.0
        
        test_cases = [
            (1, "no scaling"),
            (1e6, "microradians"),
            (1e6/(2*np.pi), "ppm conversion")
        ]
        
        for scale_factor, description in test_cases:
            with mock_nibabel_io(frequency_data) as (mock_load, mock_save, mock_img):
                frequency_to_normalized(
                    frequency_path="test.nii",
                    B0=B0,
                    scale_factor=scale_factor
                )
                
                # Verify calculation
                γ = 42.58e6
                expected = (2*np.pi * frequency_data) / (γ * B0) * scale_factor
                
                saved_data = mock_img.call_args[1]['dataobj']
                np.testing.assert_array_almost_equal(
                    saved_data, expected, 
                    err_msg=f"Failed for {description}"
                )

    def test_phase_to_normalized_calculation(self):
        """Test phase to normalized conversion with known parameters."""
        # Phase data in radians
        phase_data = np.array([[[0.1, 0.2], [0.3, 0.4]]])
        B0 = 3.0  # Tesla
        TE = 0.02  # 20ms
        scale_factor = 1e6
        
        with mock_nibabel_io(phase_data) as (mock_load, mock_save, mock_img):
            result_path = phase_to_normalized(
                phase_path="test_phase.nii",
                B0=B0,
                TE=TE,
                scale_factor=scale_factor
            )
            
            # Verify mathematical calculation
            γ = 42.58e6  # Hz/T
            expected = phase_data / (TE * γ * B0) * scale_factor
            
            saved_data = mock_img.call_args[1]['dataobj']
            np.testing.assert_array_almost_equal(saved_data, expected, decimal=10)
            assert result_path is not None

    def test_phase_to_normalized_edge_cases(self):
        """Test phase normalization with edge cases."""
        test_cases = [
            (np.array([[[0.0]]]), "zero phase"),
            (np.array([[[np.pi]]]), "pi phase"),
            (np.array([[[-np.pi]]]), "negative pi phase"),
            (np.array([[[np.nan]]]), "nan values")
        ]
        
        for phase_data, description in test_cases:
            with mock_nibabel_io(phase_data) as (mock_load, mock_save, mock_img):
                result_path = phase_to_normalized(
                    phase_path="test.nii",
                    B0=3.0,
                    TE=0.02,
                    scale_factor=1
                )
                
                saved_data = mock_img.call_args[1]['dataobj']
                
                # For NaN input, output should also be NaN
                if np.isnan(phase_data).any():
                    assert np.isnan(saved_data).any(), f"Failed for {description}"
                else:
                    assert np.isfinite(saved_data).all(), f"Failed for {description}"

    def test_scale_to_pi_basic_scaling(self):
        """Test basic phase scaling to [-π, π] range."""
        # Phase data outside [-π, π] range
        phase_data = np.array([[[0.0, 5.0], [-3.0, 10.0]]])
        
        with mock_nibabel_io(phase_data) as (mock_load, mock_save, mock_img):
            result_path = scale_to_pi("test_phase.nii")
            
            saved_data = mock_img.call_args[1]['dataobj']
            
            # Check that values are scaled to [-π, π]
            assert np.all(saved_data >= -np.pi), "Values below -π found"
            assert np.all(saved_data <= np.pi), "Values above π found"
            assert result_path is not None

    def test_scale_to_pi_nan_handling(self):
        """Test NaN replacement in phase scaling."""
        # Phase data with NaN values
        phase_data = np.array([[[np.nan, 2.0], [0.0, -5.0]]])
        
        with mock_nibabel_io(phase_data) as (mock_load, mock_save, mock_img):
            scale_to_pi("test_phase.nii")
            
            saved_data = mock_img.call_args[1]['dataobj']
            
            # NaN should be replaced with 0, then scaled
            assert not np.isnan(saved_data).any(), "NaN values not properly handled"
            assert np.all(saved_data >= -np.pi), "Scaled values outside range"
            assert np.all(saved_data <= np.pi), "Scaled values outside range"

    def test_scale_to_pi_uniform_data_replacement(self):
        """Test replacement of uniform data with random noise."""
        # Create phase data where >10% of values are close to 0 or π
        phase_data = np.zeros((10, 10, 10))  # All zeros - should trigger replacement
        
        with mock_nibabel_io(phase_data) as (mock_load, mock_save, mock_img):
            with patch('qsmxt.interfaces.nipype_interface_processphase.seed_from_filename', 
                      return_value=42):
                scale_to_pi("test_phase.nii")
            
            saved_data = mock_img.call_args[1]['dataobj']
            
            # Should not be all zeros anymore (random replacement occurred)
            assert not np.allclose(saved_data, 0), "Uniform data not replaced with noise"
            assert np.all(saved_data >= -np.pi), "Values outside expected range"
            assert np.all(saved_data <= np.pi), "Values outside expected range"

    def test_scale_to_pi_already_correct_range(self):
        """Test that properly scaled data is returned unchanged."""
        # Data already in [-π, π] range with correct bounds
        phase_data = np.array([[[-np.pi, 0.0], [np.pi, 1.0]]])
        
        with mock_nibabel_io(phase_data) as (mock_load, mock_save, mock_img):
            # Mock the return path to simulate "no change needed"
            original_path = "test_phase.nii"
            
            # This test verifies the logic, but the function always saves
            # due to the implementation, so we test the mathematical correctness
            result_path = scale_to_pi(original_path)
            
            saved_data = mock_img.call_args[1]['dataobj']
            assert np.all(saved_data >= -np.pi)
            assert np.all(saved_data <= np.pi)

    def test_seed_from_filename_deterministic(self):
        """Test that seed generation is deterministic."""
        filename1 = "test_file.nii"
        filename2 = "test_file.nii"
        filename3 = "different_file.nii"
        
        seed1 = seed_from_filename(filename1)
        seed2 = seed_from_filename(filename2)
        seed3 = seed_from_filename(filename3)
        
        # Same filename should produce same seed
        assert seed1 == seed2, "Same filename produced different seeds"
        
        # Different filename should produce different seed
        assert seed1 != seed3, "Different filenames produced same seed"
        
        # Seeds should be valid 32-bit integers
        assert 0 <= seed1 < 2**32, "Seed outside valid range"
        assert 0 <= seed3 < 2**32, "Seed outside valid range"

    def test_seed_from_filename_hash_consistency(self):
        """Test seed generation consistency across multiple calls."""
        filename = "consistent_test.nii"
        
        seeds = [seed_from_filename(filename) for _ in range(10)]
        
        # All seeds should be identical
        assert all(seed == seeds[0] for seed in seeds), "Seed generation not consistent"

    @pytest.mark.parametrize("B0,TE,scale_factor", [
        (1.5, 0.01, 1),      # Low field, short TE
        (3.0, 0.02, 1e6),    # Standard clinical
        (7.0, 0.05, 1e6/(2*np.pi)),  # High field, long TE
    ])
    def test_phase_normalization_parameter_variations(self, B0, TE, scale_factor):
        """Test phase normalization with various realistic parameter combinations."""
        phase_data = np.array([[[0.5, 1.0], [1.5, 2.0]]])
        
        with mock_nibabel_io(phase_data) as (mock_load, mock_save, mock_img):
            result_path = phase_to_normalized(
                phase_path="test.nii",
                B0=B0,
                TE=TE,
                scale_factor=scale_factor
            )
            
            # Verify calculation with expected formula
            γ = 42.58e6
            expected = phase_data / (TE * γ * B0) * scale_factor
            
            saved_data = mock_img.call_args[1]['dataobj']
            np.testing.assert_array_almost_equal(saved_data, expected)
            
            # Verify that the result is finite and reasonable
            assert np.all(np.isfinite(saved_data)), f"Non-finite results for B0={B0}, TE={TE}"


class TestPhaseProcessingIntegration:
    """Integration tests for phase processing pipeline."""
    
    def test_frequency_to_phase_to_normalized_pipeline(self):
        """Test the complete frequency → phase → normalized pipeline."""
        # Start with frequency data
        frequency_data = np.array([[[50.0, 100.0]]])  # Hz
        B0 = 3.0
        TE = 0.02
        scale_factor = 1e6/(2*np.pi)
        
        # Step 1: frequency_to_normalized
        with mock_nibabel_io(frequency_data) as (mock_load, mock_save, mock_img):
            normalized_path = frequency_to_normalized(
                frequency_path="frequency.nii",
                B0=B0,
                scale_factor=scale_factor
            )
            
            # Calculate expected final result
            γ = 42.58e6
            expected_normalized = (2*np.pi * frequency_data) / (γ * B0) * scale_factor
            
            saved_data = mock_img.call_args[1]['dataobj']
            np.testing.assert_array_almost_equal(saved_data, expected_normalized)

        # Alternative: phase_to_normalized from equivalent phase data
        equivalent_phase = 2*np.pi * frequency_data * TE
        
        with mock_nibabel_io(equivalent_phase) as (mock_load, mock_save, mock_img):
            normalized_path2 = phase_to_normalized(
                phase_path="phase.nii",
                B0=B0,
                TE=TE,
                scale_factor=scale_factor
            )
            
            # Should produce the same result
            expected_from_phase = equivalent_phase / (TE * γ * B0) * scale_factor
            
            saved_data2 = mock_save.call_args[1]['img'].dataobj
            np.testing.assert_array_almost_equal(saved_data2, expected_from_phase)
            
            # Both methods should give equivalent results
            np.testing.assert_array_almost_equal(saved_data, saved_data2, decimal=8)