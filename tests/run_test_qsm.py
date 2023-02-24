#!/usr/bin/env pytest
import os
import osfclient
import cloudstor
import pytest
import tempfile
import glob
import nibabel as nib
import shutil
import datetime
import numpy as np
import pandas as pd
import seaborn as sns
import run_2_qsm as qsm
import json
from scripts.qsmxt_functions import get_qsmxt_dir, get_qsmxt_version
from scripts.sys_cmd import sys_cmd
from matplotlib import pyplot as plt
from scripts.logger import LogLevel, make_logger

run_workflows = True

def create_logger(log_dir):
    os.makedirs(log_dir, exist_ok=True)
    return make_logger(
        logpath=os.path.join(log_dir, f"log_{str(datetime.datetime.now()).replace(':', '-').replace(' ', '_').replace('.', '')}.txt"),
        printlevel=LogLevel.DEBUG,
        writelevel=LogLevel.INFO,
        warnlevel=LogLevel.WARNING,
        errorlevel=LogLevel.ERROR
    )

@pytest.fixture
def bids_dir():
    tmp_dir = tempfile.gettempdir()
    if not os.path.exists(os.path.join(tmp_dir, 'bids-osf')):
        if not os.path.exists(os.path.join(tmp_dir, 'bids-osf.tar')):
            print("Downloading test data...")
            file_pointer = next(osfclient.OSF().project("9jc42").storage().files)
            file_handle = open(os.path.join(tmp_dir, 'bids-osf.tar'), 'wb')
            file_pointer.write_to(file_handle)
        print("Extracting test data...")
        sys_cmd(f"tar xf {os.path.join(tmp_dir, 'bids-osf.tar')} -C {tmp_dir}")
        sys_cmd(f"rm {os.path.join(tmp_dir, 'bids-osf.tar')}")
    return os.path.join(tmp_dir, 'bids-osf')

@pytest.fixture
def bids_dir_secret():
    tmp_dir = tempfile.gettempdir()
    if not os.path.exists(os.path.join(tmp_dir, 'bids-secret')):
        if not os.path.exists(os.path.join(tmp_dir, 'bids-secret.tar')):
            print("Downloading test data...")
            cloudstor.cloudstor(url=os.environ['DOWNLOAD_URL'], password=os.environ['DATA_PASS']).download('', os.path.join(tmp_dir, 'bids-secret.tar'))
        print("Extracting test data...")
        sys_cmd(f"tar xf {os.path.join(tmp_dir, 'bids-secret.tar')} -C {tmp_dir}")
        sys_cmd(f"rm {os.path.join(tmp_dir, 'bids-secret.tar')}")
    return os.path.join(tmp_dir, 'bids-secret')

def display_nii(
    nii_path=None, data=None, dim=0, title=None, slc=None, dpi=96, size=None, out_png=None, final_fig=True, title_fontsize=12,
    colorbar=False, cbar_label=None, cbar_orientation='vertical', cbar_nbins=None, cbar_fontsize=None, cbar_label_fontsize=8,
    **imshow_args
):
    data = data if data is not None else nib.load(nii_path).get_fdata()
    slc = slc or int(data.shape[0]/2)
    if dim == 0: slc_data = data[slc,:,:]
    if dim == 1: slc_data = data[:,slc,:]
    if dim == 2: slc_data = data[:,:,slc]
    if size:
        plt.figure(figsize=(size[0]/dpi, size[1]/dpi), dpi=dpi)
    else:
        plt.figure(dpi=dpi)
    plt.axis('off')
    plt.imshow(np.rot90(slc_data), **imshow_args)
    if colorbar:
        cbar = plt.colorbar(orientation=cbar_orientation, fraction=0.037, pad=0.04)
        if cbar_fontsize:
            cbar.ax.tick_params(labelsize=cbar_fontsize)
        if cbar_nbins:
            cbar.ax.locator_params(nbins=cbar_nbins)
        if cbar_label:
            if cbar_orientation == 'horizontal':
                cbar.ax.set_xlabel(cbar_label, fontsize=cbar_label_fontsize)
            else:
                cbar.ax.set_ylabel(cbar_label, fontsize=cbar_label_fontsize, rotation=90)
    if title:
        plt.title(title, fontsize=title_fontsize)
    if final_fig:
        if out_png:
            plt.savefig(out_png, bbox_inches='tight')
        else:
            plt.show()

