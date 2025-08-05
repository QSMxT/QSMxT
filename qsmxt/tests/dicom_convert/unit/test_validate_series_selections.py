#!/usr/bin/env python3
"""
Unit tests for the validate_series_selections function in dicom_convert module.
"""

import pytest
from qsmxt.cli.dicom_convert import validate_series_selections


class TestValidateSeriesSelections:
    """Test cases for the validate_series_selections function."""
    
    def test_empty_table(self):
        """Test with empty table data."""
        errors = validate_series_selections([])
        assert errors == []
    
    def test_valid_mag_phase_pair(self):
        """Test valid magnitude/phase pair."""
        table_data = [
            {"Acquisition": "gre", "Type": "Mag", "Count": 10},
            {"Acquisition": "gre", "Type": "Phase", "Count": 10}
        ]
        errors = validate_series_selections(table_data)
        assert errors == []
    
    def test_valid_real_imag_pair(self):
        """Test valid real/imaginary pair."""
        table_data = [
            {"Acquisition": "gre", "Type": "Real", "Count": 10},
            {"Acquisition": "gre", "Type": "Imag", "Count": 10}
        ]
        errors = validate_series_selections(table_data)
        assert errors == []
    
    def test_mag_without_phase(self):
        """Test magnitude without corresponding phase."""
        table_data = [
            {"Acquisition": "gre", "Type": "Mag", "Count": 10}
        ]
        errors = validate_series_selections(table_data)
        assert len(errors) == 1
        assert "Mag series (Count=10) requires at least one Phase series" in errors[0]
    
    def test_phase_without_mag(self):
        """Test phase without corresponding magnitude."""
        table_data = [
            {"Acquisition": "gre", "Type": "Phase", "Count": 10}
        ]
        errors = validate_series_selections(table_data)
        assert len(errors) == 1
        assert "Phase series (Count=10) requires at least one Mag series" in errors[0]
    
    def test_real_without_imag(self):
        """Test real without corresponding imaginary."""
        table_data = [
            {"Acquisition": "gre", "Type": "Real", "Count": 10}
        ]
        errors = validate_series_selections(table_data)
        assert len(errors) == 1
        assert "Real series (Count=10) requires at least one Imag series" in errors[0]
    
    def test_imag_without_real(self):
        """Test imaginary without corresponding real."""
        table_data = [
            {"Acquisition": "gre", "Type": "Imag", "Count": 10}
        ]
        errors = validate_series_selections(table_data)
        assert len(errors) == 1
        assert "Imag series (Count=10) requires at least one Real series" in errors[0]
    
    def test_mismatched_counts_mag_phase(self):
        """Test magnitude and phase with different counts."""
        table_data = [
            {"Acquisition": "gre", "Type": "Mag", "Count": 10},
            {"Acquisition": "gre", "Type": "Phase", "Count": 20}
        ]
        errors = validate_series_selections(table_data)
        assert len(errors) == 3  # Mag needs Phase with Count=10, Phase needs Mag with Count=20, plus mismatch error
        assert any("non-matching number of images (10 vs. 20)" in err for err in errors)
    
    def test_mismatched_counts_real_imag(self):
        """Test real and imaginary with different counts."""
        table_data = [
            {"Acquisition": "gre", "Type": "Real", "Count": 10},
            {"Acquisition": "gre", "Type": "Imag", "Count": 20}
        ]
        errors = validate_series_selections(table_data)
        assert len(errors) == 3
        assert any("non-matching number of images (10 vs. 20)" in err for err in errors)
    
    def test_multiple_mag_without_differentiation(self):
        """Test multiple magnitude series without proper differentiation."""
        table_data = [
            {"Acquisition": "gre", "Type": "Mag", "Count": 10, "InversionNumber": "", "Description": ""},
            {"Acquisition": "gre", "Type": "Mag", "Count": 10, "InversionNumber": "", "Description": ""},
            {"Acquisition": "gre", "Type": "Phase", "Count": 10, "InversionNumber": "", "Description": ""},
            {"Acquisition": "gre", "Type": "Phase", "Count": 10, "InversionNumber": "", "Description": ""}
        ]
        errors = validate_series_selections(table_data)
        assert len(errors) == 1
        assert "Multiple Mag/Phase series selections must be differentiated" in errors[0]
    
    def test_multiple_mag_with_inversion_numbers(self):
        """Test multiple magnitude series differentiated by InversionNumber."""
        table_data = [
            {"Acquisition": "gre", "Type": "Mag", "Count": 10, "InversionNumber": "1", "Description": ""},
            {"Acquisition": "gre", "Type": "Mag", "Count": 10, "InversionNumber": "2", "Description": ""},
            {"Acquisition": "gre", "Type": "Phase", "Count": 10, "InversionNumber": "1", "Description": ""},
            {"Acquisition": "gre", "Type": "Phase", "Count": 10, "InversionNumber": "2", "Description": ""}
        ]
        errors = validate_series_selections(table_data)
        assert errors == []  # Should be valid
    
    def test_multiple_mag_with_descriptions(self):
        """Test multiple magnitude series differentiated by Description."""
        table_data = [
            {"Acquisition": "gre", "Type": "Mag", "Count": 10, "InversionNumber": "", "Description": "echo1"},
            {"Acquisition": "gre", "Type": "Mag", "Count": 10, "InversionNumber": "", "Description": "echo2"},
            {"Acquisition": "gre", "Type": "Phase", "Count": 10, "InversionNumber": "", "Description": "echo1"},
            {"Acquisition": "gre", "Type": "Phase", "Count": 10, "InversionNumber": "", "Description": "echo2"}
        ]
        errors = validate_series_selections(table_data)
        assert errors == []  # Should be valid
    
    def test_multiple_real_without_differentiation(self):
        """Test multiple real series without proper differentiation."""
        table_data = [
            {"Acquisition": "gre", "Type": "Real", "Count": 10, "InversionNumber": "", "Description": ""},
            {"Acquisition": "gre", "Type": "Real", "Count": 10, "InversionNumber": "", "Description": ""},
            {"Acquisition": "gre", "Type": "Imag", "Count": 10, "InversionNumber": "", "Description": ""},
            {"Acquisition": "gre", "Type": "Imag", "Count": 10, "InversionNumber": "", "Description": ""}
        ]
        errors = validate_series_selections(table_data)
        assert len(errors) == 1
        assert "Multiple Real/Imag series selections must be differentiated" in errors[0]
    
    def test_mixed_valid_invalid_acquisitions(self):
        """Test mix of valid and invalid acquisitions."""
        table_data = [
            # Valid pair in acquisition1
            {"Acquisition": "acquisition1", "Type": "Mag", "Count": 10},
            {"Acquisition": "acquisition1", "Type": "Phase", "Count": 10},
            # Invalid in acquisition2 (mag without phase)
            {"Acquisition": "acquisition2", "Type": "Mag", "Count": 20},
            # Valid T1w in acquisition3
            {"Acquisition": "acquisition3", "Type": "T1w", "Count": 192},
            # Skip types should not generate errors
            {"Acquisition": "acquisition4", "Type": "Skip", "Count": 5}
        ]
        errors = validate_series_selections(table_data)
        assert len(errors) == 1
        assert "acquisition2" in errors[0]
        assert "Mag series (Count=20) requires at least one Phase series" in errors[0]
    
    def test_none_and_empty_values_handling(self):
        """Test handling of None and empty values."""
        table_data = [
            {"Acquisition": "gre", "Type": "Mag", "Count": 10, "InversionNumber": None, "Description": None},
            {"Acquisition": "gre", "Type": "Phase", "Count": 10, "InversionNumber": None, "Description": None}
        ]
        errors = validate_series_selections(table_data)
        assert errors == []  # Should handle None values gracefully


@pytest.mark.parametrize("table_data,expected_error_count", [
    # Valid pairs
    ([{"Acquisition": "a", "Type": "Mag", "Count": 10}, 
      {"Acquisition": "a", "Type": "Phase", "Count": 10}], 0),
    # Missing phase
    ([{"Acquisition": "a", "Type": "Mag", "Count": 10}], 1),
    # Mismatched counts
    ([{"Acquisition": "a", "Type": "Mag", "Count": 10}, 
      {"Acquisition": "a", "Type": "Phase", "Count": 20}], 3),
    # T1w only (valid)
    ([{"Acquisition": "a", "Type": "T1w", "Count": 192}], 0),
    # Skip only (valid)
    ([{"Acquisition": "a", "Type": "Skip", "Count": 10}], 0),
])
def test_validate_series_parametrized(table_data, expected_error_count):
    """Parametrized test for various validation scenarios."""
    errors = validate_series_selections(table_data)
    assert len(errors) == expected_error_count