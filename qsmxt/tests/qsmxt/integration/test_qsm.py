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
from qsmxt.tests.qsmxt.integration.utils import *

run_workflows = True

def check_chimap_json_sidecars(output_dir, logger):
    """
    Simple check that Chimap files exist and have JSON sidecars with QSM in ImageType.
    
    Args:
        output_dir: Directory containing QSMxT outputs
        logger: Logger instance
    """
    import json
    
    # Find the Chimap files
    chimap_files = find_files(output_dir, '*_Chimap.nii*')
    if len(chimap_files) == 0:
        logger.log(LogLevel.INFO.value, "No Chimap files found - skipping JSON sidecar check")
        return
    
    logger.log(LogLevel.INFO.value, f"Found {len(chimap_files)} Chimap file(s), checking JSON sidecars...")
    
    for chimap_file in chimap_files:
        # Check Chimap NIfTI exists
        assert os.path.exists(chimap_file), f"Chimap file not found: {chimap_file}"
        
        # Determine expected JSON sidecar path
        json_sidecar_path = chimap_file.replace('.nii.gz', '.json').replace('.nii', '.json')
        
        # Check JSON sidecar exists
        assert os.path.exists(json_sidecar_path), f"JSON sidecar not found for {chimap_file}: {json_sidecar_path}"
        
        # Check JSON contains QSM in ImageType
        with open(json_sidecar_path, 'r') as f:
            chimap_json = json.load(f)
        
        assert 'ImageType' in chimap_json, f"ImageType field missing in JSON sidecar: {json_sidecar_path}"
        assert 'QSM' in chimap_json['ImageType'], f"'QSM' not found in ImageType field: {json_sidecar_path}"
        
        logger.log(LogLevel.INFO.value, f"✓ {os.path.basename(chimap_file)} has valid JSON sidecar with QSM ImageType")
    
    logger.log(LogLevel.INFO.value, f"✓ All {len(chimap_files)} Chimap JSON sidecars validated")

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

    bids_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-bids")
    shutil.copytree(bids_dir_public, bids_dir)

    args = [
        bids_dir,
        "--premade", premade,
        "--auto_yes",
        "--debug",
        "--subjects", "sub-1",
        "--sessions", "ses-1"
    ]
    
    # create the workflow and run if necessary
    args = main(args)
    
    # Check JSON sidecars for Chimap outputs
    check_chimap_json_sidecars(args.output_dir, logger)
    
    # upload output folder
    tar_file = compress_folder(folder=args.output_dir, result_id=premade)
    sys_cmd(f"rm -rf {os.path.join(gettempdir(), 'public-outputs')}")
    os.makedirs(os.path.join(gettempdir(), "public-outputs"), exist_ok=True)
    shutil.move(tar_file, os.path.join(gettempdir(), "public-outputs", tar_file))

    # generate image
    github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if not github_step_summary:
        logger.log(LogLevel.WARNING.value, f"GITHUB_STEP_SUMMARY variable not found! Cannot write summary.")
    else:
        write_to_file(github_step_summary, f"Premade {premade}")
        chimap_files = find_files(args.output_dir, '*_Chimap.nii*')
        if chimap_files:
            write_to_file(github_step_summary, f"✓ JSON sidecars validated for {len(chimap_files)} Chimap file(s)")
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(nii_path=find_files(args.output_dir, '*_Chimap.nii*')[0], title=premade, colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")
        for png_file in glob.glob(os.path.join(args.output_dir, '*.png')):
            write_to_file(github_step_summary, f"![summary]({upload_png(png_file)})")

    shutil.rmtree(args.bids_dir)


def test_nocombine(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING NO PHASE COMBINATION ===")

    bids_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-bids")
    shutil.copytree(bids_dir_public, bids_dir)

    args = [
        bids_dir,
        "--premade", "fast",
        "--combine_phase", "off",
        "--auto_yes",
        "--debug",
        "--subjects", "sub-1",
        "--sessions", "ses-1"
    ]
    
    # create the workflow and run if necessary
    args = main(args)

    # Check JSON sidecars for Chimap outputs
    check_chimap_json_sidecars(args.output_dir, logger)

    # generate image
    github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if not github_step_summary:
        logger.log(LogLevel.WARNING.value, f"GITHUB_STEP_SUMMARY variable not found! Cannot write summary.")
    else:
        write_to_file(github_step_summary, f"test_nocombine")
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(nii_path=find_files(args.output_dir, '*_Chimap.nii*')[0], title='No combine', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")
        for png_file in glob.glob(os.path.join(args.output_dir, '*.png')):
            write_to_file(github_step_summary, f"![summary]({upload_png(png_file)})")

    shutil.rmtree(args.bids_dir)


def test_nomagnitude(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING NO MAGNITUDE ===")

    bids_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-bids")
    shutil.copytree(bids_dir_public, bids_dir)
    for mag_file in glob.glob(os.path.join(bids_dir, "sub-1", "ses-1", "anat", "*mag*")):
        os.remove(mag_file)
    
    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir,
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
    if not github_step_summary:
        logger.log(LogLevel.WARNING.value, f"GITHUB_STEP_SUMMARY variable not found! Cannot write summary.")
    else:
        write_to_file(github_step_summary, f"test_nomagnitude")
        qsm_result = find_files(os.path.join(args.output_dir), '*_Chimap.nii*')[0]
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(nii_path=qsm_result, title='No magnitude', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")
        for png_file in glob.glob(os.path.join(args.output_dir, '*.png')):
            write_to_file(github_step_summary, f"![summary]({upload_png(png_file)})")

    shutil.rmtree(args.bids_dir)

def test_inhomogeneity_correction(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING INHOMOGENEITY CORRECTION ===")

    bids_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-bids")
    shutil.copytree(bids_dir_public, bids_dir)

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir,
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
    if not github_step_summary:
        logger.log(LogLevel.WARNING.value, f"GITHUB_STEP_SUMMARY variable not found! Cannot write summary.")
    else:
        write_to_file(github_step_summary, f"test_inhomogeneity_correction")
        qsm_result = find_files(args.output_dir, '*_Chimap.nii*')[0]
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(nii_path=qsm_result, title='Inhomogeneity correction', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")
        for png_file in glob.glob(os.path.join(args.output_dir, '*.png')):
            write_to_file(github_step_summary, f"![summary]({upload_png(png_file)})")

    shutil.rmtree(args.bids_dir)

def test_hardcoded_percentile_threshold(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING HARDCODED PERCENTILE THRESHOLD ===")

    bids_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-bids")
    shutil.copytree(bids_dir_public, bids_dir)

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir,
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
    if not github_step_summary:
        logger.log(LogLevel.WARNING.value, f"GITHUB_STEP_SUMMARY variable not found! Cannot write summary.")
    else:
        write_to_file(github_step_summary, f"test_hardcoded_percentile_threshold")
        qsm_result = find_files(args.output_dir, '*_Chimap.nii*')[0]
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(nii_path=qsm_result, title='Hardcoded percentile threshold (0.25)', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")
        for png_file in glob.glob(os.path.join(args.output_dir, '*.png')):
            write_to_file(github_step_summary, f"![summary]({upload_png(png_file)})")

    shutil.rmtree(args.bids_dir)

def test_hardcoded_absolute_threshold(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING HARDCODED ABSOLUTE THRESHOLD ===")

    bids_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-bids")
    shutil.copytree(bids_dir_public, bids_dir)

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir,
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
    if not github_step_summary:
        logger.log(LogLevel.WARNING.value, f"GITHUB_STEP_SUMMARY variable not found! Cannot write summary.")
    else:
        write_to_file(github_step_summary, f"test_hardcoded_absolute_threshold")
        qsm_result = find_files(args.output_dir, '*_Chimap.nii*')[0]
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(nii_path=qsm_result, title='Hardcoded absolute threshold (15)', colorbar=True, vmin=-0.1, vmax=+0.1, out_png='qsm.png', cmap='gray'))})")
        for png_file in glob.glob(os.path.join(args.output_dir, '*.png')):
            write_to_file(github_step_summary, f"![summary]({upload_png(png_file)})")

    shutil.rmtree(args.bids_dir)

def test_use_existing_masks(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING EXISTING MASKS ===")

    bids_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-bids")
    shutil.copytree(bids_dir_public, bids_dir)
    
    args = [
        bids_dir,
        "--use_existing_masks",
        "--premade", "fast",
        "--auto_yes",
        "--debug",
        "--subjects", "sub-1",
        "--sessions", "ses-1"
    ]
    
    args = main(args)

    github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if not github_step_summary:
        logger.log(LogLevel.WARNING.value, f"GITHUB_STEP_SUMMARY variable not found! Cannot write summary.")
    else:
        write_to_file(github_step_summary, f"test_use_existing_masks")
        for png_file in glob.glob(os.path.join(args.output_dir, '*.png')):
            write_to_file(github_step_summary, f"![summary]({upload_png(png_file)})")

    shutil.rmtree(args.bids_dir)


def test_supplementary_images(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING SUPPLEMENTARY IMAGES AND DICOMS ===")

    bids_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-bids")
    shutil.copytree(bids_dir_public, bids_dir)
    
    args = [
        bids_dir,
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

    # Count DICOM files in the expected output locations
    dicom_locations = {
        'Chimap': '*_desc-dicoms_Chimap',
        'SWI': '*_desc-dicoms_swi',
        'SWI_MIP': '*_desc-dicoms_minIP'
    }
    
    dicom_counts = {}
    total_dicom_count = 0
    
    for desc, pattern in dicom_locations.items():
        dicom_dirs = glob.glob(os.path.join(args.output_dir, '**', pattern), recursive=True)
        if dicom_dirs:
            dicom_dir = dicom_dirs[0]
            dicom_files = glob.glob(os.path.join(dicom_dir, '*.dcm'))
            dicom_counts[desc] = len(dicom_files)
            total_dicom_count += len(dicom_files)
            logger.log(LogLevel.INFO.value, f"Found {len(dicom_files)} DICOM files in {desc} directory: {dicom_dir}")
        else:
            dicom_counts[desc] = 0
            logger.log(LogLevel.WARNING.value, f"No DICOM directory found for {desc} with pattern: {pattern}")

    # generate image
    github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if not github_step_summary:
        logger.log(LogLevel.WARNING.value, f"GITHUB_STEP_SUMMARY variable not found! Cannot write summary.")
    else:
        write_to_file(github_step_summary, f"test_supplementary_images")
        
        # Write DICOM count summary
        write_to_file(github_step_summary, f"\n**DICOM Export Summary:**")
        write_to_file(github_step_summary, f"- Total DICOM files created: {total_dicom_count}")
        for desc, count in dicom_counts.items():
            write_to_file(github_step_summary, f"- {desc}: {count} DICOM files")
        write_to_file(github_step_summary, f"\n")
        
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(nii_path=find_files(args.output_dir, '*_swi.nii*')[0], title='SWI', out_png='swi.png', cmap='gray'))})")
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(nii_path=find_files(args.output_dir, '*_minIP.nii*')[0], title='SWI MIP', out_png='swi_mip.png', cmap='gray'))})")
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(nii_path=find_files(args.output_dir, '*_T2starmap.nii*')[0], title='T2* map', out_png='t2s.png', cmap='gray', vmin=0, vmax=100, colorbar=True, cbar_label='T2* (ms)'))})")
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(nii_path=find_files(args.output_dir, '*_R2starmap.nii*')[0], title='R2* map', out_png='r2s.png', cmap='gray', vmin=0, vmax=100, colorbar=True, cbar_label='R2* (ms)'))})")
        for png_file in glob.glob(os.path.join(args.output_dir, '*.png')):
            write_to_file(github_step_summary, f"![summary]({upload_png(png_file)})")

    shutil.rmtree(args.bids_dir)
    

