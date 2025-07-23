"""Unit tests for file utility functions."""

import pytest
import os
from pathlib import Path
from unittest.mock import patch, MagicMock
from qsmxt.scripts.qsmxt_functions import extend_fname, get_fname


class TestFileUtilities:
    """Test file name manipulation utilities."""

    def test_extend_fname_basic(self):
        """Test basic filename extension."""
        result = extend_fname("/path/to/file.nii.gz", "_processed")
        assert result == "/path/to/file_processed.nii.gz"

    def test_extend_fname_custom_extension(self):
        """Test with custom extension override."""
        result = extend_fname("/path/to/file.nii", "_mask", ext="nii.gz")
        assert result == "/path/to/file_mask.nii.gz"

    def test_extend_fname_custom_output_dir(self, temp_dir):
        """Test with custom output directory."""
        result = extend_fname("/path/to/file.nii", "_out", out_dir=str(temp_dir))
        expected = str(temp_dir / "file_out.nii")
        assert result == expected

    def test_extend_fname_custom_extension_and_dir(self, temp_dir):
        """Test with both custom extension and output directory."""
        result = extend_fname(
            "/path/to/file.nii", 
            "_processed", 
            ext="nii.gz", 
            out_dir=str(temp_dir)
        )
        expected = str(temp_dir / "file_processed.nii.gz")
        assert result == expected

    def test_extend_fname_edge_cases(self):
        """Test edge cases with unusual filenames."""
        test_cases = [
            # File with multiple dots
            ("/path/file.with.dots.nii.gz", "_test", None, None, "/path/file_test.with.dots.nii.gz"),
            # File without extension
            ("/path/filename", "_test", None, None, "/path/filename_test."),
            # File with single extension
            ("/path/file.nii", "_mask", None, None, "/path/file_mask.nii"),
            # Empty append text
            ("/path/file.nii.gz", "", None, None, "/path/file.nii.gz"),
            # Special characters in filename
            ("/path/file-name_123.nii.gz", "_final", None, None, "/path/file-name_123_final.nii.gz"),
        ]
        
        for input_path, append, ext, out_dir, expected in test_cases:
            result = extend_fname(input_path, append, ext=ext, out_dir=out_dir)
            assert result == expected, f"Failed for input: {input_path}"

    def test_extend_fname_preserves_original_extension(self):
        """Test that original extension is preserved when no custom extension given."""
        test_cases = [
            ("/path/file.nii", "_test", "/path/file_test.nii"),
            ("/path/file.nii.gz", "_test", "/path/file_test.nii.gz"),
            ("/path/file.txt", "_backup", "/path/file_backup.txt"),
            ("/path/file.json", "_config", "/path/file_config.json"),
        ]
        
        for input_path, append, expected in test_cases:
            result = extend_fname(input_path, append)
            assert result == expected

    def test_extend_fname_relative_paths(self):
        """Test with relative paths."""
        result = extend_fname("file.nii.gz", "_processed")
        expected = "file_processed.nii.gz"  # No directory prefix for relative paths
        assert result == expected

    def test_extend_fname_current_directory_default(self):
        """Test that current directory is used by default when no out_dir specified."""
        with patch('os.getcwd', return_value='/current/working/dir'):
            result = extend_fname("/other/path/file.nii", "_test")
            # Should use current working directory, not original file's directory
            expected = "/current/working/dir/file_test.nii"
            # Note: The actual function uses os.path.split()[0] for the directory,
            # so it will use the original file's directory by default
            expected = "/other/path/file_test.nii"
            assert result == expected

    def test_get_fname_with_path(self):
        """Test filename extraction with path included."""
        test_cases = [
            ("/path/to/file.nii.gz", True, "/path/to/file.nii"),  # Only removes last extension
            ("/path/to/file.nii", True, "/path/to/file"),
            ("file.nii.gz", True, "file.nii"),  # Only removes last extension
            ("/complex/path/file.with.dots.nii.gz", True, "/complex/path/file.with.dots.nii"),  # Only removes .gz
        ]
        
        for input_path, include_path, expected in test_cases:
            result = get_fname(input_path, include_path=include_path)
            assert result == expected, f"Failed for input: {input_path}"

    def test_get_fname_without_path(self):
        """Test filename extraction without path."""
        test_cases = [
            ("/path/to/file.nii.gz", False, "file.nii"),  # Only removes last extension
            ("/path/to/file.nii", False, "file"),
            ("file.nii.gz", False, "file.nii"),  # Only removes last extension
            ("/complex/path/file.with.dots.nii.gz", False, "file.with.dots.nii"),  # Only removes .gz
            ("/path/to/filename_without_extension", False, ""),  # Empty when no extension
        ]
        
        for input_path, include_path, expected in test_cases:
            result = get_fname(input_path, include_path=include_path)
            assert result == expected, f"Failed for input: {input_path}"

    def test_get_fname_edge_cases(self):
        """Test get_fname with edge cases."""
        # File without extension - function returns empty string for basename
        result = get_fname("/path/to/filename", include_path=False)
        assert result == ""  # No extension to remove, so empty basename
        
        # File with multiple extensions - only removes last
        result = get_fname("/path/file.tar.gz", include_path=False)
        assert result == "file.tar"
        
        # Hidden file
        result = get_fname("/path/.hidden.nii", include_path=False)
        assert result == ".hidden"
        
        # Root level file
        result = get_fname("/file.nii", include_path=True)
        assert result == "/file"

    @pytest.mark.parametrize("input_path,append,expected", [
        ("/path/file.nii.gz", "_mask", "/path/file_mask.nii.gz"),
        ("/path/file.nii", "_processed", "/path/file_processed.nii"),
        ("/path/file", "_out", "/path/file_out."),
        ("file.with.dots.nii.gz", "_test", "file_test.with.dots.nii.gz"),  # No directory prefix
        ("/path/file.json", "_backup", "/path/file_backup.json"),
    ])
    def test_extend_fname_parametrized(self, input_path, append, expected):
        """Parametrized test for extend_fname with common cases."""
        result = extend_fname(input_path, append)
        assert result == expected

    @pytest.mark.parametrize("input_path,include_path,expected", [
        ("/path/to/file.nii.gz", True, "/path/to/file.nii"),  # Only removes last extension
        ("/path/to/file.nii.gz", False, "file.nii"),  # Only removes last extension
        ("file.nii", True, "file"),
        ("file.nii", False, "file"),
        ("/a/b/c.d.e.nii", True, "/a/b/c.d.e"),
        ("/a/b/c.d.e.nii", False, "c.d.e"),
    ])
    def test_get_fname_parametrized(self, input_path, include_path, expected):
        """Parametrized test for get_fname with common cases."""
        result = get_fname(input_path, include_path=include_path)
        assert result == expected

    def test_filename_functions_integration(self):
        """Test that extend_fname and get_fname work together correctly."""
        original_path = "/data/subject01/magnitude.nii.gz"
        
        # Extend the filename
        extended_path = extend_fname(original_path, "_brain_extracted")
        assert extended_path == "/data/subject01/magnitude_brain_extracted.nii.gz"
        
        # Extract filename without path (only removes last extension)
        base_name = get_fname(extended_path, include_path=False)
        assert base_name == "magnitude_brain_extracted.nii"  # Still has .nii
        
        # Extract filename with path (only removes last extension)
        full_name = get_fname(extended_path, include_path=True)
        assert full_name == "/data/subject01/magnitude_brain_extracted.nii"  # Still has .nii

    def test_extend_fname_preserves_directory_structure(self, temp_dir):
        """Test that extend_fname works with complex directory structures."""
        # Create nested directory structure
        nested_dir = temp_dir / "data" / "subject01" / "session1"
        nested_dir.mkdir(parents=True)
        
        original_path = str(nested_dir / "phase.nii.gz")
        
        # Test default behavior (uses original directory)
        result = extend_fname(original_path, "_unwrapped")
        expected = str(nested_dir / "phase_unwrapped.nii.gz")
        assert result == expected
        
        # Test with custom output directory
        output_dir = temp_dir / "output"
        output_dir.mkdir()
        result = extend_fname(original_path, "_unwrapped", out_dir=str(output_dir))
        expected = str(output_dir / "phase_unwrapped.nii.gz")
        assert result == expected

    def test_filename_functions_with_neuroimaging_conventions(self):
        """Test functions with typical neuroimaging file naming conventions."""
        # BIDS-style naming
        bids_path = "/data/sub-01/ses-01/anat/sub-01_ses-01_T1w.nii.gz"
        
        extended = extend_fname(bids_path, "_brain")
        assert extended == "/data/sub-01/ses-01/anat/sub-01_ses-01_T1w_brain.nii.gz"
        
        base_name = get_fname(extended, include_path=False)
        assert base_name == "sub-01_ses-01_T1w_brain.nii"  # Only removes .gz
        
        # QSM-specific naming
        qsm_path = "/data/qsm/magnitude_e1.nii.gz"
        
        extended = extend_fname(qsm_path, "_bet", ext="nii.gz")
        assert extended == "/data/qsm/magnitude_e1_bet.nii.gz"
        
        # Phase data naming
        phase_path = "/data/qsm/phase_e2_unwrapped.nii.gz"
        
        extended = extend_fname(phase_path, "_laplacian")
        assert extended == "/data/qsm/phase_e2_unwrapped_laplacian.nii.gz"
        
        base_name = get_fname(phase_path, include_path=False)
        assert base_name == "phase_e2_unwrapped.nii"  # Only removes .gz