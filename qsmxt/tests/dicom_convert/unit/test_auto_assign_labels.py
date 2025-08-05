#!/usr/bin/env python3
"""
Unit tests for the auto_assign_initial_labels function in dicom_convert module.
"""

import pytest
from qsmxt.cli.dicom_convert import auto_assign_initial_labels


class TestAutoAssignInitialLabels:
    """Test cases for the auto_assign_initial_labels function."""
    
    def test_empty_table(self):
        """Test with empty table data."""
        table_data = []
        auto_assign_initial_labels(table_data)
        assert table_data == []
    
    def test_single_mag_phase_pair(self):
        """Test single magnitude/phase pair assignment."""
        table_data = [
            {"Acquisition": "gre", "ImageType": ["M"], "Count": 10},
            {"Acquisition": "gre", "ImageType": ["P"], "Count": 10}
        ]
        auto_assign_initial_labels(table_data)
        
        assert table_data[0]["Type"] == "Mag"
        assert table_data[1]["Type"] == "Phase"
        assert table_data[0]["Description"] == ""
        assert table_data[1]["Description"] == ""
    
    def test_single_real_imag_pair(self):
        """Test single real/imaginary pair assignment."""
        table_data = [
            {"Acquisition": "gre", "ImageType": ["REAL"], "Count": 10},
            {"Acquisition": "gre", "ImageType": ["IMAGINARY"], "Count": 10}
        ]
        auto_assign_initial_labels(table_data)
        
        assert table_data[0]["Type"] == "Real"
        assert table_data[1]["Type"] == "Imag"
        assert table_data[0]["Description"] == ""
        assert table_data[1]["Description"] == ""
    
    def test_multiple_mag_phase_pairs(self):
        """Test multiple magnitude/phase pairs get descriptions."""
        table_data = [
            {"Acquisition": "gre", "ImageType": ["M"], "Count": 10, "InversionNumber": None},
            {"Acquisition": "gre", "ImageType": ["P"], "Count": 10, "InversionNumber": None},
            {"Acquisition": "gre", "ImageType": ["M"], "Count": 10, "InversionNumber": None},
            {"Acquisition": "gre", "ImageType": ["P"], "Count": 10, "InversionNumber": None}
        ]
        auto_assign_initial_labels(table_data)
        
        # Should assign descriptions to differentiate pairs
        assert table_data[0]["Type"] == "Mag"
        assert table_data[1]["Type"] == "Phase"
        assert table_data[2]["Type"] == "Mag"
        assert table_data[3]["Type"] == "Phase"
        assert table_data[0]["Description"] == "1"
        assert table_data[1]["Description"] == "1"
        assert table_data[2]["Description"] == "2"
        assert table_data[3]["Description"] == "2"
    
    def test_t1w_detection_by_uni_imagetype(self):
        """Test T1w detection based on UNI in ImageType."""
        table_data = [
            {"Acquisition": "mprage", "ImageType": ["UNI", "M"], "Count": 192}
        ]
        auto_assign_initial_labels(table_data)
        
        assert table_data[0]["Type"] == "T1w"
    
    def test_t1w_detection_by_series_description(self):
        """Test T1w detection based on SeriesDescription."""
        table_data = [
            {"Acquisition": "mprage", "ImageType": ["M"], "SeriesDescription": "T1W_3D", "Count": 192},
            {"Acquisition": "mprage2", "ImageType": ["M"], "SeriesDescription": "UNI-DEN", "Count": 192}
        ]
        auto_assign_initial_labels(table_data)
        
        assert table_data[0]["Type"] == "T1w"
        assert table_data[1]["Type"] == "T1w"
    
    def test_t1w_fallback_by_acquisition_name(self):
        """Test T1w fallback detection based on acquisition name containing T1."""
        table_data = [
            {"Acquisition": "T1_mprage", "ImageType": ["M"], "SeriesDescription": "other", "Count": 192}
        ]
        auto_assign_initial_labels(table_data)
        
        assert table_data[0]["Type"] == "T1w"
    
    def test_mixed_acquisition_types(self):
        """Test mixed acquisitions with different types."""
        table_data = [
            # T1 acquisition
            {"Acquisition": "T1_mprage", "ImageType": ["M"], "Count": 192},
            # GRE acquisition with mag/phase
            {"Acquisition": "gre", "ImageType": ["M"], "Count": 10},
            {"Acquisition": "gre", "ImageType": ["P"], "Count": 10},
            # Another acquisition that should remain Skip
            {"Acquisition": "localizer", "ImageType": ["M"], "Count": 3}
        ]
        auto_assign_initial_labels(table_data)
        
        assert table_data[0]["Type"] == "T1w"
        assert table_data[1]["Type"] == "Mag"
        assert table_data[2]["Type"] == "Phase"
        assert table_data[3]["Type"] == "Skip"
    
    def test_unmatched_pairs_remain_skip(self):
        """Test that unmatched mag/phase remain as Skip."""
        table_data = [
            {"Acquisition": "gre", "ImageType": ["M"], "Count": 10},
            {"Acquisition": "gre", "ImageType": ["P"], "Count": 20}  # Different count
        ]
        auto_assign_initial_labels(table_data)
        
        # Should not pair due to different counts
        assert table_data[0]["Type"] == "Skip"
        assert table_data[1]["Type"] == "Skip"
    
    def test_inversion_number_grouping(self):
        """Test grouping by InversionNumber."""
        table_data = [
            {"Acquisition": "mp2rage", "ImageType": ["M"], "Count": 192, "InversionNumber": 1},
            {"Acquisition": "mp2rage", "ImageType": ["P"], "Count": 192, "InversionNumber": 1},
            {"Acquisition": "mp2rage", "ImageType": ["M"], "Count": 192, "InversionNumber": 2},
            {"Acquisition": "mp2rage", "ImageType": ["P"], "Count": 192, "InversionNumber": 2}
        ]
        auto_assign_initial_labels(table_data)
        
        assert all(row["Type"] in ["Mag", "Phase"] for row in table_data)
        # With different InversionNumbers, they should not need Description differentiation
        assert all(row["Description"] == "" for row in table_data)
    
    def test_string_imagetype_handling(self):
        """Test handling of ImageType as string instead of list."""
        table_data = [
            {"Acquisition": "gre", "ImageType": "M", "Count": 10},
            {"Acquisition": "gre", "ImageType": "P", "Count": 10}
        ]
        auto_assign_initial_labels(table_data)
        
        assert table_data[0]["Type"] == "Mag"
        assert table_data[1]["Type"] == "Phase"
    
    def test_default_values_added(self):
        """Test that default Type and Description are added if missing."""
        table_data = [
            {"Acquisition": "test"}  # Missing Type and Description
        ]
        auto_assign_initial_labels(table_data)
        
        assert "Type" in table_data[0]
        assert "Description" in table_data[0]
        assert table_data[0]["Type"] == "Skip"
        assert table_data[0]["Description"] == ""


@pytest.mark.parametrize("image_types,expected_types", [
    ([["M"], ["P"]], ["Mag", "Phase"]),
    ([["REAL"], ["IMAGINARY"]], ["Real", "Imag"]),
    ([["M"], ["M"]], ["Skip", "Skip"]),  # Two mags, no phase
    ([["P"], ["P"]], ["Skip", "Skip"]),  # Two phases, no mag
])
def test_auto_assign_parametrized(image_types, expected_types):
    """Parametrized test for various ImageType combinations."""
    table_data = [
        {"Acquisition": "test", "ImageType": img_type, "Count": 10}
        for img_type in image_types
    ]
    auto_assign_initial_labels(table_data)
    
    actual_types = [row["Type"] for row in table_data]
    assert actual_types == expected_types