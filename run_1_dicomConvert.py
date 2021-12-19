#!/usr/bin/env python3
import argparse
import os
import sys
import subprocess
import glob
import json
import fnmatch

def load_json(path):
    f = open(path)
    j = json.load(f)
    f.close()
    return j

def rename(old, new, always_show=False):
    if always_show or not sys.__stdin__.isatty():
        print(f'Renaming {old} -> {new}')
    if not os.path.exists(os.path.split(new)[0]):
        os.makedirs(os.path.split(new)[0], exist_ok=True)
    os.rename(old, new)

def clean(data): 
    return data.replace('_', '')

def convert_to_nifti(input_dir, output_dir, t2starw_series_patterns, t1w_series_patterns, auto_yes):
    print('Converting all DICOMs to nifti...')
    subjects = os.listdir(input_dir)
    for subject in subjects:
        sessions = os.listdir(os.path.join(input_dir, subject))
        for session in sessions:
            session_extra_folder = os.path.join(output_dir, clean(subject), session, "extra_data")
            os.makedirs(session_extra_folder, exist_ok=True)
            if 'dcm2niix_output.txt' in os.listdir(session_extra_folder):
                print(f'Warning: {session_extra_folder} already has dcm2niix conversion output! Skipping...')
                continue
            series = os.listdir(os.path.join(input_dir, subject, session))
            for s in series:
                series_dicom_folder = os.path.join(input_dir, subject, session, s)
                print(f"dcm2niix -z n -o {session_extra_folder} {series_dicom_folder}")
                subprocess.call(f"dcm2niix -z n -o {session_extra_folder} {series_dicom_folder} >> {os.path.join(session_extra_folder, 'dcm2niix_output.txt')}", executable='/bin/bash', shell=True)
    
    print(f"Enumerating series names from JSON headers in '{output_dir}/.../extra_data' folders...")
    all_series_names = []
    subjects = os.listdir(output_dir)
    json_files = []
    json_datas = []
    for subject in subjects:
        sessions = os.listdir(os.path.join(output_dir, subject))
        for session in sessions:
            session_extra_folder = os.path.join(output_dir, subject, session, "extra_data")
            json_files.extend(sorted(glob.glob(os.path.join(session_extra_folder, "*json"))))
            json_datas.extend([load_json(json_file) for json_file in sorted(glob.glob(os.path.join(session_extra_folder, "*json")))])
    all_series_names = sorted(list(set([
        json_datas[i]['SeriesDescription'].lower()
        for i in range(len(json_datas))
        if json_datas[i]["Modality"] == "MR"
    ])))
    if not all_series_names:
        print(f"No valid series found in JSON headers in '{output_dir}/.../extra_data' folders!")
        exit(1)
    print(f"All series names identified: {all_series_names}")

    # identify series using patterns
    t2starw_series_names = []
    for t2starw_series_pattern in t2starw_series_patterns:
        for series_name in all_series_names:
            if fnmatch.fnmatch(series_name, t2starw_series_pattern):
                t2starw_series_names.append(series_name)
    t1w_series_names = []
    for t1w_series_pattern in t1w_series_patterns:
        for series_name in all_series_names:
            if fnmatch.fnmatch(series_name, t1w_series_pattern):
                t1w_series_names.append(series_name)
    if t2starw_series_names:
        print(f"Chosen t2starw patterns {t2starw_series_patterns} matched with the following series: {t2starw_series_names}")
    if t1w_series_names:
        print(f"Chosen t1w patterns {t1w_series_patterns} matched with the following series: {t1w_series_names}")

    if not t2starw_series_names and (sys.__stdin__.isatty() and not auto_yes): # if running interactively
        print(f"No t2starw series found matching patterns: {t2starw_series_patterns}")
        for i in range(len(all_series_names)):
            print(f"{i+1}. {all_series_names[i]}")
        while True:
            user_input = input("Identify T2Starw scans for QSM (comma-separated numbers): ")
            t2starw_scans_idx = user_input.split(",")
            try:
                t2starw_scans_idx = [int(j)-1 for j in t2starw_scans_idx]
            except:
                print("Invalid input")
                continue
            t2starw_scans_idx = sorted(list(set(t2starw_scans_idx)))
            try:
                t2starw_series_names = [all_series_names[j] for j in t2starw_scans_idx]
                break
            except:
                print("Invalid input")
        if t2starw_series_names:
            print(f"Identified matching t2starw series: {t2starw_series_names}")
    elif not t2starw_series_names:
        print(f"Error: No t2starw series found matching patterns: {t2starw_series_patterns}!")
        exit(1)

    # identify T1w series
    if not t1w_series_names and (sys.__stdin__.isatty() and not auto_yes):
        print(f"No t1w series found matching pattern: {t1w_series_pattern}")
        for i in range(len(all_series_names)):
            print(f"{i+1}. {all_series_names[i]}")
        while True:
            user_input = input("Identify t1w scans for automated segmentation (comma-separated numbers; enter nothing to ignore): ").strip()
            if user_input == "":
                break
            t1w_scans_idx = user_input.split(",")
            try:
                t1w_scans_idx = sorted(list(set([int(j)-1 for j in t1w_scans_idx])))
            except:
                print("Invalid input")
                continue
            try:
                t1w_series_names = [all_series_names[j] for j in t1w_scans_idx]
                break
            except:
                print("Invalid input")
        if t1w_series_names:
            print(f"Identified matching t1w series: {t1w_series_names}")
    if not t1w_series_names:
        print(f"Warning: No t1w series found matching patterns {t1w_series_patterns}! Automated segmentation will not be possible.")
    
    print('Parsing JSON headers...')
    all_session_details = []
    for subject in subjects:
        sessions = os.listdir(os.path.join(output_dir, subject))
        for session in sessions:
            session_extra_folder = os.path.join(output_dir, subject, session, "extra_data")
            session_anat_folder = os.path.join(output_dir, subject, session, "anat")
            json_files = sorted(glob.glob(os.path.join(session_extra_folder, "*json")))
            session_details = []
            for json_file in json_files:
                json_data = load_json(json_file)
                if json_data['Modality'] == 'MR' and json_data['SeriesDescription'].lower() in t2starw_series_names + t1w_series_names:
                    details = {}
                    details['subject'] = subject
                    details['session'] = session
                    details['series_type'] = None
                    if json_data['SeriesDescription'].lower() in t2starw_series_names:
                        details['series_type'] = 't2starw'
                    elif json_data['SeriesDescription'].lower() in t1w_series_names:
                        details['series_type'] = 't1w'
                    details['series_num'] = json_data['SeriesNumber']
                    details['part_type'] = 'phase' if 'P' in json_data['ImageType'] else 'magnitude'
                    details['echo_time'] = json_data['EchoTime']
                    details['file_name'] = json_file.split('.')[0]
                    details['run_num'] = None
                    details['echo_num'] = None
                    details['num_echoes'] = None
                    details['new_name'] = None
                    session_details.append(details)
            session_details = sorted(session_details, key=lambda f: (f['subject'], f['session'], f['series_type'], f['series_num'], 0 if 'phase' in f['part_type'] else 1, f['echo_time']))
            
            # update run numbers
            run_num = 1
            series_num = session_details[0]['series_num']
            series_type = session_details[0]['series_type']
            for i in range(len(session_details)):
                if session_details[i]['series_num'] != series_num:
                    if session_details[i]['series_type'] == 't2starw' and session_details[i-1]['part_type'] == 'phase':
                        run_num += 1
                    elif session_details[i]['series_type'] == 't1w' and session_details[i-1]['series_type'] == 't1w':
                        run_num += 1
                    elif session_details[i]['series_type'] != session_details[i-1]['series_type']:
                        run_num = 1
                    
                series_num = session_details[i]['series_num']
                series_type = session_details[0]['series_type']
                session_details[i]['run_num'] = run_num

            # update echo numbers and number of echoes
            t2starw_details = [details for details in session_details if details['series_type'] == 't2starw']
            t2starw_run_nums = sorted(list(set(details['run_num'] for details in t2starw_details)))
            for run_num in t2starw_run_nums:
                echo_times = sorted(list(set([details['echo_time'] for details in t2starw_details if details['run_num'] == run_num])))
                num_echoes = len(echo_times)
                for details in t2starw_details:
                    if details['run_num'] == run_num:
                        details['num_echoes'] = num_echoes
                        details['echo_num'] = echo_times.index(details['echo_time']) + 1

            # update names
            for details in session_details:
                if details['series_type'] == 't1w':
                    details['new_name'] = os.path.join(session_anat_folder, f"{clean(subject)}_{clean(session)}_run-{details['run_num']}_T1w")
                elif details['num_echoes'] == 1:
                    details['new_name'] = os.path.join(session_anat_folder, f"{clean(subject)}_{clean(session)}_run-{details['run_num']}_part-{details['part_type']}_T2starw")
                else:
                    details['new_name'] = os.path.join(session_anat_folder, f"{clean(subject)}_{clean(session)}_run-{details['run_num']}_echo-{details['echo_num']}_part-{details['part_type']}_MEGRE")

            # store session details
            all_session_details.extend(session_details)

    # if running interactively, show a summary of the renames prior to actioning
    if sys.__stdin__.isatty() and not auto_yes:
        print("Summary of identified files and proposed renames (following BIDS standard):")
        for f in all_session_details:
            print(f"{os.path.split(f['file_name'])[1]} \n\t -> {os.path.split(f['new_name'])[1]}")
        if input("Confirm renaming? (n for no): ").strip().lower() in ["n", "no"]:
            exit()

    # rename all files
    print("Renaming files...")
    for details in all_session_details:
        rename(details['file_name']+'.json', details['new_name']+'.json', always_show=auto_yes)
        rename(details['file_name']+'.nii', details['new_name']+'.nii', always_show=auto_yes)
    print("Finished!")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="QSMxT dicomConvert: Converts DICOM files to NIfTI/BIDS",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        'input_dir',
        help='Sorted DICOM directory generated using run_0_dicomSort.py of the format {subject}/{session}/{series}'
    )

    parser.add_argument(
        'output_dir',
        help='Output directory for converted NIfTIs'
    )

    parser.add_argument(
        '--use_patient_names',
        action='store_true',
        help='Use the PatientName rather than PatientID for subject folders'
    )

    parser.add_argument(
        '--use_session_dates',
        action='store_true',
        help='Use the StudyDate field rather than an incrementer for session IDs'
    )

    parser.add_argument(
        '--auto_yes',
        action='store_true',
        help='Force running non-interactively'
    )

    parser.add_argument(
        '--t2starw_series_patterns',
        default=['*t2starw*', '*qsm*'],
        nargs='*',
        help='Patterns used to identify t2starw series for QSM from the DICOM SeriesDescription field (case insensitive)'
    )

    parser.add_argument(
        '--t1w_series_patterns',
        default=['*t1w*'],
        nargs='*',
        help='Patterns used to identify t1w series for segmentation from the DICOM SeriesDescription field (case insensitive)'
    )


    args = parser.parse_args()
    
    convert_to_nifti(
        input_dir=args.input_dir,
        output_dir=args.output_dir,
        t2starw_series_patterns=args.t2starw_series_patterns,
        t1w_series_patterns=args.t1w_series_patterns,
        auto_yes=args.auto_yes
    )
    
