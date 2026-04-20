"""Unit test for version retrieval."""

from qsmxt.scripts.qsmxt_functions import get_qsmxt_version


def test_get_qsmxt_version():
    version = get_qsmxt_version()
    assert isinstance(version, str)
    assert len(version) > 0
