#!/usr/bin/env pytest
import os
import tempfile
import pytest
import csv

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
            qsm_forward.ReconParams(voxel_size=np.array([1.0, 1.0, 1.0]), session=session, TEs=TEs, TR=TR, flip_angle=flip_angle, suffix=suffix, export_phase=export_phase)
            for (session, TEs, TR, flip_angle, suffix, export_phase) in [
                ("1", np.array([3.5e-3]), 7.5e-3, 40, "T1w", False),
                ("1", np.array([0.012, 0.020]), 0.05, 15, "T2starw", True),
            ]
        ]

        bids_dir = os.path.join(tmp_dir, "bids-public")
        for recon_params in recon_params_all:
            qsm_forward.generate_bids(tissue_params=tissue_params, recon_params=recon_params, bids_dir=bids_dir)
        sys_cmd(f"rm {head_phantom_maps_dir}")

    return bids_dir

def csv_to_markdown(csv_file_path):
    markdown_table = ""
    
    # Open the CSV file
    with open(csv_file_path, 'r') as csvfile:
        reader = csv.reader(csvfile)
        
        # Initialize headers
        headers = next(reader)
        markdown_table += "| " + " | ".join(headers) + " |\n"
        markdown_table += "| " + "--- |" * (len(headers)-1) + "--- |\n"
        
        # Iterate through rows
        for row in reader:
            # Round the floating-point values to 5 decimals
            rounded_row = [str(round(float(cell), 5)) if is_float(cell) else cell for cell in row]
            markdown_table += "| " + " | ".join(rounded_row) + " |\n"
            
    return markdown_table

def is_float(value):
    try:
        float(value)
        return True
    except ValueError:
        return False

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflows, None)
])
def test_segmentation(bids_dir_public, init_workflow, run_workflow, run_args):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING SEGMENTATION PIPELINE ===")
    os.makedirs(os.path.join(tempfile.gettempdir(), "public-outputs"), exist_ok=True)

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir_public,
        os.path.join(tempfile.gettempdir(), "output"),
        "--do_qsm",
        "--premade", "fast",
        "--do_segmentation",
        "--do_analysis",
        "--auto_yes",
        "--debug",
    ]

    if not run_workflows: args += ['--dry']
    
    # create the workflow and run - it should fall back to phase-based masking with a warning
    args = main(args)

    # generate image
    add_to_github_summary(f"![result]({upload_png(display_nii(glob.glob(os.path.join(args.output_dir, 'qsm', '*.*'))[0], title='QSM', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")
    add_to_github_summary(f"![result]({upload_png(display_nii(glob.glob(os.path.join(args.output_dir, 'segmentations', 'qsm', '*.*'))[0], title='Segmentation', colorbar=True, vmin=0, vmax=+16, out_png='seg.png', cmap='tab10'))})")
    
    csv_file = glob.glob(os.path.join(args.output_dir, 'analysis', '*.*'))[0]
    add_to_github_summary(csv_to_markdown(csv_file))

