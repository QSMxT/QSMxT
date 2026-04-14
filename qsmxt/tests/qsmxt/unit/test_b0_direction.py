"""Unit tests for B0 direction computation and axial resampling interaction."""

import pytest
import numpy as np
import nibabel as nib
import tempfile
import os
from qsmxt.interfaces.nipype_interface_axialsampling import (
    compute_b0_direction,
    resample_to_axial,
)


def make_affine(voxel_sizes, rotation_matrix=None):
    """Helper to build a 4x4 affine from voxel sizes and optional rotation."""
    if rotation_matrix is None:
        rotation_matrix = np.eye(3)
    affine = np.eye(4)
    affine[:3, :3] = rotation_matrix @ np.diag(voxel_sizes)
    return affine


def rotation_about_x(angle_deg):
    """Rotation matrix about the x-axis."""
    a = np.radians(angle_deg)
    return np.array([
        [1, 0, 0],
        [0, np.cos(a), -np.sin(a)],
        [0, np.sin(a), np.cos(a)],
    ])


class TestComputeB0Direction:
    """Tests for compute_b0_direction()."""

    def test_identity_affine(self):
        """Identity affine should give B0 = [0, 0, 1]."""
        affine = np.eye(4)
        b0 = compute_b0_direction(affine)
        np.testing.assert_allclose(b0, [0, 0, 1], atol=1e-10)

    def test_isotropic_voxels_no_rotation(self):
        """Isotropic voxels with no rotation should give [0, 0, 1]."""
        affine = make_affine([2.0, 2.0, 2.0])
        b0 = compute_b0_direction(affine)
        np.testing.assert_allclose(b0, [0, 0, 1], atol=1e-10)

    def test_anisotropic_voxels_no_rotation(self):
        """Anisotropic voxels (e.g. 0.8x0.8x3mm) with no rotation should still give [0, 0, 1].

        This is the bug that was present in v8.2.2 -- the old code would give
        a wrong direction because it didn't factor out voxel sizes.
        """
        affine = make_affine([0.8, 0.8, 3.0])
        b0 = compute_b0_direction(affine)
        np.testing.assert_allclose(b0, [0, 0, 1], atol=1e-10)

    def test_anisotropic_voxels_extreme(self):
        """Very anisotropic voxels (0.5x0.5x10mm) should still give [0, 0, 1]."""
        affine = make_affine([0.5, 0.5, 10.0])
        b0 = compute_b0_direction(affine)
        np.testing.assert_allclose(b0, [0, 0, 1], atol=1e-10)

    def test_oblique_acquisition_20_degrees(self):
        """20-degree tilt about x-axis (like UKB Tra>Cor(-20))."""
        R = rotation_about_x(20)
        affine = make_affine([0.8, 0.8, 3.0], R)
        b0 = compute_b0_direction(affine)

        # B0 in world is [0,0,1]. In voxel space: R.T @ [0,0,1]
        expected = R.T @ np.array([0, 0, 1])
        expected = expected / np.linalg.norm(expected)
        np.testing.assert_allclose(b0, expected, atol=1e-10)

        # The angle from [0,0,1] should be ~20 degrees
        angle = np.degrees(np.arccos(np.clip(np.dot(b0, [0, 0, 1]), -1, 1)))
        np.testing.assert_allclose(angle, 20.0, atol=0.1)

    def test_oblique_acquisition_45_degrees(self):
        """45-degree tilt should give ~45 degree angle."""
        R = rotation_about_x(45)
        affine = make_affine([1.0, 1.0, 1.0], R)
        b0 = compute_b0_direction(affine)
        angle = np.degrees(np.arccos(np.clip(np.dot(b0, [0, 0, 1]), -1, 1)))
        np.testing.assert_allclose(angle, 45.0, atol=0.1)

    def test_direction_is_normalized(self):
        """Output should be a unit vector."""
        R = rotation_about_x(30)
        affine = make_affine([0.8, 0.8, 3.0], R)
        b0 = compute_b0_direction(affine)
        np.testing.assert_allclose(np.linalg.norm(b0), 1.0, atol=1e-10)

    def test_returns_list(self):
        """Output should be a list, not numpy array (for nipype serialization)."""
        affine = np.eye(4)
        b0 = compute_b0_direction(affine)
        assert isinstance(b0, list)
        assert len(b0) == 3

    def test_negative_voxel_sizes(self):
        """Negative diagonal elements (LAS/RAS conventions) should still work."""
        affine = make_affine([-0.8, -0.8, 3.0])
        b0 = compute_b0_direction(affine)
        # Should still point along z
        np.testing.assert_allclose(np.abs(b0[2]), 1.0, atol=1e-10)

    def test_real_ukb_affine(self):
        """Test with the actual UKB SWI affine that triggered the original bug.

        This affine has 0.8x0.8x3mm voxels with Tra>Cor(-20.1) tilt.
        The old buggy code produced [-0.155, 0.797, 0.584] (54 degrees off).
        """
        affine = np.array([
            [7.96613216e-01, 3.64526697e-02, 1.61935240e-01, -1.16186287e+02],
            [-1.94950122e-02, 7.49923170e-01, -1.02886796e+00, -8.90087204e+01],
            [-5.29813245e-02, 2.72150964e-01, 2.81339788e+00, -6.93026276e+01],
            [0.00000000e+00, 0.00000000e+00, 0.00000000e+00, 1.00000000e+00],
        ])
        b0 = compute_b0_direction(affine)

        # Should be close to [0,0,1] with ~20 degree tilt
        angle = np.degrees(np.arccos(np.clip(np.dot(b0, [0, 0, 1]), -1, 1)))
        np.testing.assert_allclose(angle, 20.3, atol=0.5)

        # Must NOT be the old buggy result
        buggy_result = [-0.155, 0.797, 0.584]
        buggy_angle = np.degrees(np.arccos(np.clip(np.dot(buggy_result, [0, 0, 1]), -1, 1)))
        assert buggy_angle > 50, "Sanity check: buggy result was ~54 degrees off"
        assert angle < 25, f"B0 direction should be within 25 degrees of [0,0,1], got {angle}"


