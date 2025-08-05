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

    bids_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-bids")
    shutil.copytree(bids_dir_public, bids_dir)

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir,
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
    if not github_step_summary:
        logger.log(LogLevel.WARNING.value, f"GITHUB_STEP_SUMMARY variable not found! Cannot write summary.")
    else:
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(nii_path=find_files(args.output_dir, '*_Chimap.nii*')[0], title='QSM', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")        
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(nii_path=find_files(args.output_dir, '*_dseg.nii*')[0], title='Segmentation', colorbar=True, vmin=0, vmax=+16, out_png='seg.png', cmap='tab10'))})")        

        csv_file = find_files(args.output_dir, '*analysis*.csv')[0]
        write_to_file(github_step_summary, csv_to_markdown(csv_file))

        for png_file in glob.glob(os.path.join(args.output_dir, '*.png')):
            write_to_file(github_step_summary, f"![summary]({upload_png(png_file)})")

    shutil.rmtree(args.bids_dir)

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflows, None)
])
def test_separate_qsm_seg_analysis(bids_dir_public, init_workflow, run_workflow, run_args):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING SEPARATE QSM, SEGMENTATION, AND ANALYSIS EXECUTIONS ===")

    bids_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-bids")
    shutil.copytree(bids_dir_public, bids_dir)

    args = [
        bids_dir,
        "--do_qsm",
        "--premade", "fast",
        "--use_existing_masks",
        "--auto_yes",
        "--debug",
        "--subjects", "sub-1",
        "--sessions", "ses-1"
    ]
    
    args = main(args)

    args = [
        bids_dir,
        "--do_segmentation",
        "--auto_yes",
        "--debug",
        "--subjects", "sub-1",
        "--sessions", "ses-1"
    ]

    args = main(args)

    args = [
        bids_dir,
        "--do_analysis",
        "--auto_yes",
        "--debug",
        "--subjects", "sub-1",
        "--sessions", "ses-1"
    ]

    args = main(args)

    # generate image
    github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if not github_step_summary:
        logger.log(LogLevel.WARNING.value, f"GITHUB_STEP_SUMMARY variable not found! Cannot write summary.")
    else:
        chi_files = find_files(args.output_dir, '*_Chimap.nii*')
        seg_files = find_files(args.output_dir, '*_dseg.nii*')

        for chi_file in chi_files:
            chi_png = display_nii(nii_path=chi_file, title=f'QSM ({chi_file})', colorbar=True, vmin=-0.1, vmax=+0.1, out_png=f"qsm_{os.path.split(chi_file)[1].replace('.', '_')}.png", cmap='gray')
            write_to_file(github_step_summary, f"![result]({upload_png(chi_png)})")
        for seg_file in seg_files:
            seg_png = display_nii(nii_path=seg_file, title=f'Segmentation ({seg_file})', colorbar=True, vmin=0, vmax=+16, out_png=f"seg_{os.path.split(seg_file)[1].replace('.', '_')}.png", cmap='tab10')
            write_to_file(github_step_summary, f"![result]({upload_png(seg_png)})")

        csv_files = find_files(args.output_dir, '*analysis*.csv')
        for csv_file in csv_files:
            write_to_file(github_step_summary, f'{csv_file}\n{csv_to_markdown(csv_file)}')

        for png_file in glob.glob(os.path.join(args.output_dir, '*.png')):
            write_to_file(github_step_summary, f"![summary]({upload_png(png_file)})")

    shutil.rmtree(args.bids_dir)

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflows, None)
])
def test_seg_analysis_only(bids_dir_public, init_workflow, run_workflow, run_args):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING SEGMENTATION + ANALYSIS ONLY ===")

    bids_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-bids")
    shutil.copytree(bids_dir_public, bids_dir)

    args = [
        bids_dir,
        "--do_segmentation",
        "--do_analysis",
        "--auto_yes",
        "--debug",
        "--subjects", "sub-1",
        "--sessions", "ses-1"
    ]
    
    args = main(args)

    # generate image
    github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if not github_step_summary:
        logger.log(LogLevel.WARNING.value, f"GITHUB_STEP_SUMMARY variable not found! Cannot write summary.")
    else:
        chi_files = find_files(args.output_dir, '*_Chimap.nii*')
        seg_files = find_files(args.output_dir, '*_dseg.nii*')

        for chi_file in chi_files:
            chi_png = display_nii(nii_path=chi_file, title=f'QSM ({chi_file})', colorbar=True, vmin=-0.1, vmax=+0.1, out_png=f"qsm_{os.path.split(chi_file)[1].replace('.', '_')}.png", cmap='gray')
            write_to_file(github_step_summary, f"![result]({upload_png(chi_png)})")
        for seg_file in seg_files:
            seg_png = display_nii(nii_path=seg_file, title=f'Segmentation ({seg_file})', colorbar=True, vmin=0, vmax=+16, out_png=f"seg_{os.path.split(seg_file)[1].replace('.', '_')}.png", cmap='tab10')
            write_to_file(github_step_summary, f"![result]({upload_png(seg_png)})")

        csv_files = find_files(args.output_dir, '*analysis*.csv')
        for csv_file in csv_files:
            write_to_file(github_step_summary, f'{csv_file}\n{csv_to_markdown(csv_file)}')

        for png_file in glob.glob(os.path.join(args.output_dir, '*.png')):
            write_to_file(github_step_summary, f"![summary]({upload_png(png_file)})")

    shutil.rmtree(args.bids_dir)

