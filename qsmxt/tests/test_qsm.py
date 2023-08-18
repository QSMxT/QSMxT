#!/usr/bin/env pytest
import os
import glob
import tempfile
import pytest
import shutil

import numpy as np
import qsm_forward
from qsmxt.cli.main import process_args, parse_args

from qsmxt.scripts.logger import LogLevel, make_logger
from qsmxt.scripts.qsmxt_functions import get_qsm_premades
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
            qsm_forward.ReconParams(voxel_size=np.array([1.0, 1.0, 1.0]), session=session, TEs=TEs, TR=TR, flip_angle=flip_angle, weighting_suffix=weighting_suffix, export_phase=export_phase)
            for (session, TEs, TR, flip_angle, weighting_suffix, export_phase) in [
                #("1", np.array([3.5e-3]), 7.5e-3, 40, "T1w", False),
                ("1", np.array([0.004, 0.012]), 0.05, 15, "T2starw", True),
            ]
        ]

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

@pytest.mark.parametrize("premade, init_workflow, run_workflow, run_args", [
    (p, True, run_workflows, None)
    for p in get_qsm_premades().keys() if p != 'default'
])
def test_premade(bids_dir_public, premade, init_workflow, run_workflow, run_args):
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
    args = workflow(args, init_workflow, run_workflow, run_args, premade, delete_workflow=True, upload_png=True)
    
    # upload output folder
    tar_file = compress_folder(folder=args.output_dir, result_id=premade)
    shutil.move(tar_file, os.path.join(tempfile.gettempdir(), "public-outputs", tar_file))
        
@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflows, { 'num_echoes' : 1 })
])
def test_nomagnitude(bids_dir_public, init_workflow, run_workflow, run_args):
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
        "--auto_yes",
        "--debug"
    ]
    
    # create the workflow and run - it should fall back to phase-based masking with a warning
    args = workflow(args, init_workflow, run_workflow, run_args, "no-magnitude", delete_workflow=True, upload_png=True)

    # delete the modified bids directory
    shutil.rmtree(bids_dir_nomagnitude)

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflows, { 'num_echoes' : 1 })
])
def test_inhomogeneity_correction(bids_dir_public, init_workflow, run_workflow, run_args):
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
        "--auto_yes",
        "--debug"
    ]
    
    # create the workflow and run
    args = workflow(args, init_workflow, run_workflow, run_args, "inhomogeneity-correction", delete_workflow=True, upload_png=True)

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflows, { 'num_echoes' : 1 })
])
def test_hardcoded_percentile_threshold(bids_dir_public, init_workflow, run_workflow, run_args):
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
        "--auto_yes",
        "--debug"
    ]
    
    # create the workflow and run
    args = workflow(args, init_workflow, run_workflow, run_args, "percentile-threshold", delete_workflow=True, upload_png=True)

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflows, { 'num_echoes' : 1 })
])
def test_hardcoded_absolute_threshold(bids_dir_public, init_workflow, run_workflow, run_args):
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
        "--auto_yes",
        "--debug"
    ]
    
    # create the workflow and run
    args = workflow(args, init_workflow, run_workflow, run_args, "absolute-threshold", delete_workflow=True, upload_png=True)

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflows, { 'num_echoes' : 1, 'bf_algorithm' : 'vsharp', 'two_pass' : False })
])
def test_use_existing_masks(bids_dir_public, init_workflow, run_workflow, run_args):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING EXISTING MASKS ===")
    os.makedirs(os.path.join(tempfile.gettempdir(), "public-outputs"), exist_ok=True)
    
    args = [
        bids_dir_public,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--use_existing_masks",
        "--auto_yes",
        "--debug"
    ]
    
    assert(args.use_existing_masks == True)
    
    args = workflow(args, init_workflow, run_workflow, run_args, "use-existing-masks", delete_workflow=True)

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflows, { 'num_echoes' : 2 })
])
def test_supplementary_images(bids_dir_public, init_workflow, run_workflow, run_args):
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
    
    assert(args.do_swi == True)
    assert(args.do_t2starmap == True)
    assert(args.do_r2starmap == True)
    
    args = workflow(args, init_workflow, run_workflow, run_args, "supplementary-images", upload_png=False)

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflows, { 'num_echoes' : 2, 'two_pass' : False, 'bf_algorithm' : 'vsharp' })
])
def test_realdata(bids_dir_real, init_workflow, run_workflow, run_args):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING REAL DATA ===")
    os.makedirs(os.path.join(tempfile.gettempdir(), "public-outputs"), exist_ok=True)

    if not bids_dir_real:
        pass
    args = [
        bids_dir_real,
        os.path.join(tempfile.gettempdir(), "qsm-secret"),
        "--auto_yes",
        "--debug"
    ]
    
    args = workflow(args, init_workflow, run_workflow, run_args, "realdata", delete_workflow=True)
    local_path = compress_folder(folder=args.output_dir, result_id='real')

    try:
        upload_to_rdm(
            local_path=local_path,
            remote_path=f"QSMFUNCTOR-Q0748/data/QSMxT-Test-Results/{os.path.split(local_path)[1]}"
        )
        add_to_github_summary("Results uploaded to RDM")
    except:
        add_to_github_summary("Result upload to RDM failed!")

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflows, { 'num_echoes' : 1 })
])
def test_singleecho(bids_dir_public, init_workflow, run_workflow, run_args):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING SINGLE ECHO WITH PHASE COMBINATION / ROMEO ===")
    os.makedirs(os.path.join(tempfile.gettempdir(), "public-outputs"), exist_ok=True)

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir_public,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--combine_phase", "on",
        "--unwrapping_algorithm", "romeo",
        "--num_echoes", "1",
        "--auto_yes",
        "--debug"
    ]
    
    # create the workflow and run
    args = workflow(args, init_workflow, run_workflow, run_args, "single-echo-with-phase-combination", delete_workflow=True, upload_png=True)

