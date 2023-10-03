#!/usr/bin/env python3

import argparse
import os
import sys
import subprocess
import glob
import json
import fnmatch
import datetime
import re

from qsmxt.scripts.qsmxt_functions import get_qsmxt_version, get_diff
from qsmxt.scripts.logger import LogLevel, make_logger, show_warning_summary 
from qsmxt.scripts.nii_fix_ge import fix_ge_polar, fix_ge_complex

def sys_cmd(cmd):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"Running command: '{cmd}'")
        
    process = subprocess.run(cmd, shell=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    stdout_byte = process.stdout
    stderr_byte = process.stderr

    stdout_str = stdout_byte.decode('UTF-8')
    stderr_str = str(stderr_byte.decode('UTF-8'))
    return_code = process.returncode
    
    if stdout_str:
        logger.log(LogLevel.DEBUG.value, f"Command output: '{stdout_str}'", end="")

    if return_code:
        logger.log(LogLevel.WARNING.value, f"Command '{cmd}' returned error {return_code}: '{stderr_str}'")
    
    return return_code

def load_json(path):
    with open(path, encoding='utf-8') as f:
        j = json.load(f)
    return j

def rename(old, new, always_show=False):
    if always_show or not sys.__stdin__.isatty():
        logger = make_logger()
        logger.log(LogLevel.INFO.value, f'Renaming {old} -> {new}')
    os.makedirs(os.path.split(new)[0], exist_ok=True)
    os.rename(old, new)

def clean(data): 
    return re.sub(r'[^a-zA-Z0-9]', '', data).lower()

def get_folders_in(folder, full_path=False):
    folders = list(filter(os.path.isdir, [os.path.join(folder, d) for d in os.listdir(folder)]))
    if full_path: return folders
    folders = [os.path.split(folder)[1] for folder in folders]
    return folders

def convert_to_nifti(input_dir, output_dir, qsm_protocol_patterns, t1w_protocol_patterns, auto_yes):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, 'Converting all DICOMs to NIfTI...')
    subjects = get_folders_in(input_dir)

    for subject in subjects:
        sessions = get_folders_in(os.path.join(input_dir, subject))
        for session in sessions:
            session_extra_folder = os.path.join(output_dir, f"sub-{clean(subject)}", f"ses-{clean(session)}", "extra_data")
            os.makedirs(session_extra_folder, exist_ok=True)
            if 'dcm2niix_output.txt' in os.listdir(session_extra_folder):
                logger.log(LogLevel.WARNING.value, f'{session_extra_folder} already has dcm2niix conversion output! Skipping...')
                continue
            series = get_folders_in(os.path.join(input_dir, subject, session))
            for s in series:
                series_dicom_folder = os.path.join(input_dir, subject, session, s)
                sys_cmd(f"dcm2niix -z n -o \"{session_extra_folder}\" \"{series_dicom_folder}\" >> \"{os.path.join(session_extra_folder, 'dcm2niix_output.txt')}\"")
    
    logger.log(LogLevel.INFO.value, f"Loading JSON headers from '{output_dir}/.../extra_data' folders...")
    subjects = get_folders_in(output_dir)
    json_files = []
    json_datas = []
    for subject in subjects:
        sessions = get_folders_in(os.path.join(output_dir, subject))
        for session in sessions:
            session_extra_folder = os.path.join(output_dir, subject, session, "extra_data")
            json_files.extend(sorted(glob.glob(os.path.join(session_extra_folder, "*json"))))
            json_datas.extend([load_json(json_file) for json_file in sorted(glob.glob(os.path.join(session_extra_folder, "*json")))])

    logger.log(LogLevel.INFO.value, f"Checking for GE data requiring correction...")
    ge_corrections = False
    for i in range(len(json_datas)):
        if any([x in json_files[i] for x in ['_ph.json', '_real.json']]):
            if "Manufacturer" not in json_datas[i]:
                logger.log(LogLevel.WARNING.value, f"'Manufacturer' missing from JSON header '{json_files[i]}'. Unable to determine whether any GE data requires correction. You may need to manually run nii-fix-ge.py.")
                continue
            ge_corrections = True
            if json_datas[i]["Manufacturer"].upper().strip() in ["GE", "GE MEDICAL SYSTEMS"]:
                if '_ph.json' in json_files[i]:
                    phase_path = glob.glob(json_files[i].replace('.json', '.nii*'))[0]
                    mag_path = glob.glob(json_files[i].replace('_ph.json', '.nii*'))[0]
                    logger.log(LogLevel.INFO.value, f"Correcting GE data: phase={phase_path}; mag={mag_path}")
                    fix_ge_polar(mag_path, phase_path, delete_originals=True)
                else: # if '_real.json' in json_files[i]:
                    real_path = glob.glob(json_files[i].replace('.json', '.nii*'))[0]
                    imag_path = glob.glob(json_files[i].replace('_real.json', '_imaginary.nii*'))[0]
                    logger.log(LogLevel.INFO.value, f"Correcting GE data: real={real_path}; imag={imag_path}")
                    fix_ge_complex(real_path, imag_path, delete_originals=True)
    if ge_corrections:
        logger.log(LogLevel.INFO.value, f"Loading updated JSON headers from '{output_dir}/.../extra_data' folders...")
        json_files = []
        json_datas = []
        for subject in subjects:
            sessions = get_folders_in(os.path.join(output_dir, subject))
            for session in sessions:
                session_extra_folder = os.path.join(output_dir, subject, session, "extra_data")
                json_files.extend(sorted(glob.glob(os.path.join(session_extra_folder, "*json"))))
                json_datas.extend([load_json(json_file) for json_file in sorted(glob.glob(os.path.join(session_extra_folder, "*json")))])

    logger.log(LogLevel.INFO.value, f"Enumerating protocol names from JSON headers...")
    all_protocol_names = []
    for i in range(len(json_datas)):
        if "Modality" not in json_datas[i]:
            logger.log(LogLevel.WARNING.value, f"'Modality' missing from JSON header '{json_files[i]}'. Skipping...")
            continue
        if json_datas[i]["Modality"] != "MR":
            continue
        if "ProtocolName" not in json_datas[i]:
            logger.log(LogLevel.WARNING.value, f"'ProtocolName' missing from JSON header '{json_files[i]}'. Skipping...")
            continue
        all_protocol_names.append(json_datas[i]["ProtocolName"].lower())
    all_protocol_names = sorted(list(set(all_protocol_names)))

    if not all_protocol_names:
        logger.log(LogLevel.ERROR.value, f"No valid protocol names found in JSON headers in '{output_dir}/.../extra_data' folders!")
        script_exit(1)

    logger.log(LogLevel.INFO.value, f"All protocol names identified: {all_protocol_names}")

    # identify protocol names using patterns if not interactive or auto_yes is enabled
    qsm_protocol_names = []
    t1w_protocol_names = []

    if not sys.__stdin__.isatty() or auto_yes:
        logger.log(LogLevel.INFO.value, f"Enumerating protocol names with QSM intention using match patterns {qsm_protocol_patterns}...")
        qsm_protocol_names = []
        for qsm_protocol_pattern in qsm_protocol_patterns:
            for protocol_name in all_protocol_names:
                if fnmatch.fnmatch(protocol_name, qsm_protocol_pattern):
                    qsm_protocol_names.append(protocol_name)
        if not qsm_protocol_names:
            logger.log(LogLevel.ERROR.value, "No QSM-intended protocols identified! Exiting...")
            script_exit(1)
        logger.log(LogLevel.INFO.value, f"Identified the following protocols intended for QSM: {qsm_protocol_names}")

        logger.log(LogLevel.INFO.value, f"Enumerating T1w protocol names using match patterns {t1w_protocol_patterns}...")
        t1w_protocol_names = []
        for t1w_protocol_pattern in t1w_protocol_patterns:
            for protocol_name in all_protocol_names:
                if fnmatch.fnmatch(protocol_name, t1w_protocol_pattern):
                    t1w_protocol_names.append(protocol_name)
        if not t1w_protocol_names:
            logger.log(LogLevel.WARNING.value, f"No T1w protocols found matching patterns {t1w_protocol_patterns}! Automated segmentation will not be possible.")
        else:
            logger.log(LogLevel.INFO.value, f"Identified the following protocols as T1w: {t1w_protocol_names}")

    else: # manually identify protocols using selection if interactive

        # === T2*W PROTOCOLS SELECTION ===
        print("== PROTOCOL NAMES ==")
        for i in range(len(all_protocol_names)):
            print(f"{i+1}. {all_protocol_names[i]}")
        while True:
            user_input = input("Identify protocols intended for QSM (comma-separated numbers): ")
            qsm_scans_idx = user_input.split(",")
            try:
                qsm_scans_idx = [int(j)-1 for j in qsm_scans_idx]
            except:
                print("Invalid input")
                continue
            qsm_scans_idx = sorted(list(set(qsm_scans_idx)))
            try:
                qsm_protocol_names = [all_protocol_names[j] for j in qsm_scans_idx]
                break
            except:
                print("Invalid input")
        if not qsm_protocol_names:
            logger.log(LogLevel.ERROR.value, "No QSM-intended protocols identified! Exiting...")
            script_exit(1)
        logger.log(LogLevel.INFO.value, f"Identified the following protocols intended for QSM: {qsm_protocol_names}")

        # === T1W PROTOCOLS SELECTION ===
        remaining_protocol_names = [protocol_name for protocol_name in all_protocol_names if protocol_name not in qsm_protocol_names]
        if remaining_protocol_names:
            print("== PROTOCOL NAMES ==")
            for i in range(len(remaining_protocol_names)):
                print(f"{i+1}. {remaining_protocol_names[i]}")
            while True:
                user_input = input("Identify T1w scans for automated segmentation (comma-separated numbers; enter nothing to ignore): ").strip()
                if user_input == "":
                    break
                t1w_scans_idx = user_input.split(",")
                try:
                    t1w_scans_idx = sorted(list(set([int(j)-1 for j in t1w_scans_idx])))
                except:
                    print("Invalid input")
                    continue
                try:
                    t1w_protocol_names = [remaining_protocol_names[j] for j in t1w_scans_idx]
                    break
                except:
                    print("Invalid input")
        if not t1w_protocol_names:
            logger.log(LogLevel.WARNING.value, f"No T1w protocols found matching patterns {t1w_protocol_patterns}! Automated segmentation will not be possible.")
        else:
            logger.log(LogLevel.INFO.value, f"Identified the following protocols as T1w: {t1w_protocol_names}")

    logger.log(LogLevel.INFO.value, 'Parsing relevant details from JSON headers...')
    all_session_details = []
    for subject in subjects:
        sessions = get_folders_in(os.path.join(output_dir, subject))
        for session in sessions:
            logger.log(LogLevel.INFO.value, f"Parsing relevant JSON data from {subject}/{session}...")
            logger.log(LogLevel.INFO.value, f"Parsing relevant JSON data from {subject}/{session}...")
            session_extra_folder = os.path.join(output_dir, subject, session, "extra_data")
            session_anat_folder = os.path.join(output_dir, subject, session, "anat")
            json_files = sorted(glob.glob(os.path.join(session_extra_folder, "*json")))
            session_details = []
            for json_file in json_files:
                json_data = load_json(json_file)
                if 'Modality' not in json_data:
                    logger.log(LogLevel.WARNING.value, f"'Modality' missing from JSON header '{json_file}'! Skipping...")
                    continue
                if 'ProtocolName' not in json_data:
                    logger.log(LogLevel.WARNING.value, f"'ProtocolName' missing from JSON header '{json_file}'! Skipping...")
                    continue
                if 'SeriesNumber' not in json_data:
                    logger.log(LogLevel.WARNING.value, f"'SeriesNumber' missing from JSON header '{json_file}'! Skipping...")
                    continue
                if 'EchoTime' not in json_data:
                    logger.log(LogLevel.WARNING.value, f"'EchoTime' missing from JSON header '{json_file}'! Skipping...")
                    continue
                if json_data['Modality'] == 'MR' and json_data['ProtocolName'].lower() in qsm_protocol_names + t1w_protocol_names:
                    details = {}
                    details['subject'] = subject
                    details['session'] = session
                    details['series_description'] = json_data['SeriesDescription'] if 'SeriesDescription' in json_data else None
                    details['protocol_type'] = None
                    details['protocol_name'] = clean(json_data['ProtocolName'].lower())
                    if json_data['ProtocolName'].lower() in qsm_protocol_names:
                        details['protocol_type'] = 'qsm'
                    elif json_data['ProtocolName'].lower() in t1w_protocol_names:
                        details['protocol_type'] = 't1w'
                    details['series_num'] = json_data['SeriesNumber']
                    details['acquisition_time'] = None
                    if 'AcquisitionTime' in json_data:
                        details['acquisition_time'] = datetime.datetime.strptime(json_data['AcquisitionTime'], "%H:%M:%S.%f")
                    if 'ImageType' in json_data.keys(): details['image_type'] = [t.upper() for t in json_data['ImageType']]
                    details['echo_time'] = json_data['EchoTime']
                    details['file_name'] = json_file.split('.json')[0]
                    details['run_num'] = None
                    details['echo_num'] = None
                    details['num_echoes'] = None
                    details['new_name'] = None
                    session_details.append(details)

            if session_details:
                session_details = sorted(session_details, key=lambda f: (f['subject'], f['session'], f['protocol_type'], f['protocol_name'], f['acquisition_time'], f['series_num'], 0 if any(t in f['image_type'] for t in ['P', 'PHASE']) else 1, f['echo_time']))

                # prune details based on known issues
                session_details = [details for details in session_details if not any(t in details['image_type'] for t in ['SWI', 'MASK', 'QSM'])]
                session_details = [details for details in session_details if details['series_description'] not in ['Pha_Images', 'SWI_Images', 't2star_wip_tra_p2_tgv_Mask', 't2star_wip_tra_p2_tgv_Qsm']]

                # update run numbers
                run_num = 1
                series_num = session_details[0]['series_num']
                protocol_type = session_details[0]['protocol_type']
                protocol_name = session_details[0]['protocol_name']
                acquisition_time = session_details[0]['acquisition_time']
                for i in range(len(session_details)):
                    if ((protocol_name == session_details[i]) and 
                        ((acquisition_time and session_details[i]['acquisition_time'] and abs(acquisition_time - session_details[i]['acquisition_time']) > datetime.timedelta(seconds=5)) or
                        (not (acquisition_time and session_details[i]['acquisition_time']) and session_details[i]['series_num'] != series_num))):
                        
                        if session_details[i]['protocol_type'] != session_details[i-1]['protocol_type']:
                            run_num = 1
                        elif session_details[i]['protocol_type'] == 'qsm' and any(t in session_details[i]['image_type'] for t in ['P', 'PHASE']):
                            run_num += 1
                        elif session_details[i]['protocol_type'] == 't1w':
                            run_num += 1
                        
                    series_num = session_details[i]['series_num']
                    acquisition_time = session_details[i]['acquisition_time']
                    protocol_name = session_details[i]['protocol_name']
                    protocol_type = session_details[i]['protocol_type']
                    session_details[i]['run_num'] = run_num

                # update echo numbers and number of echoes
                qsm_details = [details for details in session_details if details['protocol_type'] == 'qsm']
                qsm_acq_names = sorted(list(set(details['protocol_name'] for details in qsm_details)))
                for acq in qsm_acq_names:
                    acq_runs = sorted(list(set(details['run_num'] for details in qsm_details if details['protocol_name'] == acq)))
                    for run_num in acq_runs:
                        echo_times = sorted(list(set([details['echo_time'] for details in qsm_details if details['run_num'] == run_num and details['protocol_name'] == acq])))
                        num_echoes = len(echo_times)
                        for details in qsm_details:
                            if details['run_num'] == run_num and details['protocol_name'] == acq:
                                details['num_echoes'] = num_echoes
                                details['echo_num'] = echo_times.index(details['echo_time']) + 1

                # update part types
                for details in session_details:
                    if details['protocol_type'] == 'qsm' and any(t in details['image_type'] for t in ['P', 'PHASE']):
                        details['part_type'] = 'phase'
                    elif details['protocol_type'] == 'qsm' and any(t in details['image_type'] for t in ['M', 'MAGNITUDE']):
                        details['part_type'] = 'mag'

                # update names
                for details in session_details:
                    if details['protocol_type'] == 't1w':
                        details['new_name'] = os.path.join(session_anat_folder, f"{subject}_{session}_acq-{str(details['protocol_name'])}_run-{str(details['run_num']).zfill(2)}_T1w")
                    elif details['num_echoes'] == 1:
                        details['new_name'] = os.path.join(session_anat_folder, f"{subject}_{session}_acq-{str(details['protocol_name'])}_run-{str(details['run_num']).zfill(2)}_part-{details['part_type']}_T2starw")
                    else:
                        details['new_name'] = os.path.join(session_anat_folder, f"{subject}_{session}_acq-{str(details['protocol_name'])}_run-{str(details['run_num']).zfill(2)}_echo-{str(details['echo_num']).zfill(2)}_part-{details['part_type']}_MEGRE")

                # store session details
                all_session_details.extend(session_details)
    
    # check for extra mag/phase series
    for qsm_protocol_name in list(set(details['protocol_name'] for details in all_session_details if details['protocol_type'] == 'qsm')):
        num_mag_series = len(set(details['series_num'] for details in all_session_details if details['protocol_name'] == qsm_protocol_name and details['part_type'] == 'mag'))
        num_phs_series = len(set(details['series_num'] for details in all_session_details if details['protocol_name'] == qsm_protocol_name and details['part_type'] == 'phase'))

        if num_mag_series > 1:
            if sys.__stdin__.isatty() and not auto_yes:
                logger.log(LogLevel.INFO.value, f"Multiple magnitude series found for protocol {qsm_protocol_name}!")
                run_1_details = [details for details in all_session_details if details['protocol_name'] == qsm_protocol_name and details['run_num'] == 1 and details['part_type'] == 'mag']
                run_1_series_nums = sorted(list(set([details['series_num'] for details in run_1_details])))
                run_1_series_nums_idxs = [i for i in range(len(run_1_series_nums))]
                run_1_image_types = [next(details['image_type'] for details in run_1_details if details['series_num'] == series_num) for series_num in run_1_series_nums]
                run_1_descriptions = [next(details['series_description'] for details in run_1_details if details['series_num'] == series_num) for series_num in run_1_series_nums]
                
                # user selects which series idx to keep
                print(f"== MULTIPLE MAGNITUDE SERIES FOUND FOR {qsm_protocol_name} ==")
                for i in range(len(run_1_series_nums)):
                    print(f"{i+1}. IMAGE_TYPE={run_1_image_types[i]}; SeriesDescription={run_1_descriptions[i]}")
                while True:
                    mag_series_idx = input("Select magnitude series to use for QSM: ")
                    try:
                        mag_series_idx = int(mag_series_idx)-1
                        if mag_series_idx not in run_1_series_nums_idxs:
                            raise Exception()
                        break
                    except:
                        print("Invalid input")
            else:
                logger.log(LogLevel.WARNING.value, f"Multiple magnitude series found for protocol {qsm_protocol_name}! Using whichever series comes first! Run interactively to select.")
                mag_series_idx = 0
                
            # remove all magnitude series except the mag_series_idx-th from each run with qsm_protocol_name
            for run_num in sorted(list(set(details['run_num'] for details in all_session_details if details['protocol_name'] == qsm_protocol_name))):
                run_details = [details for details in all_session_details if details['protocol_name'] == qsm_protocol_name and details['run_num'] == run_num and details['part_type'] == 'mag']
                series_num_to_keep = sorted(list(set([details['series_num'] for details in run_details])))[mag_series_idx]
                to_remove = [details for details in run_details if details['series_num'] != series_num_to_keep]
                for details in to_remove:
                    all_session_details.remove(details)

        if num_phs_series > 1:
            if sys.__stdin__.isatty() and not auto_yes:
                logger.log(LogLevel.INFO.value, f"Multiple phase series found for protocol {qsm_protocol_name}!")
                run_1_details = [details for details in all_session_details if details['protocol_name'] == qsm_protocol_name and details['run_num'] == 1 and details['part_type'] == 'phase']
                run_1_series_nums = sorted(list(set([details['series_num'] for details in run_1_details])))
                run_1_series_nums_idxs = [i for i in range(len(run_1_series_nums))]
                run_1_image_types = [next(details['image_type'] for details in run_1_details if details['series_num'] == series_num) for series_num in run_1_series_nums]
                run_1_descriptions = [next(details['series_description'] for details in run_1_details if details['series_num'] == series_num) for series_num in run_1_series_nums]
                
                # user selects which series idx to keep
                print(f"== MULTIPLE PHASE SERIES FOUND FOR {qsm_protocol_name} ==")
                for i in range(len(run_1_series_nums)):
                    print(f"{i+1}. IMAGE_TYPE={run_1_image_types[i]}; SeriesDescription={run_1_descriptions[i]}")
                while True:
                    phs_series_idx = input("Select phase series to use for QSM: ")
                    try:
                        phs_series_idx = int(phs_series_idx)-1
                        if phs_series_idx not in run_1_series_nums_idxs:
                            raise Exception()
                        break
                    except:
                        print("Invalid input")
            else:
                logger.log(LogLevel.WARNING.value, f"Multiple phase series found for protocol {qsm_protocol_name}! Using whichever series comes first! Run interactively to select.")
                phs_series_idx = 0
                
            # remove all magnitude series except the phs_series_idx-th from each run with qsm_protocol_name
            for run_num in sorted(list(set(details['run_num'] for details in all_session_details if details['protocol_name'] == qsm_protocol_name))):
                run_details = [details for details in all_session_details if details['protocol_name'] == qsm_protocol_name and details['run_num'] == run_num and details['part_type'] == 'mag']
                series_num_to_keep = sorted(list(set([details['series_num'] for details in run_details])))[phs_series_idx]
                to_remove = [details for details in run_details if details['series_num'] != series_num_to_keep]
                for details in to_remove:
                    all_session_details.remove(details)


    print("Summary of identified files and proposed renames (following BIDS standard):")
    for f in all_session_details:
        print(f"{os.path.split(f['file_name'])[1]} \n\t -> {os.path.split(f['new_name'])[1]}")

    if len(all_session_details) != len(set(details['new_name'] for details in all_session_details)):
        logger.log(LogLevel.ERROR.value, f"Resultant BIDS data contains name conflicts! Try running `nifti-convert {output_dir} {output_dir}`.")
        script_exit(1)

    # if running interactively, show a summary of the renames prior to actioning
    if sys.__stdin__.isatty() and not auto_yes:
        if input("Confirm renaming? (n for no): ").strip().lower() in ["n", "no"]:
            script_exit(0)

    # rename all files
    logger.log(LogLevel.INFO.value, "Renaming files...")
    for details in all_session_details:
        rename(details['file_name']+'.json', details['new_name']+'.json', always_show=auto_yes)
        rename(details['file_name']+'.nii', details['new_name']+'.nii', always_show=auto_yes)
    
    # create required dataset_description.json file
    logger.log(LogLevel.INFO.value, 'Generating details for BIDS datset_description.json...')
    dataset_description = {
        "Name" : f"QSMxT BIDS ({datetime.date.today()})",
        "BIDSVersion" : "1.7.0",
        "GeneratedBy" : [{
            "Name" : "QSMxT",
            "Version": f"{get_qsmxt_version()}",
            "CodeURL" : "https://github.com/QSMxT/QSMxT"
        }],
        "Authors" : ["ADD AUTHORS HERE"]
    }
    logger.log(LogLevel.INFO.value, 'Writing BIDS dataset_description.json...')
    with open(os.path.join(output_dir, 'dataset_description.json'), 'w', encoding='utf-8') as dataset_json_file:
        json.dump(dataset_description, dataset_json_file)

    logger.log(LogLevel.INFO.value, 'Writing BIDS .bidsignore file...')
    with open(os.path.join(output_dir, '.bidsignore'), 'w', encoding='utf-8') as bidsignore_file:
        bidsignore_file.write('*dcm2niix_output.txt\n')
        bidsignore_file.write('references.txt\n')

    logger.log(LogLevel.INFO.value, 'Writing BIDS dataset README...')
    with open(os.path.join(output_dir, 'README'), 'w', encoding='utf-8') as readme_file:
        readme_file.write(f"Generated using QSMxT ({get_qsmxt_version()})\n")
        readme_file.write(f"\nDescribe your dataset here.\n")

