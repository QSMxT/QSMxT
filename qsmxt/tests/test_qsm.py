#!/usr/bin/env pytest
import os
import glob
import tempfile
import pytest
import shutil

import numpy as np
import qsm_forward
from qsmxt.cli.main import main
from qsmxt.scripts.logger import LogLevel, make_logger
from qsmxt.scripts.qsmxt_functions import get_qsm_premades
from qsmxt.scripts.sys_cmd import sys_cmd
from qsmxt.tests.utils import *

run_workflows = True

def getrunid():
    return os.environ.get('RUN_ID') or ''

def gettempdir():
    return os.environ.get('TEST_DIR') or tempfile.gettempdir()

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

@pytest.fixture
def bids_dir_real():
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== BIDS Preparation ===")
    tmp_dir = gettempdir()
    if not os.path.exists(os.path.join(tmp_dir, 'bids-secret')):
        logger.log(LogLevel.INFO.value, f"Secret BIDS directory doesn not exist!")
        if not os.path.exists(os.path.join(tmp_dir, 'bids-secret.zip')):
            logger.log(LogLevel.INFO.value, f"bids-secret.zip does not exist! Downloading...")
            download_from_osf(
                project="n6uqk",
                local_path=os.path.join(tmp_dir, "bids-secret.zip")
            )
        
        logger.log(LogLevel.INFO.value, f"Extracting bids-secret.zip...")
        sys_cmd(f"unzip {os.path.join(tmp_dir, 'bids-secret.zip')} -d {tmp_dir}")
        sys_cmd(f"rm {os.path.join(tmp_dir, 'bids-secret.zip')}")

    return os.path.join(tmp_dir, 'bids-secret')

@pytest.mark.parametrize("premade", [
    p for p in get_qsm_premades().keys() if p != 'default'
])
def test_premade(bids_dir_public, premade):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING PREMADE {premade} ===")

    out_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-qsm")

    args = [
        bids_dir_public,
        out_dir,
        "--premade", premade,
        "--auto_yes",
        "--debug",
        "--subjects", "sub-1",
        "--sessions", "ses-1"
    ]
    
    # create the workflow and run if necessary
    args = main(args)
    
    # upload output folder
    tar_file = compress_folder(folder=args.output_dir, result_id=premade)
    sys_cmd(f"rm -rf {os.path.join(gettempdir(), 'public-outputs')}")
    os.makedirs(os.path.join(gettempdir(), "public-outputs"), exist_ok=True)
    shutil.move(tar_file, os.path.join(gettempdir(), "public-outputs", tar_file))

    # generate image
    github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if github_step_summary:
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(glob.glob(os.path.join(out_dir, 'qsm', '*.*'))[0], title=premade, colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")

    shutil.rmtree(out_dir)


def test_nocombine(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING NO PHASE COMBINATION ===")

    out_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-qsm")

    args = [
        bids_dir_public,
        out_dir,
        "--premade", "fast",
        "--combine_phase", "off",
        "--auto_yes",
        "--debug",
        "--subjects", "sub-1",
        "--sessions", "ses-1"
    ]
    
    # create the workflow and run if necessary
    args = main(args)

    # generate image
    github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if github_step_summary:
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(glob.glob(os.path.join(out_dir, 'qsm', '*.*'))[0], title='No multi-echo combination', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")

    shutil.rmtree(out_dir)


def test_nomagnitude(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING NO MAGNITUDE ===")

    # create copy of bids directory
    bids_dir_nomagnitude = os.path.join(os.path.split(bids_dir_public)[0], "bids-nomagnitude")
    if os.path.exists(bids_dir_nomagnitude):
        shutil.rmtree(bids_dir_nomagnitude)
    shutil.copytree(bids_dir_public, bids_dir_nomagnitude)

    # delete magnitude files from modified directory
    for mag_file in glob.glob(os.path.join(bids_dir_nomagnitude, "sub-1", "ses-1", "anat", "*mag*")):
        os.remove(mag_file)

    out_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-qsm")
    
    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir_public,
        out_dir,
        "--premade", "fast",
        "--masking_input", "magnitude",
        "--num_echoes", "1",
        "--auto_yes",
        "--debug",
        "--subjects", "sub-1",
        "--sessions", "ses-1"
    ]
    
    # create the workflow and run - it should fall back to phase-based masking with a warning
    args = main(args)

    # generate image
    github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if github_step_summary:
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(glob.glob(os.path.join(out_dir, 'qsm', '*.*'))[0], title='No magnitude', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")

    shutil.rmtree(out_dir)

def test_inhomogeneity_correction(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING INHOMOGENEITY CORRECTION ===")

    out_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-qsm")

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir_public,
        out_dir,
        "--premade", "fast",
        "--masking_algorithm", "threshold",
        "--filling_algorithm", "bet",
        "--inhomogeneity_correction",
        "--num_echoes", "1",
        "--auto_yes",
        "--debug",
        "--subjects", "sub-1",
        "--sessions", "ses-1"
    ]
    
    # create the workflow and run
    args = main(args)

    # generate image
    github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if github_step_summary:
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(glob.glob(os.path.join(out_dir, 'qsm', '*.*'))[0], title='Inhomogeneity correction', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")

    shutil.rmtree(out_dir)

def test_hardcoded_percentile_threshold(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING HARDCODED PERCENTILE THRESHOLD ===")

    out_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-qsm")

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir_public,
        out_dir,
        "--premade", "fast",
        "--masking_algorithm", "threshold",
        "--masking_input", "magnitude",
        "--threshold_value", "0.25",
        "--num_echoes", "1",
        "--auto_yes",
        "--debug",
        "--subjects", "sub-1",
        "--sessions", "ses-1"
    ]
    
    # create the workflow and run
    args = main(args)

    # generate image
    github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if github_step_summary:
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(glob.glob(os.path.join(out_dir, 'qsm', '*.*'))[0], title='Hardcoded percentile threshold (0.25)', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")

    shutil.rmtree(out_dir)

def test_hardcoded_absolute_threshold(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING HARDCODED ABSOLUTE THRESHOLD ===")

    out_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-qsm")

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir_public,
        out_dir,
        "--premade", "fast",
        "--masking_algorithm", "threshold",
        "--masking_input", "magnitude",
        "--threshold_value", "15",
        "--num_echoes", "1",
        "--auto_yes",
        "--debug",
        "--subjects", "sub-1",
        "--sessions", "ses-1"
    ]
    
    # create the workflow and run
    args = main(args)

    # generate image
    github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if github_step_summary:
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(glob.glob(os.path.join(out_dir, 'qsm', '*.*'))[0], title='Hardcoded absolute threshold (15)', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")

    shutil.rmtree(out_dir)

def test_use_existing_masks(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING EXISTING MASKS ===")

    out_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-qsm")
    
    args = [
        bids_dir_public,
        out_dir,
        "--use_existing_masks",
        "--premade", "fast",
        "--auto_yes",
        "--debug",
        "--subjects", "sub-1",
        "--sessions", "ses-1"
    ]
    
    args = main(args)

    shutil.rmtree(out_dir)

def test_supplementary_images(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING SUPPLEMENTARY IMAGES AND DICOMS ===")

    out_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-qsm")
    
    args = [
        bids_dir_public,
        out_dir,
        "--do_qsm",
        "--do_swi",
        "--do_t2starmap",
        "--do_r2starmap",
        "--auto_yes",
        "--debug",
        "--subjects", "sub-1",
        "--sessions", "ses-1",
        "--export_dicoms"
    ]
    
    args = main(args)

    # generate image
    github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if github_step_summary:
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(glob.glob(os.path.join(out_dir, 'swi', '*swi.*'))[0], title='SWI', out_png='swi.png', cmap='gray'))})")
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(glob.glob(os.path.join(out_dir, 'swi', '*swi-mip.*'))[0], dim=2, title='SWI MIP', out_png='swi_mip.png', cmap='gray'))})")
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(glob.glob(os.path.join(out_dir, 't2s', '*.*'))[0], title='T2* map', out_png='t2s.png', cmap='gray', vmin=0, vmax=100, colorbar=True, cbar_label='T2* (ms)'))})")
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(glob.glob(os.path.join(out_dir, 'r2s', '*.*'))[0], title='R2* map', out_png='r2s.png', cmap='gray', vmin=0, vmax=100, colorbar=True, cbar_label='R2* (ms)'))})")

    shutil.rmtree(out_dir)
    

