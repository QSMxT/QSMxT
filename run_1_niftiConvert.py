#!/usr/bin/env python3

import argparse
import os
import sys
import nibabel as nib
import json
import shutil
import datetime

from fnmatch import fnmatch
from re import findall

from scripts.get_qsmxt_version import get_qsmxt_version


def copy(old, new, always_show=False):
    if always_show or not sys.__stdin__.isatty():
        print(f'Copying {old} -> {new}')
    if not os.path.exists(os.path.split(new)[0]):
        os.makedirs(os.path.split(new)[0], exist_ok=True)
    shutil.copy2(old, new)


def load_json(path):
    f = open(path)
    j = json.load(f)
    f.close()
    return j


def json_filename(nifti_filename):
    return nifti_filename.replace(".nii.gz", ".nii").replace(".nii", ".json")


def parse_num_or_exit(string, error_message, whole_number=False):
    try:
        return int(string) if whole_number else float(string)
    except:
        print(error_message)
        exit()


def flatten(a):
    return [i for g in a for i in g]


def find_files_with_extension(input_dir, extension):
    file_list = []
    for root, dirs, files in os.walk(input_dir):
        for f in files:
            if (isinstance(extension, list) and any(f.endswith(a) for a in extension)):
                file_list.append(os.path.join(root, f))
            elif isinstance(extension, str) and f.endswith(extension):
                file_list.append(os.path.join(root, f))
    return file_list


def get_details_from_csv(csv_file):
    csv_contents = []
    with open(csv_file, "r") as f:
        for line in f:
            line_contents = line.replace("\n", "").split(",")
            if len(line_contents) != 10:
                print('ERROR: Incorrect number of columns in CSV! Delete it or correct it and try again.')
                exit()
            if '' in line_contents:
                print('ERROR: CSV file incomplete! Delete it or complete it and try again.')
                exit()
            csv_contents.append(line_contents)
    csv_contents = csv_contents[1:]
    
    all_details = []
    for i, line_contents in enumerate(csv_contents):
        details = {}
        details['filename'] = line_contents[0]
        details['subject_id'] = line_contents[1]
        details['session_id'] = line_contents[2]
        details['series_type'] = line_contents[8]
        details['multi-echo'] = line_contents[6].strip().lower()
        details['part_type'] = line_contents[9].strip().lower()
        
        details['run_num'] = parse_num_or_exit(
            line_contents[3],
            error_message=f"ERROR: Could not parse run number '{line_contents[3]}' on line {i+1} as int",
            whole_number=True
        )
        
        details['echo_num'] = parse_num_or_exit(
            line_contents[4],
            error_message=f"ERROR: Could not parse echo number '{line_contents[4]}' on line {i+1} as int",
            whole_number=True
        )

        details['echo_time'] = parse_num_or_exit(
            line_contents[5],
            error_message=f"ERROR: Could not parse echo time '{line_contents[5]}' on line {i+1} as float",
            whole_number=False
        )
        
        details['field_strength'] = parse_num_or_exit(
            line_contents[7],
            error_message=f"ERROR: Could not parse field strength '{line_contents[7]}' on line {i+1} as float",
            whole_number=False
        )
        
        if details['multi-echo'] not in ['yes', 'no']:
            print(f"ERROR: Could not parse multi-echo field contents '{details['multi-echo']}' on line {i+1} as 'yes' or 'no'")
            exit()

        if details['part_type'] not in ['phase', 'mag']:
            print(f"ERROR: Could not parse part type field contents '{details['part_type']}' on line {i+1} as 'mag' or 'phase'")
            exit()


        all_details.append(details)
        
    return all_details