def script_exit(exit_code=0):
    logger = make_logger()
    show_warning_summary(logger)
    logger.log(LogLevel.INFO.value, 'Finished')
    exit(exit_code)

def main():
    parser = argparse.ArgumentParser(
        description="QSMxT dicomConvert: Converts DICOM files to NIfTI/BIDS",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        'input_dir',
        help='Sorted DICOM directory generated using dicom_sort.py of the format {subject}/{session}/{series}'
    )

    parser.add_argument(
        'output_dir',
        help='Output BIDS directory.'
    )

    parser.add_argument(
        '--auto_yes',
        action='store_true',
        help='Force running non-interactively. This option is useful when used as part of a script or on a testing server.'
    )

    parser.add_argument(
        '--qsm_protocol_patterns',
        default=['*t2starw*', '*qsm*'],
        nargs='*',
        help='Patterns used to identify series acquired for QSM. These patterns will be used to match the \'ProtocolName\' '+
             'field. If no series are found matching these protocols, you will be prompted to select the appropriate '+
             'series\' interactively.'
    )

    parser.add_argument(
        '--t1w_protocol_patterns',
        default=['*t1w*'],
        nargs='*',
        help='Patterns used to identify series containing T1-weighted brain images. These series may be used during the '+
             'qsmxt.py script for automated brain segmentation and registration to the QSM space.'
    )

    args = parser.parse_args()

    args.input_dir = os.path.abspath(args.input_dir)
    args.output_dir = os.path.abspath(args.output_dir)

    os.makedirs(args.output_dir, exist_ok=True)

    logger = make_logger(
        logpath=os.path.join(args.output_dir, f"log_{str(datetime.datetime.now()).replace(':', '-').replace(' ', '_').replace('.', '')}.txt"),
        printlevel=LogLevel.INFO,
        writelevel=LogLevel.INFO,
        warnlevel=LogLevel.WARNING,
        errorlevel=LogLevel.ERROR
    )

    logger.log(LogLevel.INFO.value, f"Running QSMxT {get_qsmxt_version()}")
    logger.log(LogLevel.INFO.value, f"Command: {str.join(' ', sys.argv)}")
    logger.log(LogLevel.INFO.value, f"Python interpreter: {sys.executable}")

    diff = get_diff()
    if diff:
        logger.log(LogLevel.WARNING.value, f"Working directory not clean! Writing diff to {os.path.join(args.output_dir, 'diff.txt')}...")
        with open(os.path.join(args.output_dir, "diff.txt"), "w") as diff_file:
            diff_file.write(diff)

    # write "references.txt" with the command used to invoke the script and any necessary citations
    with open(os.path.join(args.output_dir, "references.txt"), 'w', encoding='utf-8') as f:
        # output QSMxT version, run command, and python interpreter
        f.write(f"QSMxT: {get_qsmxt_version()}")
        f.write(f"\nRun command: {str.join(' ', sys.argv)}")
        f.write(f"\nPython interpreter: {sys.executable}")

        f.write("\n\n == References ==")

        # qsmxt, dcm2niix, bids
        f.write("\n\n - Stewart AW, Robinson SD, O'Brien K, et al. QSMxT: Robust masking and artifact reduction for quantitative susceptibility mapping. Magnetic Resonance in Medicine. 2022;87(3):1289-1300. doi:10.1002/mrm.29048")
        f.write("\n\n - Stewart AW, Shaw T, Bollmann S. QSMxT: A Complete QSM Processing Framework. GitHub; 2022. https://github.com/QSMxT/QSMxT")
        f.write("\n\n - Li X, Morgan PS, Ashburner J, Smith J, Rorden C. The first step for neuroimaging data analysis: DICOM to NIfTI conversion. J Neurosci Methods. 2016;264:47-56. doi:10.1016/j.jneumeth.2016.03.001")
        f.write("\n\n - Rorden C et al. Rordenlab/Dcm2niix. GitHub; 2022. https://github.com/rordenlab/dcm2niix")
        f.write("\n\n - Gorgolewski KJ, Auer T, Calhoun VD, et al. The brain imaging data structure, a format for organizing and describing outputs of neuroimaging experiments. Sci Data. 2016;3(1):160044. doi:10.1038/sdata.2016.44")
        f.write("\n\n")
    
    convert_to_nifti(
        input_dir=args.input_dir,
        output_dir=args.output_dir,
        qsm_protocol_patterns=[pattern.lower() for pattern in args.qsm_protocol_patterns],
        t1w_protocol_patterns=[pattern.lower() for pattern in args.t1w_protocol_patterns],
        auto_yes=args.auto_yes
    )

    script_exit()

if __name__ == "__main__":
    main()

