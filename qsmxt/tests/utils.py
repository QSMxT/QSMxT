import os
import datetime
import signal
from random import randint
import tempfile
import glob
import shutil
from time import sleep

import osfclient
import webdav3.client
import nibabel as nib
import numpy as np

from matplotlib import pyplot as plt

from qsmxt.cli.main import *
from qsmxt.scripts.sys_cmd import sys_cmd
from qsmxt.scripts.logger import LogLevel, make_logger
from qsmxt.scripts.qsmxt_functions import get_qsmxt_version, extend_fname

def create_logger(log_dir):
    os.makedirs(log_dir, exist_ok=True)
    return make_logger(
        logpath=os.path.join(log_dir, f"log_{str(datetime.datetime.now()).replace(':', '-').replace(' ', '_').replace('.', '')}.txt"),
        printlevel=LogLevel.DEBUG,
        writelevel=LogLevel.INFO,
        warnlevel=LogLevel.WARNING,
        errorlevel=LogLevel.ERROR
    )

def add_to_github_summary(markdown):
    tmp_dir = tempfile.gettempdir()
    with open(os.path.join(tmp_dir, 'GITHUB_STEP_SUMMARY.md'), 'a') as github_step_summary_file:
        github_step_summary_file.write(markdown + "\n")

def upload_png(png_path):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"Uploading png {png_path}")
    png_url = sys_cmd(f"images-upload-cli --no-clipboard --hosting freeimage {png_path}").strip()
    return png_url

def webdav_connect():
    logger = make_logger()
    logger.log(LogLevel.INFO.value, "Retrieving WEBDAV details...")
    try:
        webdav_login = os.environ['WEBDAV_LOGIN']
        webdav_password = os.environ['WEBDAV_PASSWORD']
    except KeyError as e:
        logger.log(LogLevel.ERROR.value, f"Could not connect to WEBDAV - missing WEBDAV_LOGIN and/or WEBDAV_PASSWORD")
        raise e

    logger.log(LogLevel.INFO.value, f"Establishing WEBDAV connection...")
    try:
        client = webdav3.client.Client({
            'webdav_hostname': f"https://cloud.rdm.uq.edu.au/remote.php/dav/files/{webdav_login}/",
            'webdav_login':    webdav_login,
            'webdav_password': webdav_password,
            'webdav_timeout': 120
        })
    except Exception as e:
        logger.log(LogLevel.ERROR.value, f"Could not connect to WEBDAV - connection error!")
        raise e

    logger.log(LogLevel.INFO.value, "Connection successful!")
    return client

def upload_to_rdm(local_path, remote_path):
    logger = make_logger()
    client = webdav_connect()
    exception = None

    # Signal handler for the alarm
    def handler(signum, frame):
        raise TimeoutError("Upload took too long!")

    # Set the signal handler for the alarm signal
    signal.signal(signal.SIGALRM, handler)

    for i in range(5):
        logger.log(LogLevel.INFO.value, f"Uploading {local_path} to {remote_path}")
        try:
            # Set an alarm for 10 minutes
            signal.alarm(10 * 60)

            client.upload_sync(
                remote_path=remote_path,
                local_path=local_path
            )
            
            # Cancel the alarm if the upload is successful
            signal.alarm(0)

        except webdav3.exceptions.ConnectionException as e:
            logger.log(LogLevel.WARNING.value, f"Connection failed! {e}")
            sleeptime = randint(120, 300)
            logger.log(LogLevel.INFO.value, f"Sleeping for {sleeptime} seconds...")
            sleep(sleeptime)
            exception = e
            continue
        except TimeoutError as e:
            logger.log(LogLevel.WARNING.value, f"{e}")
            return
        break
    else:
        logger.log(LogLevel.ERROR.value, f"Upload failed after 5 attempts!")
        raise exception


def download_from_rdm(remote_path, local_path):
    logger = make_logger()
    client = webdav_connect()
    exception = None
    for i in range(5):
        logger.log(LogLevel.INFO.value, f"Downloading {remote_path} to {local_path}")
        try:
            client.download_sync(
                remote_path=remote_path,
                local_path=local_path,
            )
        except webdav3.exceptions.ConnectionException as e:
            logger.log(LogLevel.WARNING.value, f"Connection failed! {e}")
            sleeptime = randint(120, 300)
            logger.log(LogLevel.INFO.value, f"Sleeping for {sleeptime} seconds...")
            sleep(sleeptime)
            exception = e
            continue
        break
    else:
        logger.log(LogLevel.ERROR.value, f"Download failed after 5 attempts!")
        raise exception

def download_from_osf(project, local_path):
    logger = make_logger()
    try:
        osf_token = os.environ['OSF_TOKEN']
        osf_user = os.environ['OSF_USER']
        osf_pass = os.environ['OSF_PASS']
    except KeyError as e:
        logger.log(LogLevel.ERROR.value, f"Cannot connect to OSF - missing environment variable/s! Need OSF_TOKEN, OSF_USER and OSF_PASS.")
        raise e
    
    if any(len(x) == 0 for x in [osf_token, osf_user, osf_pass]):
        if len(osf_token) == 0:
            logger.log(LogLevel.ERROR.value, f"OSF_TOKEN length is zero")
        if len(osf_user) == 0:
            logger.log(LogLevel.ERROR.value, f"OSF_USER length is zero")
        if len(osf_pass) == 0:
            logger.log(LogLevel.ERROR.value, f"OSF_PASS length is zero")
        raise ValueError("OSF_TOKEN, OSF_USER and/or OSF_PASS not initialised properly!")
    
    logger.log(LogLevel.INFO.value, "Connecting to OSF...")
    osf = osfclient.OSF(username=osf_user, password=osf_pass, token=osf_token)
    osf_project = osf.project(project)
    osf_file = list(osf_project.storage().files)[0]

    logger.log(LogLevel.INFO.value, f"Downloading from {project} to {local_path}")
    with open(local_path, 'wb') as fpr:
        osf_file.write_to(fpr)

def compress_folder(folder, result_id):
    logger = make_logger()
    if os.environ.get('BRANCH'):
        results_tar = f"{str(datetime.datetime.now()).replace(':', '-').replace(' ', '_').replace('.', '')}_{os.environ['BRANCH']}_{result_id}.tar"
    else:
        results_tar = f"{str(datetime.datetime.now()).replace(':', '-').replace(' ', '_').replace('.', '')}_{result_id}.tar"

    logger.log(LogLevel.INFO.value, f"Compressing folder {folder} with suffix '{result_id}'")
    sys_cmd(f"tar -cf {results_tar} {folder}")

    return results_tar

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
            return out_png
        else:
            plt.show()