def test_realdata(bids_dir_real):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING REAL DATA ===")

    bids_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-bids-real")
    shutil.copytree(bids_dir_real, bids_dir)

    args = [
        bids_dir,
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
            write_to_file(github_step_summary, f"test_realdata")
            write_to_file(github_step_summary, "Results uploaded to RDM")
            for png_file in glob.glob(os.path.join(args.output_dir, '*.png')):
                write_to_file(github_step_summary, f"![summary]({upload_png(png_file)})")
    except:
        github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
        if not github_step_summary:
            logger.log(LogLevel.WARNING.value, f"GITHUB_STEP_SUMMARY variable not found! Cannot write summary.")
        else:
            write_to_file(github_step_summary, f"test_realdata")
            write_to_file(github_step_summary, "Result upload to RDM failed!")
            for png_file in glob.glob(os.path.join(args.output_dir, '*.png')):
                write_to_file(github_step_summary, f"![summary]({upload_png(png_file)})")

    shutil.rmtree(args.bids_dir)

def test_singleecho(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING SINGLE ECHO WITH PHASE COMBINATION / ROMEO ===")

    bids_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-bids")
    shutil.copytree(bids_dir_public, bids_dir)

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir,
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
    if not github_step_summary:
        logger.log(LogLevel.WARNING.value, f"GITHUB_STEP_SUMMARY variable not found! Cannot write summary.")
    else:
        write_to_file(github_step_summary, f"test_singleecho")
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(nii_path=find_files(args.output_dir, '*_Chimap.nii*')[0], title='Single echo', out_png='qsm.png', cmap='gray', colorbar=True, vmin=-0.1, vmax=+0.1))})")
        for png_file in glob.glob(os.path.join(args.output_dir, '*.png')):
            write_to_file(github_step_summary, f"![summary]({upload_png(png_file)})")

    shutil.rmtree(args.bids_dir)

def test_laplacian_and_tv(bids_dir_public):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"=== TESTING LAPLACIAN UNWRAPPING AND TV ALGO ===")

    bids_dir = os.path.join(gettempdir(), f"{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}-{getrunid()}-bids")
    shutil.copytree(bids_dir_public, bids_dir)

    # run pipeline and specifically choose magnitude-based masking
    args = [
        bids_dir,
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

    # Check JSON sidecars for Chimap outputs
    check_chimap_json_sidecars(args.output_dir, logger)

    # generate image
    github_step_summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if not github_step_summary:
        logger.log(LogLevel.WARNING.value, f"GITHUB_STEP_SUMMARY variable not found! Cannot write summary.")
    else:
        write_to_file(github_step_summary, f"test_laplacian_and_tv")
        write_to_file(github_step_summary, f"![result]({upload_png(display_nii(nii_path=find_files(args.output_dir, '*_Chimap.nii*')[0], title='Laplacian + TV', out_png='qsm.png', cmap='gray', colorbar=True, vmin=-0.1, vmax=+0.1))})")
        for png_file in glob.glob(os.path.join(args.output_dir, '*.png')):
            write_to_file(github_step_summary, f"![summary]({upload_png(png_file)})")

    shutil.rmtree(args.bids_dir)




