#!/usr/bin/env pytest
import os
import shutil
import pytest

import numpy as np
import qsm_forward
from qsmxt.cli.main import main

from qsmxt.scripts.logger import LogLevel, make_logger
from qsmxt.scripts.sys_cmd import sys_cmd
from qsmxt.tests.qsmxt.integration.utils import *

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

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflows, None)
])
def test_template(bids_dir_public, init_workflow, run_workflow, run_args):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING TEMPLATE PIPELINE ===")

    bids_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-bids")
    shutil.copytree(bids_dir_public, bids_dir)

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir,
        "--do_qsm", "yes",
        "--premade", "fast",
        "--do_template", "yes",
        "--auto_yes",
        "--debug",
    ]
    
    # create the workflow and run - it should fall back to phase-based masking with a warning
    args = main(args)

    # generate image - index out of range
    github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if not github_step_summary:
        logger.log(LogLevel.WARNING.value, f"GITHUB_STEP_SUMMARY variable not found! Cannot write summary.")
    else:
        
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(find_files(os.path.join(args.output_dir, 'template', 'qsm_template'), '*.nii*')[0], title='QSM template', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm_template.png', cmap='gray'))})")
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(find_files(os.path.join(args.output_dir, 'template', 'magnitude_template'), '*.nii*')[0], title='Magnitude template', out_png='mag_template.png', cmap='gray'))})")

        for png_file in glob.glob(os.path.join(args.output_dir, '*.png')):
            write_to_file(github_step_summary, f"![summary]({upload_png(png_file)})")

    shutil.rmtree(args.bids_dir)

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflows, None)
])
def test_template_existing_qsms(bids_dir_public, init_workflow, run_workflow, run_args):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING TEMPLATE PIPELINE ===")

    bids_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-bids")
    shutil.copytree(bids_dir_public, bids_dir)

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir,
        "--do_template", "yes",
        "--auto_yes",
        "--debug",
    ]
    
    # create the workflow and run - it should fall back to phase-based masking with a warning
    args = main(args)

    # generate image - index out of range
    github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if not github_step_summary:
        logger.log(LogLevel.WARNING.value, f"GITHUB_STEP_SUMMARY variable not found! Cannot write summary.")
    else:
        
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(find_files(os.path.join(args.output_dir, 'template', 'qsm_template'), '*.nii*')[0], title='QSM template', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm_template.png', cmap='gray'))})")
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(find_files(os.path.join(args.output_dir, 'template', 'magnitude_template'), '*.nii*')[0], title='Magnitude template', out_png='mag_template.png', cmap='gray'))})")

        for png_file in glob.glob(os.path.join(args.output_dir, '*.png')):
            write_to_file(github_step_summary, f"![summary]({upload_png(png_file)})")

    shutil.rmtree(args.bids_dir)

    