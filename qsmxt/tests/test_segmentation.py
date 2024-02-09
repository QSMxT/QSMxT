#!/usr/bin/env pytest
import os
import shutil
import pytest
import csv

import numpy as np
import qsm_forward
from qsmxt.cli.main import main

from qsmxt.scripts.logger import LogLevel, make_logger
from qsmxt.scripts.sys_cmd import sys_cmd
from qsmxt.tests.utils import *

run_workflows = True

def gettempdir():
    #return tempfile.gettempdir()
    return "/storage/tmp"

def getrunid():
    return os.environ.get('RUN_ID') or ''

@pytest.fixture
def bids_dir_public():
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== BIDS Preparation ===")

    tmp_dir = gettempdir()
    bids_dir = os.path.join(tmp_dir, "bids")
    if not glob.glob(os.path.join(bids_dir, "sub*1")):
        logger.log(LogLevel.INFO.value, f"No subjects in BIDS directory yet")
        head_phantom_maps_dir = os.path.join(tmp_dir, 'data')
        if not os.path.exists(head_phantom_maps_dir):
            logger.log(LogLevel.INFO.value, f"Head phantom maps directory does not exist yet")
            if not os.path.exists(os.path.join(tmp_dir, 'head-phantom-maps.tar')):
                logger.log(LogLevel.INFO.value, f"Head phantom maps tar file does not exist - downloading...")
                download_from_osf(
                    project="9jc42",
                    local_path=os.path.join(tmp_dir, "head-phantom-maps.tar")
                )
            logger.log(LogLevel.INFO.value, f"Extracting then deleting head-phantom-maps.tar...")
            sys_cmd(f"tar xf {os.path.join(tmp_dir, 'head-phantom-maps.tar')} -C {tmp_dir}")

        logger.log(LogLevel.INFO.value, "Preparing simulation information...")
        tissue_params = qsm_forward.TissueParams(os.path.join(tmp_dir, 'data'))
        recon_params_all = [
            qsm_forward.ReconParams(voxel_size=np.array([1.0, 1.0, 1.0]), session=session, TEs=TEs, TR=TR, flip_angle=flip_angle, suffix=suffix, save_phase=save_phase)
            for (session, TEs, TR, flip_angle, suffix, save_phase) in [
                ("1", np.array([3.5e-3]), 7.5e-3, 40, "T1w", False),
                ("1", np.array([0.012, 0.020]), 0.05, 15, "T2starw", True),
                ("2", np.array([0.012, 0.020]), 0.05, 15, "T2starw", True)
            ]
        ]

        logger.log(LogLevel.INFO.value, "Generating BIDS dataset...")
        for recon_params in recon_params_all:
            qsm_forward.generate_bids(tissue_params=tissue_params, recon_params=recon_params, bids_dir=bids_dir)

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

    out_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-qsm")

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir_public,
        out_dir,
        "--do_qsm",
        "--premade", "fast",
        "--do_segmentation",
        "--do_analysis",
        "--auto_yes",
        "--debug",
        "--subjects", "sub-1",
        "--sessions", "ses-1"
    ]

    if not run_workflows: args += ['--dry']
    
    # create the workflow and run - it should fall back to phase-based masking with a warning
    args = main(args)

    # generate image
    github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if github_step_summary:
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(glob.glob(os.path.join(out_dir, 'qsm', '*.*'))[0], title='QSM', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(glob.glob(os.path.join(out_dir, 'segmentations', 'qsm', '*.*'))[0], title='Segmentation', colorbar=True, vmin=0, vmax=+16, out_png='seg.png', cmap='tab10'))})")

        csv_file = glob.glob(os.path.join(out_dir, 'analysis', '*.*'))[0]
        write_to_file(github_step_summary, csv_to_markdown(csv_file))

    shutil.rmtree(out_dir)