def get_details_from_filenames(file_list):
    all_details = []
    for nifti_file in file_list:
        details = {}
        details['filename'] = nifti_file
        details['directory'] = os.path.split(nifti_file)[0]

        subject_matches = findall(args.subject_pattern, nifti_file) if args.subject_pattern else None
        session_matches = findall(args.session_pattern, nifti_file) if args.session_pattern else None
        run_matches = findall(args.run_pattern, nifti_file) if args.run_pattern else None
        echo_matches = findall(args.echo_pattern, nifti_file) if args.echo_pattern else None
        protocol_matches = findall(args.protocol_pattern, nifti_file) if args.protocol_pattern else None

        details['subject_id'] = subject_matches[0] if subject_matches else None
        details['session_id'] = session_matches[0] if session_matches else None
        details['run_num'] = run_matches[0] if run_matches else None
        details['echo_num'] = echo_matches[0] if echo_matches else None
        details['protocol_name'] = protocol_matches[0] if protocol_matches else None

        details['echo_time'] = None
        details['multi-echo'] = None
        details['field_strength'] = None
        details['protocol_name'] = None
        details['series_type'] = None
        details['part_type'] = None

        if details['protocol_name']:
            if args.t1w_protocol_patterns and any([fnmatch(details['protocol_name'], pattern) for pattern in args.t1w_protocol_patterns]):
                details['series_type'] = 't1w'
            if args.t2starw_protocol_patterns and any([fnmatch(details['protocol_name'], pattern) for pattern in args.t2starw_protocol_patterns]):
                details['series_type'] = 't2starw'

        magnitude = fnmatch(nifti_file, args.magnitude_pattern) if args.magnitude_pattern else None
        phase = fnmatch(nifti_file, args.phase_pattern) if args.phase_pattern else None
        t1 = fnmatch(nifti_file, args.t1w_pattern) if args.t1w_pattern else None

        if t1: 
            details['series_type'] = 't1w'
            details['echo_num'] = 1
            details['multi-echo'] = 'no'
        if magnitude or phase: details['series_type'] = 't2starw'
        if magnitude: details['part_type'] = 'mag'
        if phase: details['part_type'] = 'phase'

        all_details.append(details)

    return all_details


def update_details_with_jsons(all_details):
    for details in all_details:
        json_file = json_filename(details['filename'])
        if os.path.exists(json_file):
            json_data = load_json(json_file)
            if 'EchoTime' in json_data:
                try: details['echo_time'] = float(json_data['EchoTime'])
                except: pass
            if 'MagneticFieldStrength' in json_data:
                try: details['field_strength'] = float(json_data['MagneticFieldStrength'])
                except: pass
            if 'EchoNumber' in json_data:
                try: details['echo_num'] = int(json_data['EchoNumber'])
                except: pass
            if 'ProtocolName' in json_data:
                details['protocol_name'] = json_data['ProtocolName']
                if args.t1w_protocol_patterns and any([fnmatch(details['protocol_name'], pattern) for pattern in args.t1w_protocol_patterns]):
                    details['series_type'] = 't1w'
                if args.t2starw_protocol_patterns and any([fnmatch(details['protocol_name'], pattern) for pattern in args.t2starw_protocol_patterns]):
                    details['series_type'] = 't2starw'
            if 'ImageType' in json_data:
                details['part_type'] = 'phase' if 'P' in json_data['ImageType'] else 'mag'
            if 'EchoTrainLength' in json_data:
                try:
                    num_echoes = int(json_data['EchoTrainLength'])
                    if num_echoes > 1: details['multi-echo'] = 'yes'
                    if num_echoes == 1: details['multi-echo'] = 'no'
                    if not details['multi-echo'] and details['echo_num'] not in [None, '1']:
                        details['multi-echo'] = 'yes'
                except:
                    pass
    return all_details


def write_details_to_csv(all_details):
    f = open(csv_file, 'w')
    f.write('filename,subject id,session id,run number,echo number,echo_time (ms),multi-echo (yes or no),field_strength (T),series_type (t2starw or t1w),part_type (mag or phase)\n')
    for d in all_details:
        line = f"{d['filename']},{d['subject_id']},{d['session_id']},{d['run_num']},{d['echo_num']},{d['echo_time']},{d['multi-echo']},{d['field_strength']},{d['series_type']},{d['part_type']}\n"
        line = line.replace(",None", ",").replace("None,", ",")
        f.write(line)
    f.close()