def print_metrics(name, bids_path, qsm_path):
    qsm_file = glob.glob(os.path.join(qsm_path, "qsm_final", "*qsm*nii*"))[0]
    seg_file = glob.glob(os.path.join(bids_path, "sub-1", "ses-1", "extra_data", "*segmentation*nii*"))[0]
    chi_file = glob.glob(os.path.join(bids_path, "sub-1", "ses-1", "extra_data", "*chi*crop*nii*"))[0]
    mask_file = glob.glob(os.path.join(bids_path, "sub-1", "ses-1", "extra_data", "*brainmask*nii*"))[0]

    qsm = nib.load(qsm_file).get_fdata()
    seg = nib.load(seg_file).get_fdata()
    chi = nib.load(chi_file).get_fdata()
    mask = nib.load(mask_file).get_fdata()
    seg *= mask
    chi *= mask

    labels = { 
        1 : "Caudate",
        2 : "Globus pallidus",
        3 : "Putamen",
        4 : "Red nucleus",
        5 : "Dentate nucleus",
        6 : "SN and STN",
        7 : "Thalamus",
        8 : "White matter",
        9 : "Gray matter",
        10 : "CSF",
        11 : "Blood",
        12 : "Fat",
        13 : "Bone",
        14 : "Air",
        15 : "Muscle",
        16 : "Calcification"
    }

    columns = ["Label", "RMSE"]

    # whole brain
    qsm_values = qsm[mask == 1].flatten()
    chi_values = chi[mask == 1].flatten()
    rmse_column = np.sqrt(np.square(qsm_values - chi_values)).reshape(-1,1)
    labels_column = np.full(rmse_column.shape, "Whole brain")
    new_vals = np.append(labels_column, rmse_column, axis=1)
    metrics_np = np.array(new_vals)

    # other areas
    for label_num in labels.keys():
        qsm_values = qsm[seg == label_num].flatten()
        chi_values = chi[seg == label_num].flatten()
        rmse_column = np.sqrt(np.square(qsm_values - chi_values)).reshape(-1,1)
        labels_column = np.full(rmse_column.shape, labels[label_num])
        new_vals = np.append(labels_column, rmse_column, axis=1)
        metrics_np = np.append(metrics_np, new_vals, axis=0)

    metrics = pd.DataFrame(data=metrics_np, columns=columns)
    metrics['RMSE'] = metrics['RMSE'].astype(float)
    plt.figure(figsize=(15, 8), dpi=200)
    ax = sns.boxplot(data=metrics, x="Label", y="RMSE", color="seagreen")
    ax.set_xticklabels(ax.get_xticklabels(), rotation=45, ha="right")
    plt.tight_layout()
    plt.savefig(os.path.join(qsm_path, "qsm_final", f"{name}_metrics.png"))
    plt.close()

    display_nii(data=qsm, dim=0, cmap='gray', vmin=-0.1, vmax=+0.1, colorbar=True, cbar_label='ppm', cbar_orientation='horizontal', cbar_nbins=3, out_png=os.path.join(qsm_path, "qsm_final", os.path.join(qsm_path, "qsm_final", f"{name}_slice.png")))

def workflow(args, init_workflow, run_workflow, run_args, delete_workflow=True):
    assert(not (run_workflow == True and init_workflow == False))
    run_workflow &= not (args.qsm_algorithm == 'nextqsm' != (run_args.get('qsm_algorithm') == 'nextqsm'))
    shutil.rmtree(os.path.join(args.output_dir), ignore_errors=True)
    logger = create_logger(args.output_dir)
    logger.log(LogLevel.DEBUG.value, f"WORKFLOW DETAILS: {args}")
    if init_workflow:
        logger.log(LogLevel.DEBUG.value, f"Initialising workflow...")
        wf = qsm.init_workflow(args)
    if init_workflow and run_workflow:
        qsm.set_env_variables(args)
        if run_args:
            logger.log(LogLevel.DEBUG.value, f"Updating args with run_args: {run_args}")
            arg_dict = vars(args)
            for key, value in run_args.items():
                arg_dict[key] = value
            logger.log(LogLevel.DEBUG.value, f"Initialising workflow with updated args...")
            wf = qsm.init_workflow(args)
        logger.log(LogLevel.DEBUG.value, f"Saving args to {os.path.join(args.output_dir, 'args.txt')}...")
        args_file = open(os.path.join(args.output_dir, "args.txt"), 'w')
        args_file.write(str(args))
        args_file.close()
        logger.log(LogLevel.DEBUG.value, f"Running workflow!")
        wf.run(plugin='MultiProc', plugin_args={'n_procs': args.n_procs})
        if delete_workflow:
            logger.log(LogLevel.DEBUG.value, f"Deleting workflow folder {os.path.join(args.output_dir, 'workflow_qsm')}")
            shutil.rmtree(os.path.join(args.output_dir, "workflow_qsm"), ignore_errors=True)


