#!/usr/bin/env python3
"""
Unit tests for the clean function in dicom_convert module.
"""

import pytest
from qsmxt.cli.dicom_convert import clean


class TestCleanFunction:
    """Test cases for the clean function."""
    
    def test_clean_basic_string(self):
        """Test basic string cleaning."""
        assert clean("hello") == "hello"
        assert clean("HELLO") == "hello"
        assert clean("HeLLo") == "hello"
    
    def test_clean_with_special_characters(self):
        """Test cleaning strings with special characters."""
        assert clean("hello-world") == "helloworld"
        assert clean("hello_world") == "helloworld"
        assert clean("hello.world") == "helloworld"
        assert clean("hello@world!") == "helloworld"
        assert clean("123-test-456") == "123test456"
    
    def test_clean_with_whitespace(self):
        """Test cleaning strings with whitespace."""
        assert clean("  hello  ") == "hello"
        assert clean("hello world") == "helloworld"
        assert clean("\thello\n") == "hello"
        assert clean(" multiple   spaces ") == "multiplespaces"
    
    def test_clean_with_sub_prefix(self):
        """Test cleaning strings with 'sub-' prefix."""
        assert clean("sub-001") == "sub-001"
        assert clean("sub-patient123") == "sub-patient123"
        assert clean("SUB-TEST") == "subtest"  # Uppercase SUB is not recognized as prefix
        assert clean("sub-ABC_123") == "sub-abc123"
        assert clean("  sub-001  ") == "sub-001"
    
    def test_clean_with_ses_prefix(self):
        """Test cleaning strings with 'ses-' prefix."""
        assert clean("ses-001") == "ses-001"
        assert clean("ses-baseline") == "ses-baseline"
        assert clean("SES-FOLLOWUP") == "sesfollowup"  # Uppercase SES is not recognized as prefix
        assert clean("ses-2023_01_15") == "ses-20230115"
        assert clean("  ses-001  ") == "ses-001"
    
    def test_clean_empty_and_edge_cases(self):
        """Test edge cases."""
        assert clean("") == ""
        assert clean("   ") == ""
        assert clean("sub-") == "sub-"
        assert clean("ses-") == "ses-"
        assert clean("!!!") == ""
        assert clean("___") == ""
    
    def test_clean_mixed_cases(self):
        """Test mixed scenarios."""
        assert clean("Patient_Name-123") == "patientname123"
        assert clean("2023-01-15_scan") == "20230115scan"
        assert clean("MRI@Scanner#1") == "mriscanner1"
        assert clean("sub-Patient_Name-123") == "sub-patientname123"
        assert clean("ses-2023-01-15_scan") == "ses-20230115scan"
    
    def test_clean_unicode_characters(self):
        """Test cleaning strings with unicode characters."""
        assert clean("café") == "caf"  # Non-ASCII characters are removed
        assert clean("naïve") == "nave"
        assert clean("测试") == ""  # Chinese characters removed
        assert clean("test™") == "test"
    
    def test_clean_numeric_strings(self):
        """Test cleaning numeric strings."""
        assert clean("123") == "123"
        assert clean("001") == "001"
        assert clean("3.14159") == "314159"
        assert clean("-123") == "123"
        assert clean("+456") == "456"


@pytest.mark.parametrize("input_str,expected", [
    ("test", "test"),
    ("TEST", "test"),
    ("test-123", "test123"),
    ("sub-001", "sub-001"),
    ("ses-001", "ses-001"),
    ("", ""),
    ("   ", ""),
    ("@#$%", ""),
    ("sub-ABC_123", "sub-abc123"),
    ("ses-2023_01_15", "ses-20230115"),
])
def test_clean_parametrized(input_str, expected):
    """Parametrized test for various clean function inputs."""
    assert clean(input_str) == expected