def nifti_to_bids(input_dir, output_dir):
    if os.path.exists(csv_file):
        print(f"INFO: CSV spreadsheet '{csv_file}' found! Reading...")
        all_details = get_details_from_csv(csv_file)
        print(f"INFO: CSV spreadsheet loaded.")
    else:
        print(f"INFO: Finding NIfTI files...")
        nifti_files = find_files_with_extension(args.input_dir, ['.nii', '.nii.gz'])
        print(f"INFO: {len(nifti_files)} NIfTI files found.")
        print(f"INFO: Extracting details from filenames using patterns...")
        all_details = get_details_from_filenames(nifti_files)
        print(f"INFO: Done reading details.")
        print(f"INFO: Updating details with JSON header information...")
        all_details = update_details_with_jsons(all_details)
        print(f"INFO: Done reading JSON header files.")

    if any(value is None for value in flatten([list(details.values()) for details in all_details])):
        print(f"INFO: Some information is missing! Writing all details to CSV spreadsheet '{csv_file}'...")
        write_details_to_csv(all_details)
        print(f"INFO: Done writing to CSV.")
        print(f"PLEASE REVIEW SPREADSHEET '{csv_file}', FILL IN MISSING INFORMATION AND RUN AGAIN WITH THE SAME COMMAND.")
        exit()

    print(f"INFO: Computing new NIfTI file names and locations...")
    for details in all_details:
        ext = 'nii.gz' if details['filename'].endswith('nii.gz') else 'nii'
        if details['series_type'] == 't1w':
            details['new_name'] = os.path.join(args.output_dir, f"sub-{details['subject_id']}", f"ses-{details['session_id']}", "anat", f"sub-{details['subject_id']}_ses-{details['session_id']}_run-{details['run_num']}_T1w.{ext}")
        elif details['multi-echo'] and details['multi-echo'].lower() == 'no':
            details['new_name'] = os.path.join(args.output_dir, f"sub-{details['subject_id']}", f"ses-{details['session_id']}", "anat", f"sub-{details['subject_id']}_ses-{details['session_id']}_run-{details['run_num']}_part-{details['part_type']}_T2starw.{ext}")
        else:
            details['new_name'] = os.path.join(args.output_dir, f"sub-{details['subject_id']}", f"ses-{details['session_id']}", "anat", f"sub-{details['subject_id']}_ses-{details['session_id']}_run-{details['run_num']}_echo-{details['echo_num']}_part-{details['part_type']}_MEGRE.{ext}")
    print(f"INFO: New NIfTI file names and locations determined.")

    # if running interactively, show a summary of the renames prior to actioning
    if sys.__stdin__.isatty() and not args.auto_yes:
        print("Summary of identified files and proposed new names (following BIDS standard):")
        for f in all_details:
            print(f"{os.path.split(f['filename'])[1]} \n\t -> {os.path.split(f['new_name'])[1]}")
        print("Confirm copy + renames? (n for no): ")
        if input().strip().lower() in ["n", "no"]:
            exit()

    # copy/rename all files
    print("INFO: Copying NIfTI files to new locations with new names...")
    for details in all_details:
        copy(details['filename'], details['new_name'], always_show=args.auto_yes)
    print("INFO: Done copying NIfTI files.")

    print("INFO: Copying JSON header files if present and generating them if needed...")
    for details in all_details:
        f = json_filename(details['filename'])
        if os.path.exists(f):
            copy(f, json_filename(details['new_name']), always_show=args.auto_yes)
        else:
            dictionary = { 
                "EchoTime" : details['echo_time'],
                "MagneticFieldStrength" : details['field_strength'],
                "EchoNumber" : details['echo_num'],
                "ImageType" : ["P", "PHASE"] if details['part_type'] == 'phase' else ["M", "MAGNITUDE"],
                "ProtocolName" : details['series_type']
            }
            with open(json_filename(details['new_name']), 'w', encoding='utf-8') as json_file:
                json.dump(dictionary, json_file)
            print(f"INFO: Automatically generated JSON header file '{json_filename(details['new_name'])}'")
    print("INFO: Done copying and generating JSON header files.")

    # create required dataset_description.json file
    print('Generating datset description details...')
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
    print('Writing dataset_description.json...')
    with open(os.path.join(args.output_dir, 'dataset_description.json'), 'w', encoding='utf-8') as dataset_json_file:
        json.dump(dataset_description, dataset_json_file)
    print('Done writing dataset_description.json')

    print('Writing .bidsignore file...')
    with open(os.path.join(args.output_dir, '.bidsignore'), 'w') as bidsignore_file:
        bidsignore_file.write('details_and_citations.txt\n')
        bidsignore_file.write('dataset_qsmxt.csv\n')
    print('Done writing .bidsignore file')

    with open(os.path.join(args.output_dir, 'README'), 'w') as readme_file:
        readme_file.write(f"Generated using QSMxT ({get_qsmxt_version()})\n")
        readme_file.write(f"\nDescribe your dataset here.\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="QSMxT niftiConvert: Sorts NIfTI files into a near-BIDS format for use with QSMxT",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        'input_dir',
        help='input NIfTI directory; will be recursively searched for NIfTI files'
    )

    parser.add_argument(
        'output_dir',
        help='output near-BIDS format folder'
    )

    parser.add_argument(
        '--magnitude_pattern',
        type=str,
        default='*mag*',
        help='Pattern used to match t2starw magnitude files for QSM'
    )

    parser.add_argument(
        '--phase_pattern',
        type=str,
        default='*phase*',
        help='Pattern used to match t2starw phase files for QSM'
    )

    parser.add_argument(
        '--t1w_pattern',
        type=str,
        default='*T1w*',
        help='Pattern used to match T1w images for segmentation'
    )

    parser.add_argument(
        '--t1w_protocol_patterns',
        type=str,
        default=['*t1w*'],
        help='Patterns used to match protocol names in JSON headers to identify T1w images for segmentation'
    )

    parser.add_argument(
        '--t2starw_protocol_patterns',
        type=str,
        default=['*qsm*', '*t2starw*'],
        help='Patterns used to match protocol names in JSON headers to identify t2starw images for QSM'
    )

    parser.add_argument(
        '--subject_pattern',
        type=str,
        default='sub-([^_/\\\\]+)',
        help='Regular expression to retrieve the subject name from the filepath'
    )

    parser.add_argument(
        '--session_pattern',
        type=str,
        default='ses-([^_/\\\\]+)',
        help='Regular expression to retrieve the session name from the filepath'
    )

    parser.add_argument(
        '--protocol_pattern',
        type=str,
        default=None,
        help='Regular expression to retrieve the protocol name from the filepath'
    )

    parser.add_argument(
        '--run_pattern',
        type=str,
        default='run-([0-9]+)',
        help='Regular expression to retrieve the run number from the filepath'
    )

    parser.add_argument(
        '--echo_pattern',
        type=str,
        default='echo-([0-9]+)',
        help='Regular expression to retrieve the echo number from the filepath'
    )

    parser.add_argument(
        '--auto_yes',
        action='store_true',
        help='Force running non-interactively'
    )

    args = parser.parse_args()

    args.input_dir = os.path.abspath(args.input_dir)
    args.output_dir = os.path.abspath(args.output_dir)
    this_dir = os.path.dirname(os.path.abspath(__file__))
    csv_file = os.path.join(args.output_dir, 'dataset_qsmxt.csv')

    os.makedirs(args.output_dir, exist_ok=True)

    with open(os.path.join(args.output_dir, "details_and_citations.txt"), 'w') as f:
        # output QSMxT version
        f.write(f"QSMxT: {get_qsmxt_version()}")
        f.write("\n\n")

        # output command used to invoke script
        f.write(str.join(" ", sys.argv))

        f.write("\n\n - Brett M, Markiewicz CJ, Hanke M, et al. nipy/nibabel. GitHub; 2019. https://github.com/nipy/nibabel")
        f.write("\n\n - Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT")
        f.write("\n\n")

    nifti_to_bids(
        input_dir=args.input_dir,
        output_dir=args.output_dir,
    )

