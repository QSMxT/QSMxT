#!/usr/bin/env python3

# Adapted from Alex Weston
# Digital Innovation Lab, Mayo Clinic
# https://gist.github.com/alex-weston-13/4dae048b423f1b4cb9828734a4ec8b83

import argparse
import os
import sys
import nibabel as nib
import json
import shutil
from fnmatch import fnmatch
from re import findall

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

def get_value_or_none(dictionary, key):
    return dictionary[key] if key in dictionary else None

def print_details(details):
    print()
    print('filename:', details['filename'])
    print('\tsubject:', details['subject'])
    print('\tsession:', details['session'])
    print('\trun:', details['run'])
    print('\tmulti-echo:', details['multi-echo'])
    print('\techo number:', details['echo'])
    print('\techo time:', details['echo_time'])
    print('\tfield strength:', details['field_strength'])
    print('\tprotocol name:', details['protocol_name'])
    print('\tseries type:', details['series_type'])
    print('\tpart type:', details['part_type'])
    print()


def nifti_to_bids(input_dir, output_dir):
    os.makedirs(output_dir, exist_ok=True)

    csv_file = os.path.join(args.output_dir, 'dataset.csv')
    if os.path.exists(csv_file):
        print(f"CSV file '{csv_file}' found in output folder!")
        csv_contents = []
        with open(csv_file, "r") as f:
            for line in f:
                line_contents = line.replace("\n", "").split(",")
                if len(line_contents) != 10:
                    print('Incorrect number of columns in CSV! Delete it or correct it and try again.')
                    exit()
                if '' in line_contents:
                    print('CSV file incomplete! Delete it or complete it and try again.')
                    exit()
                csv_contents.append(line_contents)
        csv_contents = csv_contents[1:]

        file_details = []
        for line_contents in csv_contents:
            details = {}
            details['filename'] = line_contents[0]
            details['subject'] = line_contents[1]
            details['session'] = line_contents[2]
            details['run'] = line_contents[3]
            details['echo'] = line_contents[4]
            details['echo_time'] = line_contents[5]
            details['multi-echo'] = line_contents[6]
            details['field_strength'] = line_contents[7]
            details['series_type'] = line_contents[8]
            details['part_type'] = line_contents[9]
            file_details.append(details)
    else:
        print(f"Searching for NIfTI files in input directory '{args.input_dir}'...")
        unsorted_list = []
        for root, dirs, files in os.walk(input_dir):
            for f in files:
                if f[-4:] == '.nii' or f[-7:] == '.nii.gz':
                    unsorted_list.append(os.path.join(root, f))
        print(f'Found {len(unsorted_list)} NIfTI files.')

        print(f'Extracting any relevant details from filepaths and corresponding JSON files using match patterns and regular expressions...')
        file_details = []
        info_needed = False
        for nifti_file in unsorted_list:
            details = {}
            details['filename'] = nifti_file
            details['directory'] = os.path.split(nifti_file)[0]

            subject_matches = findall(args.subject_pattern, nifti_file) if args.subject_pattern else None
            session_matches = findall(args.session_pattern, nifti_file) if args.session_pattern else None
            run_matches = findall(args.run_pattern, nifti_file) if args.run_pattern else None
            echo_matches = findall(args.echo_pattern, nifti_file) if args.echo_pattern else None
            protocol_matches = findall(args.protocol_pattern, nifti_file) if args.protocol_pattern else None

            details['subject'] = subject_matches[0] if subject_matches else None
            details['session'] = session_matches[0] if session_matches else None
            details['run'] = run_matches[0] if run_matches else None
            details['echo'] = echo_matches[0] if echo_matches else None
            details['protocol_name'] = protocol_matches[0] if protocol_matches else None

            details['echo_time'] = None
            details['multi-echo'] = None
            details['field_strength'] = None
            details['protocol_name'] = None
            details['series_type'] = None
            details['part_type'] = None

            magnitude = fnmatch(nifti_file, args.magnitude_pattern) if args.magnitude_pattern else None
            phase = fnmatch(nifti_file, args.phase_pattern) if args.phase_pattern else None
            t1 = fnmatch(nifti_file, args.t1w_pattern) if args.t1w_pattern else None

            if t1: 
                details['series_type'] = 't1w'
                details['echo'] = 1
                details['multi-echo'] = 'No'
            if magnitude or phase: details['series_type'] = 't2starw'
            if magnitude: details['part_type'] = 'magnitude'
            if phase: details['part_type'] = 'phase'

            json_file = json_filename(nifti_file)
            if os.path.exists(json_file):
                json_data = load_json(json_file)
                if 'EchoTime' in json_data: details['echo_time'] = json_data['EchoTime']
                if 'MagneticFieldStrength' in json_data: details['field_strength'] = json_data['MagneticFieldStrength']
                if 'ProtocolName' in json_data: details['protocol_name'] = json_data['ProtocolName']
                if 'ImageType' in json_data: details['part_type'] = 'phase' if 'P' in json_data['ImageType'] else 'magnitude'
                if 'EchoNumber' in json_data: details['echo'] = json_data['EchoNumber']
                if 'EchoTrainLength' in json_data:
                    try:
                        num_echoes = int(json_data['EchoTrainLength'])
                        if num_echoes > 1: details['multi-echo'] = 'Yes'
                        if num_echoes == 1: details['multi-echo'] = 'No'
                        if not details['multi-echo'] and details['echo'] not in [None, '1']:
                            details['multi-echo'] = 'Yes'
                    except:
                        pass
            
            if details['protocol_name']:
                if args.t1w_protocol_patterns and any([fnmatch(details['protocol_name'], pattern) for pattern in args.t1w_protocol_patterns]):
                    details['series_type'] = 't1w'
                if args.t2starw_protocol_patterns and any([fnmatch(details['protocol_name'], pattern) for pattern in args.t2starw_protocol_patterns]):
                    details['series_type'] = 't2starw'

            info_needed = info_needed or any(v is None for v in list(details.values()))
            file_details.append(details)
        print("Finished extracting details.")

        if info_needed:
            print(f"Some information is missing! Writing all details to spreadsheet '{csv_file}'...")
            f = open(csv_file, 'w')
            f.write('filename,subject id,session id,run number,echo number,echo_time (ms),multi-echo (yes or no),field_strength (T),series_type (t2starw or t1w),part_type (magnitude or phase)\n')
            for d in file_details:
                line = f"{d['filename']},{d['subject']},{d['session']},{d['run']},{d['echo']},{d['echo_time']},{d['multi-echo']},{d['field_strength']},{d['series_type']},{d['part_type']}\n"
                line = line.replace(",None", ",").replace("None,", ",")
                f.write(line)
            f.close()
            print(f"PLEASE REVIEW SPREADSHEET '{csv_file}', FILL IN MISSING INFORMATION AND TRY AGAIN.")
            exit()

    print(f"Computing new file names...")
    for details in file_details:
        ext = 'nii.gz' if details['filename'][:-6] == 'nii.gz' else 'nii'
        if details['series_type'] == 't1w':
            details['new_name'] = os.path.join(args.output_dir, f"sub-{details['subject']}", f"ses-{details['session']}", "anat", f"sub-{details['subject']}_ses-{details['session']}_run-{details['run']}_T1w.{ext}")
        elif details['multi-echo'].lower() == 'no':
            details['new_name'] = os.path.join(args.output_dir, f"sub-{details['subject']}", f"ses-{details['session']}", "anat", f"sub-{details['subject']}_ses-{details['session']}_run-{details['run']}_part-{details['part_type']}_T2starw.{ext}")
        else:
            details['new_name'] = os.path.join(args.output_dir, f"sub-{details['subject']}", f"ses-{details['session']}", "anat", f"sub-{details['subject']}_ses-{details['session']}_run-{details['run']}_echo-{details['echo']}_part-{details['part_type']}_MEGRE.{ext}")

    # if running interactively, show a summary of the renames prior to actioning
    if sys.__stdin__.isatty() and not args.auto_yes:
        print("Summary of identified files and proposed new names (following BIDS standard):")
        for f in file_details:
            print(f"{os.path.split(f['filename'])[1]} \n\t -> {os.path.split(f['new_name'])[1]}")
        if input("Confirm copy + renames? (n for no): ").strip().lower() in ["n", "no"]:
            exit()

    # copy/rename all files
    print("Copying NIfTI files with renames...")
    for details in file_details:
        copy(details['filename'], details['new_name'], always_show=args.auto_yes)
    print("Copying JSON files with renames and generating them if needed...")
    for details in file_details:
        f = json_filename(details['filename'])
        if os.path.exists(f):
            copy(f, json_filename(details['new_name']), always_show=args.auto_yes)
        else:
            dictionary = { 
                "EchoTime" : float(details['echo_time']),
                "MagneticFieldStrength" : float(details['field_strength']),
                "EchoNumber" : int(details['echo']),
                "ImageType" : ["P", "PHASE"] if details['part_type'] == 'phase' else ["M", "MAGNITUDE"],
                "ProtocolName" : details['series_type']
            }
            with open(json_filename(details['new_name']), 'w') as json_file:
                json.dump(dictionary, json_file)
            print(f"Automatically generated JSON file '{json_filename(details['new_name'])}'")
    print("Done!")


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
        default='*magnitude*',
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

    os.makedirs(args.output_dir, exist_ok=True)

    with open(os.path.join(args.output_dir, "details_and_citations.txt"), 'w') as f:
        # output command used to invoke script
        f.write(str.join(" ", sys.argv))

        # qsmxt, sort_dicoms.py
        #f.write("\n\n - Stewart AW, Robinson SD, O'Brien K, et al. QSMxT: Robust masking and artifact reduction for quantitative susceptibility mapping. Magnetic Resonance in Medicine. 2022;87(3):1289-1300. doi:10.1002/mrm.29048")
        #f.write("\n\n - Weston A. alex-weston-13/sort_dicoms.py. GitHub; 2020. https://gist.github.com/alex-weston-13/4dae048b423f1b4cb9828734a4ec8b83")
        #f.write("\n\n")

    nifti_to_bids(
        input_dir=args.input_dir,
        output_dir=args.output_dir,
    )
    
