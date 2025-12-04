"""
Tests for Boutiques descriptor validation.

These tests ensure the QSMxT Boutiques descriptor is valid and can be used
with the Boutiques framework for reproducible execution.
"""

import glob
import json
import os
import shutil
import tempfile

import pytest

# Path to the boutiques descriptor
BOUTIQUES_DIR = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'boutiques')
DESCRIPTOR_PATH = os.path.join(BOUTIQUES_DIR, 'qsmxt.json')
TEST_INVOCATION_PATH = os.path.join(BOUTIQUES_DIR, 'test_invocation.json')


def gettempdir():
    """Get temp directory from environment or use system default."""
    return os.environ.get('TEST_DIR') or tempfile.gettempdir()


class TestBoutiquesDescriptor:
    """Test suite for Boutiques descriptor validation."""

    def test_descriptor_exists(self):
        """Test that the Boutiques descriptor file exists."""
        assert os.path.exists(DESCRIPTOR_PATH), f"Descriptor not found at {DESCRIPTOR_PATH}"

    def test_descriptor_is_valid_json(self):
        """Test that the descriptor is valid JSON."""
        with open(DESCRIPTOR_PATH, 'r') as f:
            descriptor = json.load(f)
        assert isinstance(descriptor, dict)

    def test_descriptor_has_required_fields(self):
        """Test that the descriptor has all required Boutiques fields."""
        with open(DESCRIPTOR_PATH, 'r') as f:
            descriptor = json.load(f)

        required_fields = ['name', 'description', 'tool-version', 'schema-version',
                          'command-line', 'inputs']
        for field in required_fields:
            assert field in descriptor, f"Missing required field: {field}"

    def test_descriptor_name_matches(self):
        """Test that the descriptor name matches 'qsmxt'."""
        with open(DESCRIPTOR_PATH, 'r') as f:
            descriptor = json.load(f)
        assert descriptor['name'] == 'qsmxt'

    def test_all_inputs_have_required_fields(self):
        """Test that all inputs have required fields (id, name, type, value-key)."""
        with open(DESCRIPTOR_PATH, 'r') as f:
            descriptor = json.load(f)

        for input_def in descriptor['inputs']:
            assert 'id' in input_def, f"Input missing 'id': {input_def}"
            assert 'name' in input_def, f"Input {input_def['id']} missing 'name'"
            assert 'type' in input_def, f"Input {input_def['id']} missing 'type'"
            assert 'value-key' in input_def, f"Input {input_def['id']} missing 'value-key'"

    def test_value_keys_in_command_line(self):
        """Test that all value-keys are present in the command-line template."""
        with open(DESCRIPTOR_PATH, 'r') as f:
            descriptor = json.load(f)

        command_line = descriptor['command-line']
        for input_def in descriptor['inputs']:
            value_key = input_def['value-key']
            assert value_key in command_line, \
                f"Value-key {value_key} for input {input_def['id']} not found in command-line"

    def test_input_ids_are_unique(self):
        """Test that all input IDs are unique."""
        with open(DESCRIPTOR_PATH, 'r') as f:
            descriptor = json.load(f)

        ids = [inp['id'] for inp in descriptor['inputs']]
        assert len(ids) == len(set(ids)), "Duplicate input IDs found"

    def test_test_invocation_exists(self):
        """Test that the test invocation file exists."""
        assert os.path.exists(TEST_INVOCATION_PATH), \
            f"Test invocation not found at {TEST_INVOCATION_PATH}"

    def test_test_invocation_is_valid_json(self):
        """Test that the test invocation is valid JSON."""
        with open(TEST_INVOCATION_PATH, 'r') as f:
            invocation = json.load(f)
        assert isinstance(invocation, dict)

    def test_test_invocation_has_required_inputs(self):
        """Test that the test invocation provides required inputs."""
        with open(TEST_INVOCATION_PATH, 'r') as f:
            invocation = json.load(f)

        # bids_dir is the only required input
        assert 'bids_dir' in invocation, "Test invocation missing required 'bids_dir'"

    @pytest.mark.skipif(
        not os.environ.get('TEST_BOUTIQUES_VALIDATION', False),
        reason="Full Boutiques validation requires boutiques package; set TEST_BOUTIQUES_VALIDATION=1 to enable"
    )
    def test_boutiques_validate(self):
        """Test descriptor validation using boutiques bosh validate."""
        from boutiques import bosh

        # This will raise an exception if validation fails
        bosh(['validate', DESCRIPTOR_PATH])

    @pytest.mark.skipif(
        not os.environ.get('TEST_BOUTIQUES_VALIDATION', False),
        reason="Full Boutiques validation requires boutiques package; set TEST_BOUTIQUES_VALIDATION=1 to enable"
    )
    def test_boutiques_simulate(self):
        """Test command simulation using boutiques."""
        from boutiques import bosh

        # Simulate execution - this validates the invocation against the descriptor
        result = bosh(['exec', 'simulate', '-i', TEST_INVOCATION_PATH, DESCRIPTOR_PATH])
        assert result is not None


