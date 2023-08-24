#!/usr/bin/env python3

import argparse
import os
import sys
import json
import shutil
import datetime

from fnmatch import fnmatch
from re import findall

from qsmxt.scripts.qsmxt_functions import get_qsmxt_version, get_diff
from qsmxt.scripts.logger import LogLevel, make_logger, show_warning_summary 

def copy(old, new, always_show=False):
    logger = make_logger()
    if always_show or not sys.__stdin__.isatty():
        logger.log(LogLevel.INFO.value, f'Copying {old} -> {new}')
    if not os.path.exists(os.path.split(new)[0]):
        os.makedirs(os.path.split(new)[0], exist_ok=True)
    shutil.copy2(old, new)


def load_json(path):
    with open(path, encoding='utf-8') as f:
        j = json.load(f)
    return j


def json_filename(nifti_filename):
    return nifti_filename.split('.')[0] + '.json'


def parse_num_or_exit(string, error_message, whole_number=False):
    logger = make_logger()
    try:
        return int(string) if whole_number else float(string)
    except:
        logger.log(LogLevel.ERROR.value, error_message)
        script_exit(1)


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
    logger = make_logger()
    csv_contents = []
    with open(csv_file, "r", encoding='utf-8') as f:
        for line in f:
            line_contents = line.replace("\n", "").split(",")
            if len(line_contents) != 10:
                logger.log(LogLevel.ERROR.value, 'Incorrect number of columns in CSV! Delete it or correct it and try again.')
                script_exit(1)
            if '' in line_contents:
                logger.log(LogLevel.ERROR.value, 'CSV file incomplete! Delete it or complete it and try again.')
                script_exit(1)
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
            error_message=f"Could not parse run number '{line_contents[3]}' on line {i+1} as int",
            whole_number=True
        )
        
        details['echo_num'] = parse_num_or_exit(
            line_contents[4],
            error_message=f"Could not parse echo number '{line_contents[4]}' on line {i+1} as int",
            whole_number=True
        )

        details['echo_time'] = parse_num_or_exit(
            line_contents[5],
            error_message=f"Could not parse echo time '{line_contents[5]}' on line {i+1} as float",
            whole_number=False
        )
        
        details['field_strength'] = parse_num_or_exit(
            line_contents[7],
            error_message=f"Could not parse field strength '{line_contents[7]}' on line {i+1} as float",
            whole_number=False
        )
        
        if details['multi-echo'] not in ['yes', 'no']:
            logger.log(LogLevel.ERROR.value, f"Could not parse multi-echo field contents '{details['multi-echo']}' on line {i+1} as 'yes' or 'no'")
            script_exit(1)

        if details['part_type'] not in ['phase', 'mag']:
            logger.log(LogLevel.ERROR.value, f"Could not parse part type field contents '{details['part_type']}' on line {i+1} as 'mag' or 'phase'")
            script_exit(1)


        all_details.append(details)
        
    return all_details


def get_details_from_filenames(file_list, args):
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


def update_details_with_jsons(all_details, args):
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


def write_details_to_csv(all_details, csv_file):
    with open(csv_file, 'w', encoding='utf-8') as f:
        f.write('filename,subject id,session id,run number,echo number,echo_time (s),multi-echo (yes or no),field_strength (T),series_type (t2starw or t1w),part_type (mag or phase)\n')
        all_details.sort(key=lambda d: d['filename'])
        for d in all_details:
            line = f"{d['filename']},{d['subject_id']},{d['session_id']},{d['run_num']},{d['echo_num']},{d['echo_time']},{d['multi-echo']},{d['field_strength']},{d['series_type']},{d['part_type']}\n"
            line = line.replace(",None", ",").replace("None,", ",")
            f.write(line)