def compress_folder(folder, result_id):
    if os.environ.get('BRANCH'):
        results_tar = f"{str(datetime.datetime.now()).replace(':', '-').replace(' ', '_').replace('.', '')}_{os.environ['BRANCH']}_{result_id}.tar"
    else:
        results_tar = f"{str(datetime.datetime.now()).replace(':', '-').replace(' ', '_').replace('.', '')}_{result_id}.tar"
    
    sys_cmd(f"tar -cf {results_tar} {folder}")

    return results_tar


def upload_file(fname):
    try:
        cs = cloudstor.cloudstor(url=os.environ['UPLOAD_URL'], password=os.environ['DATA_PASS'])
        cs.upload(fname, os.path.split(fname)[1])
    except KeyError:
        print("No UPLOAD_URL/DATA_PASS variable found... Skipping upload")

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflows, { 'num_echoes' : 1 })
])
def test_premades(bids_dir, init_workflow, run_workflow, run_args):
    pipeline_file = f"{os.path.join(get_qsmxt_dir(), 'qsm_pipelines.json')}"
    with open(pipeline_file, "r") as json_file:
        premades = json.load(json_file)
    os.makedirs(os.path.join(tempfile.gettempdir(), "public-outputs"), exist_ok=True)

    for premade in premades.keys():
        if premade == 'default': continue
        print(f"=== TESTING PREMADE {premade} ===")

        args = qsm.process_args(qsm.parse_args([
            bids_dir,
            os.path.join(tempfile.gettempdir(), "qsm"),
            "--premade", premade,
            "--auto_yes"
        ]))

        # ensure the args match the appropriate premade
        premade_args = premades[premade]
        args_dict = vars(args)
        for key in premade_args.keys():
            if key not in ['description']:
                assert(premade_args[key] == args_dict[key])
        
        # create the workflow and run if necessary
        workflow(args, init_workflow, run_workflow, run_args, delete_workflow=False)

        # visualise results
        for nii_file in glob.glob(os.path.join(args.output_dir, "qsm_final", "*", "*.nii*")):
            display_nii(
                nii_path=nii_file,
                title=f"QSM using premade pipeline: {premade}\n({get_qsmxt_version()})",
                colorbar=True,
                cbar_label="Susceptibility (ppm)",
                out_png=os.path.join(tempfile.gettempdir(), "public-outputs", f"{premade}.png"),
                cmap='gray',
                vmin=-0.15,
                vmax=+0.15
            )
        
        # upload output folder
        tar_file = compress_folder(folder=args.output_dir, result_id=premade)
        shutil.move(tar_file, os.path.join(tempfile.gettempdir(), "public-outputs", tar_file))
        
@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflows, { 'num_echoes' : 2, 'two_pass' : False, 'bf_algorithm' : 'vsharp' })
])
def test_realdata(bids_dir_secret, init_workflow, run_workflow, run_args):
    if not bids_dir_secret:
        pass
    args = qsm.process_args(qsm.parse_args([
        bids_dir_secret,
        os.path.join(tempfile.gettempdir(), "qsm-secret"),
        "--auto_yes"
    ]))
    
    workflow(args, init_workflow, run_workflow, run_args)
    upload_file(compress_folder(folder=args.output_dir, result_id='real'))

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflows, { 'num_echoes' : 1, 'bf_algorithm' : 'vsharp', 'two_pass' : False })
])
def test_use_existing_masks(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--use_existing_masks",
        "--auto_yes"
    ]))
    
    assert(args.use_existing_masks == True)
    
    workflow(args, init_workflow, run_workflow, run_args)

# TODO
#  - check file outputs
#  - test axial resampling / obliquity
#  - test for errors that may occur within a run, including:
#    - no phase files present
#    - number of json files different from number of phase files
#    - no magnitude files present - default to phase-based masking
#    - use_existing_masks specified but none found - default to masking method
#    - use_existing_masks specified but number of masks > 1 and mismatches # of echoes 
#    - use_existing_masks specified and masks found:
#      - inhomogeneity_correction, two_pass, and add_bet should all disable