class TestBoutiquesInputsCoverCLI:
    """Test that Boutiques inputs cover the actual CLI arguments."""

    def test_major_cli_args_covered(self):
        """Test that major CLI arguments are represented in the Boutiques descriptor."""
        with open(DESCRIPTOR_PATH, 'r') as f:
            descriptor = json.load(f)

        input_ids = {inp['id'] for inp in descriptor['inputs']}

        # Essential arguments that must be present
        essential_args = [
            'bids_dir', 'output_dir', 'do_qsm', 'do_segmentation',
            'do_analysis', 'do_template', 'qsm_algorithm', 'premade',
            'masking_algorithm', 'subjects', 'sessions', 'auto_yes',
            'n_procs', 'debug', 'dry'
        ]

        for arg in essential_args:
            assert arg in input_ids, f"Essential CLI argument '{arg}' not in Boutiques inputs"

    def test_qsm_algorithm_choices(self):
        """Test that QSM algorithm choices match the CLI."""
        with open(DESCRIPTOR_PATH, 'r') as f:
            descriptor = json.load(f)

        qsm_input = next(inp for inp in descriptor['inputs'] if inp['id'] == 'qsm_algorithm')
        expected_choices = ['tgv', 'tv', 'nextqsm', 'rts']

        assert 'value-choices' in qsm_input
        assert set(qsm_input['value-choices']) == set(expected_choices)

    def test_masking_algorithm_choices(self):
        """Test that masking algorithm choices match the CLI."""
        with open(DESCRIPTOR_PATH, 'r') as f:
            descriptor = json.load(f)

        mask_input = next(inp for inp in descriptor['inputs'] if inp['id'] == 'masking_algorithm')
        expected_choices = ['threshold', 'bet']

        assert 'value-choices' in mask_input
        assert set(mask_input['value-choices']) == set(expected_choices)


class TestBoutiquesIntegration:
    """Integration tests that actually run QSMxT via Boutiques."""

    @pytest.mark.skipif(
        not os.environ.get('TEST_BOUTIQUES_INTEGRATION', False),
        reason="Boutiques integration tests require container and test data; set TEST_BOUTIQUES_INTEGRATION=1 to enable"
    )
    def test_boutiques_exec_launch(self):
        """
        Test actual execution of QSMxT via bosh exec launch.

        This test:
        1. Uses BIDS data generated by setup_qsmxt.sh (same as other QSM tests)
        2. Creates a dynamic invocation JSON
        3. Runs QSMxT via boutiques
        4. Verifies outputs exist
        """
        import datetime
        from boutiques import bosh

        # Use the same BIDS directory as other QSM tests (generated by setup_qsmxt.sh)
        tmp_dir = gettempdir()
        bids_dir = os.path.join(tmp_dir, "bids")

        # Verify BIDS data exists
        assert os.path.exists(bids_dir), f"BIDS directory not found at {bids_dir}"
        assert glob.glob(os.path.join(bids_dir, "sub-*")), "No subjects found in BIDS directory"

        # Create output directory
        output_dir = os.path.join(
            tmp_dir,
            f"boutiques-output-{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}"
        )
        os.makedirs(output_dir, exist_ok=True)

        # Create dynamic invocation
        invocation = {
            "bids_dir": bids_dir,
            "output_dir": output_dir,
            "do_qsm": "on",
            "premade": "fast",
            "subjects": ["sub-1"],
            "sessions": ["ses-1"],
            "auto_yes": True,
            "debug": True
        }

        invocation_path = os.path.join(tmp_dir, "boutiques_integration_invocation.json")
        with open(invocation_path, 'w') as f:
            json.dump(invocation, f)

        try:
            # Run via boutiques with --no-container since we're already inside the QSMxT container
            bosh(['exec', 'launch', '--no-container', DESCRIPTOR_PATH, invocation_path])

            # Verify outputs
            chimap_files = glob.glob(os.path.join(output_dir, '**', '*_Chimap.nii*'), recursive=True)
            assert len(chimap_files) > 0, f"No Chimap output files found in {output_dir}"

        finally:
            # Cleanup
            if os.path.exists(invocation_path):
                os.remove(invocation_path)
            if os.path.exists(output_dir):
                shutil.rmtree(output_dir)

    @pytest.mark.skipif(
        not os.environ.get('TEST_BOUTIQUES_VALIDATION', False),
        reason="Full Boutiques validation requires boutiques package; set TEST_BOUTIQUES_VALIDATION=1 to enable"
    )
    def test_boutiques_invocation_validation(self):
        """
        Test that a real invocation validates against the descriptor.

        This doesn't run the pipeline, just validates the invocation schema.
        Uses placeholder paths since we're only validating the JSON structure.
        """
        from boutiques import bosh

        # Create a realistic invocation (paths don't need to exist for simulation)
        invocation = {
            "bids_dir": "/path/to/bids",
            "output_dir": "/path/to/output",
            "do_qsm": "on",
            "premade": "fast",
            "auto_yes": True
        }

        invocation_path = os.path.join(gettempdir(), "boutiques_validation_invocation.json")
        with open(invocation_path, 'w') as f:
            json.dump(invocation, f)

        try:
            # This validates the invocation against the descriptor without running
            result = bosh(['exec', 'simulate', '-i', invocation_path, DESCRIPTOR_PATH])
            assert result is not None
            # result is an ExecutorOutput object, check the command-line string
            assert hasattr(result, 'shell_command') or 'qsmxt' in str(result)
        finally:
            if os.path.exists(invocation_path):
                os.remove(invocation_path)