class TestAxialResamplingB0Interaction:
    """Tests that axial resampling produces an affine where B0 = [0, 0, 1]."""

    def _make_nifti(self, affine, shape=(32, 32, 16)):
        """Create a minimal NIfTI image with the given affine."""
        data = np.random.rand(*shape).astype(np.float32)
        return nib.Nifti1Image(data, affine=affine)

    def test_resampled_affine_is_diagonal(self):
        """After axial resampling, the affine should be diagonal (no rotation)."""
        R = rotation_about_x(20)
        affine = make_affine([0.8, 0.8, 3.0], R)
        mag_nii = self._make_nifti(affine)

        mag_rot, _, _ = resample_to_axial(mag_nii=mag_nii)
        resampled_affine = mag_rot.affine

        # Off-diagonal elements of the 3x3 submatrix should be ~0
        rot = resampled_affine[:3, :3]
        off_diag = rot - np.diag(np.diag(rot))
        np.testing.assert_allclose(off_diag, 0, atol=1e-6)

    def test_b0_direction_after_resampling(self):
        """B0 direction computed from resampled data should be [0, 0, 1]."""
        R = rotation_about_x(20)
        affine = make_affine([0.8, 0.8, 3.0], R)
        mag_nii = self._make_nifti(affine)

        mag_rot, _, _ = resample_to_axial(mag_nii=mag_nii)
        b0 = compute_b0_direction(mag_rot.affine)
        np.testing.assert_allclose(b0, [0, 0, 1], atol=1e-6)

    def test_b0_direction_before_vs_after_resampling(self):
        """B0 from original oblique data should differ from B0 after resampling."""
        R = rotation_about_x(30)
        affine = make_affine([0.8, 0.8, 3.0], R)
        mag_nii = self._make_nifti(affine)

        b0_before = compute_b0_direction(affine)
        mag_rot, _, _ = resample_to_axial(mag_nii=mag_nii)
        b0_after = compute_b0_direction(mag_rot.affine)

        # Before: should be tilted (~30 degrees from [0,0,1])
        angle_before = np.degrees(np.arccos(np.clip(np.dot(b0_before, [0, 0, 1]), -1, 1)))
        assert angle_before > 25, f"Pre-resampling B0 should be tilted, got {angle_before} degrees"

        # After: should be [0,0,1]
        np.testing.assert_allclose(b0_after, [0, 0, 1], atol=1e-6)

    def test_no_resampling_preserves_oblique_b0(self):
        """When obliquity is below threshold, files pass through and B0 stays oblique.

        This tests the scenario where resample_files() skips resampling --
        the B0 direction should still be correct for the oblique data.
        """
        R = rotation_about_x(5)  # small tilt
        affine = make_affine([0.8, 0.8, 3.0], R)

        b0 = compute_b0_direction(affine)
        angle = np.degrees(np.arccos(np.clip(np.dot(b0, [0, 0, 1]), -1, 1)))
        np.testing.assert_allclose(angle, 5.0, atol=0.5)