def nifti_convert(input_dir, output_dir, args):
    logger = make_logger()
    if os.path.exists(args.csv_file):
        logger.log(LogLevel.INFO.value, f"CSV spreadsheet '{args.csv_file}' found! Reading...")
        all_details = get_details_from_csv(args.csv_file)
        logger.log(LogLevel.INFO.value, f"CSV spreadsheet loaded.")
    else:
        logger.log(LogLevel.INFO.value, f"Finding NIfTI files...")
        nifti_files = find_files_with_extension(args.input_dir, ['.nii', '.nii.gz'])
        logger.log(LogLevel.INFO.value, f"{len(nifti_files)} NIfTI files found.")
        logger.log(LogLevel.INFO.value, f"Extracting details from filenames using patterns...")
        all_details = get_details_from_filenames(nifti_files, args)
        logger.log(LogLevel.INFO.value, f"Done reading details.")
        logger.log(LogLevel.INFO.value, f"Updating details with JSON header information...")
        all_details = update_details_with_jsons(all_details, args)
        logger.log(LogLevel.INFO.value, f"Done reading JSON header files.")

    if any(value is None for value in flatten([list(details.values()) for details in all_details])):
        logger.log(LogLevel.INFO.value, f"Some information is missing! Writing all details to CSV spreadsheet '{args.csv_file}'...")
        write_details_to_csv(all_details)
        logger.log(LogLevel.INFO.value, f"Done writing to CSV.")
        logger.log(LogLevel.INFO.value, f"PLEASE FILL IN SPREADSHEET '{args.csv_file}' WITH MISSING INFORMATION AND RUN AGAIN WITH THE SAME COMMAND.")
        script_exit()

    logger.log(LogLevel.INFO.value, "Computing new NIfTI file names and locations...")
    for details in all_details:
        ext = 'nii.gz' if details['filename'].endswith('nii.gz') else 'nii'
        if details['series_type'] == 't1w':
            details['new_name'] = os.path.join(args.output_dir, f"sub-{details['subject_id']}", f"ses-{details['session_id']}", "anat", f"sub-{details['subject_id']}_ses-{details['session_id']}_run-{str(details['run_num']).zfill(2)}_T1w.{ext}")
        elif details['multi-echo'] and details['multi-echo'].lower() == 'no':
            details['new_name'] = os.path.join(args.output_dir, f"sub-{details['subject_id']}", f"ses-{details['session_id']}", "anat", f"sub-{details['subject_id']}_ses-{details['session_id']}_run-{str(details['run_num']).zfill(2)}_part-{details['part_type']}_T2starw.{ext}")
        else:
            details['new_name'] = os.path.join(args.output_dir, f"sub-{details['subject_id']}", f"ses-{details['session_id']}", "anat", f"sub-{details['subject_id']}_ses-{details['session_id']}_run-{str(details['run_num']).zfill(2)}_echo-{str(details['echo_num']).zfill(2)}_part-{details['part_type']}_MEGRE.{ext}")
    logger.log(LogLevel.INFO.value, "New NIfTI file names and locations determined.")

    # if running interactively, show a summary of the renames prior to actioning
    if sys.__stdin__.isatty() and not args.auto_yes:
        print("Summary of identified files and proposed new names (following BIDS standard):")
        for f in all_details:
            print(f"{os.path.split(f['filename'])[1]} \n\t -> {os.path.split(f['new_name'])[1]}")
        print("Confirm copy + renames? (n for no): ")
        if input().strip().lower() in ["n", "no"]:
            script_exit()

    # copy/rename all files
    logger.log(LogLevel.INFO.value, "Copying NIfTI files to new locations with new names...")
    for details in all_details:
        copy(details['filename'], details['new_name'], always_show=args.auto_yes)
    logger.log(LogLevel.INFO.value, "Done copying NIfTI files.")

    logger.log(LogLevel.INFO.value, "Copying JSON header files if present and generating them if needed...")
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
                "ProtocolName" : details['series_type'],
                "ConversionSoftware" : "dcm2niix"
            }
            with open(json_filename(details['new_name']), 'w', encoding='utf-8') as json_file:
                json.dump(dictionary, json_file)
            logger.log(LogLevel.INFO.value, f"Automatically generated JSON header file '{json_filename(details['new_name'])}'")
    logger.log(LogLevel.INFO.value, "Done copying and generating JSON header files.")

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
    with open(os.path.join(args.output_dir, 'dataset_description.json'), 'w', encoding='utf-8') as dataset_json_file:
        json.dump(dataset_description, dataset_json_file)

    logger.log(LogLevel.INFO.value, 'Writing BIDS .bidsignore file...')
    with open(os.path.join(args.output_dir, '.bidsignore'), 'w', encoding='utf-8') as bidsignore_file:
        bidsignore_file.write('references.txt\n')
        bidsignore_file.write('dataset_qsmxt.csv\n')

    logger.log(LogLevel.INFO.value, 'Writing BIDS dataset README...')
    with open(os.path.join(args.output_dir, 'README'), 'w', encoding='utf-8') as readme_file:
        readme_file.write(f"Generated using QSMxT ({get_qsmxt_version()})\n")
        readme_file.write(f"\nDescribe your dataset here.\n")