def test_realdata(bids_dir_real):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING REAL DATA ===")

    out_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-qsm")

    args = [
        bids_dir_real,
        out_dir,
        "--premade", "fast",
        "--auto_yes",
        "--debug"
    ]
    
    args = main(args)
    local_path = compress_folder(folder=args.output_dir, result_id='real')

    try:
        upload_to_rdm(
            local_path=local_path,
            remote_path=f"QSMFUNCTOR-Q0748/data/QSMxT-Test-Results/{os.path.split(local_path)[1]}"
        )
        github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
        if github_step_summary:
            write_to_file(github_step_summary, "Results uploaded to RDM")
    except:
        github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
        if github_step_summary:
            write_to_file(github_step_summary, "Result upload to RDM failed!")

    shutil.rmtree(out_dir)

def test_singleecho(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING SINGLE ECHO WITH PHASE COMBINATION / ROMEO ===")

    out_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-qsm")

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir_public,
        out_dir,
        "--unwrapping_algorithm", "romeo",
        "--num_echoes", "1",
        "--auto_yes",
        "--debug",
        "--subjects", "sub-1",
        "--sessions", "ses-1"
    ]
    
    # create the workflow and run
    args = main(args)

    # generate image
    github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if github_step_summary:
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(glob.glob(os.path.join(out_dir, 'qsm', '*.*'))[0], title='Single echo', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")

    shutil.rmtree(out_dir)

def test_laplacian_and_tv(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING LAPLACIAN UNWRAPPING AND TV ALGO ===")

    out_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-qsm")

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir_public,
        out_dir,
        "--unwrapping_algorithm", "laplacian",
        "--qsm_algorithm", "tv",
        "--combine_phase", "off",
        "--auto_yes",
        "--debug",
        "--subjects", "sub-1",
        "--sessions", "ses-1"
    ]
    
    # create the workflow and run
    args = main(args)

    # generate image
    github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if github_step_summary:
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(glob.glob(os.path.join(out_dir, 'qsm', '*.*'))[0], title='Single echo', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")

    shutil.rmtree(out_dir)


