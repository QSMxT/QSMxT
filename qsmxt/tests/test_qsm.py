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

@pytest.fixture
def bids_dir_public():
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== Generating BIDS dataset ===")

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
            logger.log(LogLevel.INFO.value, f"Extracting then deleting head-phantom-maps.tar...")
            sys_cmd(f"tar xf {os.path.join(tmp_dir, 'head-phantom-maps.tar')} -C {tmp_dir}")
            sys_cmd(f"rm {os.path.join(tmp_dir, 'head-phantom-maps.tar')}")

        logger.log(LogLevel.INFO.value, "Preparing simulation information...")
        tissue_params = qsm_forward.TissueParams(os.path.join(tmp_dir, 'head-phantom-maps'))
        recon_params_all = [
            qsm_forward.ReconParams(voxel_size=np.array([1.0, 1.0, 1.0]), session=session, TEs=TEs, TR=TR, flip_angle=flip_angle, suffix=suffix, export_phase=export_phase)
            for (session, TEs, TR, flip_angle, suffix, export_phase) in [
                #("1", np.array([3.5e-3]), 7.5e-3, 40, "T1w", False),
                ("1", np.array([0.012, 0.020]), 0.05, 15, "T2starw", True),
            ]
        ]

        logger.log(LogLevel.INFO.value, "Generating BIDS dataset...")
        bids_dir = os.path.join(tmp_dir, "bids-public")
        for recon_params in recon_params_all:
            qsm_forward.generate_bids(tissue_params=tissue_params, recon_params=recon_params, bids_dir=bids_dir)
        sys_cmd(f"rm {head_phantom_maps_dir}")

    return bids_dir

@pytest.fixture
def bids_dir_real():
    tmp_dir = tempfile.gettempdir()
    if not os.path.exists(os.path.join(tmp_dir, 'bids-secret')):
        if not os.path.exists(os.path.join(tmp_dir, 'bids-secret.tar')):
            download_from_osf(
                project="n6uqk",
                local_path=os.path.join(tmp_dir, "bids-secret.zip")
            )

        sys_cmd(f"unzip {os.path.join(tmp_dir, 'bids-secret.zip')} -d {tmp_dir}")
        sys_cmd(f"rm {os.path.join(tmp_dir, 'bids-secret.zip')}")

    return os.path.join(tmp_dir, 'bids-secret')

@pytest.mark.parametrize("premade", [
    p for p in get_qsm_premades().keys() if p != 'default'
])
def test_premade(bids_dir_public, premade):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING PREMADE {premade} ===")

    premades = get_qsm_premades()
    os.makedirs(os.path.join(tempfile.gettempdir(), "public-outputs"), exist_ok=True)

    args = [
        bids_dir_public,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--premade", premade,
        "--auto_yes",
        "--debug"
    ]
    
    # create the workflow and run if necessary
    args = main(args)
    
    # upload output folder
    tar_file = compress_folder(folder=args.output_dir, result_id=premade)
    shutil.move(tar_file, os.path.join(tempfile.gettempdir(), "public-outputs", tar_file))

    # generate image
    add_to_github_summary(f"![result]({upload_png(display_nii(glob.glob(os.path.join(tempfile.gettempdir(), 'qsm', 'qsm', '*.*'))[0], title=premade, colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")

