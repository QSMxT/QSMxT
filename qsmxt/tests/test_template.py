#!/usr/bin/env pytest
import os
import tempfile
import pytest

import numpy as np
import qsm_forward
from qsmxt.cli.main import main

from qsmxt.scripts.logger import LogLevel, make_logger
from qsmxt.scripts.sys_cmd import sys_cmd
from qsmxt.tests.utils import *

run_workflows = True

@pytest.fixture
def bids_dir_public():
    tmp_dir = tempfile.gettempdir()
    bids_dir = os.path.join(tmp_dir, "bids-public")
    if not os.path.exists(bids_dir):

        head_phantom_maps_dir = os.path.join(tmp_dir, 'head-phantom-maps')
        if not os.path.exists(head_phantom_maps_dir):
            if not os.path.exists(os.path.join(tmp_dir, 'head-phantom-maps.tar')):
                download_from_osf(
                    project="9jc42",
                    local_path=os.path.join(tmp_dir, "head-phantom-maps.tar")
                )

            sys_cmd(f"tar xf {os.path.join(tmp_dir, 'head-phantom-maps.tar')} -C {tmp_dir}")
            sys_cmd(f"rm {os.path.join(tmp_dir, 'head-phantom-maps.tar')}")

        tissue_params = qsm_forward.TissueParams(os.path.join(tmp_dir, 'head-phantom-maps'))
        
        recon_params_all = [
            qsm_forward.ReconParams(voxel_size=np.array([1.0, 1.0, 1.0]), subject=subject, TEs=TEs, TR=TR, flip_angle=flip_angle, suffix=suffix, export_phase=export_phase)
            for (subject, TEs, TR, flip_angle, suffix, export_phase) in [
                ("1", np.array([0.004, 0.012]), 0.05, 15, "T2starw", True),
                ("2", np.array([0.004, 0.012]), 0.05, 15, "T2starw", True)
            ]
        ]

        bids_dir = os.path.join(tmp_dir, "bids-public")
        for recon_params in recon_params_all:
            qsm_forward.generate_bids(tissue_params=tissue_params, recon_params=recon_params, bids_dir=bids_dir)
        sys_cmd(f"rm {head_phantom_maps_dir}")

    return bids_dir

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflows, None)
])
def test_template(bids_dir_public, init_workflow, run_workflow, run_args):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING TEMPLATE PIPELINE ===")
    os.makedirs(os.path.join(tempfile.gettempdir(), "public-outputs"), exist_ok=True)

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir_public,
        os.path.join(tempfile.gettempdir(), "output"),
        "--do_qsm", "yes",
        "--premade", "fast",
        "--do_template", "yes",
        "--auto_yes",
        "--debug",
    ]
    
    # create the workflow and run - it should fall back to phase-based masking with a warning
    args = main(args)

    # generate image - index out of range
    add_to_github_summary(f"![result]({upload_png(display_nii(glob.glob(os.path.join(args.output_dir, 'template', 'qsm_template', '*', '*.*'))[0], title='QSM template', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm_template.png', cmap='gray'))})")
    add_to_github_summary(f"![result]({upload_png(display_nii(glob.glob(os.path.join(args.output_dir, 'template', 'magnitude_template', '*.*'))[0], title='Magnitude template', out_png='mag_template.png', cmap='gray'))})")