def script_exit(exit_code=0):
    logger = make_logger()
    show_warning_summary(logger)
    logger.log(LogLevel.INFO.value, 'Finished')
    exit(exit_code)

def main():
    parser = argparse.ArgumentParser(
        description="QSMxT niftiConvert: Sorts NIfTI files into BIDS for use with QSMxT",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        'input_dir',
        help='Input NIfTI directory to be recursively searched for NIfTI files.'
    )

    parser.add_argument(
        'output_dir',
        help='Output BIDS directory.'
    )

    parser.add_argument(
        '--magnitude_pattern',
        type=str,
        default='*mag*',
        help='Pattern used to identify T2*-weighted magnitude files to be used for QSM based on filenames.'
    )

    parser.add_argument(
        '--phase_pattern',
        type=str,
        default='*phase*',
        help='Pattern used to identify T2*-weighted phase files to be used for QSM based on filenames.'
    )

    parser.add_argument(
        '--t1w_pattern',
        type=str,
        default='*T1w*',
        help='Pattern used to identify T1-weighted files for segmentation purposes based on filenames.'
    )

    parser.add_argument(
        '--t1w_protocol_patterns',
        type=str,
        default=['*t1w*'],
        help='Patterns used to identify T1-weighted files for segmentation purposes based on the \'ProtocolName\' in adjacent JSON headers.'
    )

    parser.add_argument(
        '--t2starw_protocol_patterns',
        type=str,
        default=['*qsm*', '*t2starw*'],
        help='Patterns used to identify T2*-weighted files to be used for QSM based on the \'ProtocolName\' in adjacent JSON headers.'
    )

    parser.add_argument(
        '--subject_pattern',
        type=str,
        default='sub-([^_/\\\\]+)',
        help='Regular expression to capture the subject ID from NIfTI filepaths.'
    )

    parser.add_argument(
        '--session_pattern',
        type=str,
        default='ses-([^_/\\\\]+)',
        help='Regular expression to capture the session ID from NIfTI filepaths.'
    )

    parser.add_argument(
        '--protocol_pattern',
        type=str,
        default=None,
        help='Regular expression to capture the \'ProtocolName\' from NIfTI filepaths (used in place of JSON headers if unavailable).'
    )

    parser.add_argument(
        '--run_pattern',
        type=str,
        default='run-([0-9]+)',
        help='Regular expression to capture the run number from NIfTI filepaths (one scanning session may have multiple runs of the same sequence).'
    )

    parser.add_argument(
        '--echo_pattern',
        type=str,
        default='echo-([0-9]+)',
        help='Regular expression to capture the echo number from NIfTI filepaths.'
    )

    parser.add_argument(
        '--auto_yes',
        action='store_true',
        help='Force running non-interactively. This is useful when used as part of a script or on a testing server.'
    )

    args = parser.parse_args()

    args.input_dir = os.path.abspath(args.input_dir)
    args.output_dir = os.path.abspath(args.output_dir)
    args.csv_file = os.path.join(args.output_dir, 'dataset_qsmxt.csv')

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

    with open(os.path.join(args.output_dir, "references.txt"), 'w', encoding='utf-8') as f:
        # output QSMxT version, run command, and python interpreter
        f.write(f"QSMxT: {get_qsmxt_version()}")
        f.write(f"\nRun command: {str.join(' ', sys.argv)}")
        f.write(f"\nPython interpreter: {sys.executable}")

        f.write("\n\n == References ==")

        f.write("\n\n - Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT")
        f.write("\n\n - Gorgolewski KJ, Auer T, Calhoun VD, et al. The brain imaging data structure, a format for organizing and describing outputs of neuroimaging experiments. Sci Data. 2016;3(1):160044. doi:10.1038/sdata.2016.44")
        f.write("\n\n")

    nifti_convert(
        input_dir=args.input_dir,
        output_dir=args.output_dir,
        args=args
    )

    script_exit()