def test_nocombine(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING NO PHASE COMBINATION ===")

    os.makedirs(os.path.join(tempfile.gettempdir(), "public-outputs"), exist_ok=True)

    args = [
        bids_dir_public,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--premade", "fast",
        "--combine_phase", "off",
        "--auto_yes",
        "--debug"
    ]
    
    # create the workflow and run if necessary
    args = main(args)

    # generate image
    add_to_github_summary(f"![result]({upload_png(display_nii(glob.glob(os.path.join(tempfile.gettempdir(), 'qsm', 'qsm', '*.*'))[0], title='No multi-echo combination', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")


def test_nomagnitude(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING NO MAGNITUDE ===")
    os.makedirs(os.path.join(tempfile.gettempdir(), "public-outputs"), exist_ok=True)

    # create copy of bids directory
    bids_dir_nomagnitude = os.path.join(os.path.split(bids_dir_public)[0], "bids-nomagnitude")
    shutil.copytree(bids_dir_public, bids_dir_nomagnitude)

    # delete magnitude files from modified directory
    for mag_file in glob.glob(os.path.join(bids_dir_nomagnitude, "sub-1", "ses-1", "anat", "*mag*")):
        os.remove(mag_file)
    
    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir_public,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--premade", "fast",
        "--masking_input", "magnitude",
        "--num_echoes", "1",
        "--auto_yes",
        "--debug"
    ]
    
    # create the workflow and run - it should fall back to phase-based masking with a warning
    args = main(args)

    # delete the modified bids directory
    shutil.rmtree(bids_dir_nomagnitude)

    # generate image
    add_to_github_summary(f"![result]({upload_png(display_nii(glob.glob(os.path.join(tempfile.gettempdir(), 'qsm', 'qsm', '*.*'))[0], title='No magnitude', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")

def test_inhomogeneity_correction(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING INHOMOGENEITY CORRECTION ===")
    os.makedirs(os.path.join(tempfile.gettempdir(), "public-outputs"), exist_ok=True)

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir_public,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--premade", "fast",
        "--masking_algorithm", "threshold",
        "--filling_algorithm", "bet",
        "--inhomogeneity_correction",
        "--num_echoes", "1",
        "--auto_yes",
        "--debug"
    ]
    
    # create the workflow and run
    args = main(args)

    # generate image
    add_to_github_summary(f"![result]({upload_png(display_nii(glob.glob(os.path.join(tempfile.gettempdir(), 'qsm', 'qsm', '*.*'))[0], title='Inhomogeneity correction', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")

def test_hardcoded_percentile_threshold(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING HARDCODED PERCENTILE THRESHOLD ===")
    os.makedirs(os.path.join(tempfile.gettempdir(), "public-outputs"), exist_ok=True)

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir_public,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--premade", "fast",
        "--masking_algorithm", "threshold",
        "--masking_input", "magnitude",
        "--threshold_value", "0.25",
        "--num_echoes", "1",
        "--auto_yes",
        "--debug",
    ]
    
    # create the workflow and run
    args = main(args)

    # generate image
    add_to_github_summary(f"![result]({upload_png(display_nii(glob.glob(os.path.join(tempfile.gettempdir(), 'qsm', 'qsm', '*.*'))[0], title='Hardcoded percentile threshold (0.25)', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")

def test_hardcoded_absolute_threshold(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING HARDCODED ABSOLUTE THRESHOLD ===")
    os.makedirs(os.path.join(tempfile.gettempdir(), "public-outputs"), exist_ok=True)

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir_public,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--premade", "fast",
        "--masking_algorithm", "threshold",
        "--masking_input", "magnitude",
        "--threshold_value", "15",
        "--num_echoes", "1",
        "--auto_yes",
        "--debug"
    ]
    
    # create the workflow and run
    args = main(args)

    # generate image
    add_to_github_summary(f"![result]({upload_png(display_nii(glob.glob(os.path.join(tempfile.gettempdir(), 'qsm', 'qsm', '*.*'))[0], title='Hardcoded absolute threshold (15)', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")

def test_use_existing_masks(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING EXISTING MASKS ===")
    os.makedirs(os.path.join(tempfile.gettempdir(), "public-outputs"), exist_ok=True)
    
    args = [
        bids_dir_public,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--use_existing_masks",
        "--premade", "fast",
        "--auto_yes",
        "--debug"
    ]
    
    args = main(args)

def test_supplementary_images(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING SUPPLEMENTARY IMAGES ===")
    os.makedirs(os.path.join(tempfile.gettempdir(), "public-outputs"), exist_ok=True)
    
    args = [
        bids_dir_public,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--do_swi",
        "--do_t2starmap",
        "--do_r2starmap",
        "--auto_yes",
        "--debug"
    ]
    
    args = main(args)

    # generate image
    add_to_github_summary(f"![result]({upload_png(display_nii(glob.glob(os.path.join(tempfile.gettempdir(), 'qsm', 'swi', '*swi.*'))[0], title='SWI', out_png='swi.png', cmap='gray'))})")
    add_to_github_summary(f"![result]({upload_png(display_nii(glob.glob(os.path.join(tempfile.gettempdir(), 'qsm', 'swi', '*swi-mip.*'))[0], dim=2, title='SWI MIP', out_png='swi_mip.png', cmap='gray'))})")
    add_to_github_summary(f"![result]({upload_png(display_nii(glob.glob(os.path.join(tempfile.gettempdir(), 'qsm', 't2s', '*.*'))[0], title='T2* map', out_png='t2s.png', cmap='gray', vmin=0, vmax=100, colorbar=True, cbar_label='T2* (ms)'))})")
    add_to_github_summary(f"![result]({upload_png(display_nii(glob.glob(os.path.join(tempfile.gettempdir(), 'qsm', 'r2s', '*.*'))[0], title='R2* map', out_png='r2s.png', cmap='gray', vmin=0, vmax=100, colorbar=True, cbar_label='R2* (ms)'))})")
    

def test_realdata(bids_dir_real):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING REAL DATA ===")
    os.makedirs(os.path.join(tempfile.gettempdir(), "public-outputs"), exist_ok=True)

    args = [
        bids_dir_real,
        os.path.join(tempfile.gettempdir(), "qsm-secret"),
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
        add_to_github_summary("Results uploaded to RDM")
    except:
        add_to_github_summary("Result upload to RDM failed!")

def test_singleecho(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING SINGLE ECHO WITH PHASE COMBINATION / ROMEO ===")
    os.makedirs(os.path.join(tempfile.gettempdir(), "public-outputs"), exist_ok=True)

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir_public,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--unwrapping_algorithm", "romeo",
        "--num_echoes", "1",
        "--auto_yes",
        "--debug"
    ]
    
    # create the workflow and run
    args = main(args)

    # generate image
    add_to_github_summary(f"![result]({upload_png(display_nii(glob.glob(os.path.join(tempfile.gettempdir(), 'qsm', 'qsm', '*.*'))[0], title='Single echo', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")


