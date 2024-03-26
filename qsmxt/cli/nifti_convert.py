#!/usr/bin/env python3

import argparse
import os
import sys
import csv
import json
import shutil
import datetime

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
    head, filename, ext = splitext(nifti_filename)
    return os.path.join(head, filename + '.json')


def flatten(a):
    return [i for g in a for i in g]

def get_bids_entities():
    return ['sub', 'ses', 'acq', 'ce', 'rec', 'run', 'mod', 'echo', 'flip', 'inv', 'mt', 'part', 'desc', 'suffix']

def find_files_with_extension(input_dir, extension):
    file_list = []
    for root, dirs, files in os.walk(input_dir):
        for f in files:
            if (isinstance(extension, list) and any(f.endswith(a) for a in extension)):
                file_list.append(os.path.join(root, f))
            elif isinstance(extension, str) and f.endswith(extension):
                file_list.append(os.path.join(root, f))
    return file_list

def splitext(path):
    head, tail = os.path.split(path)

    if tail.endswith('.nii.gz'):
        ext = '.nii.gz'
        filename = tail.split('.nii.gz')[0]
    else:
        filename, ext = os.path.splitext(tail)

    return head, filename, ext


import csv

def get_details_from_csv(csv_file):
    logger = make_logger()
    all_details = []
    
    with open(csv_file, "r", encoding='utf-8') as f:
        csv_reader = csv.DictReader(f)
        
        for i, line_contents in enumerate(csv_reader):
            details = {}
            details['filedir'] = line_contents['filedir']
            details['filename'] = line_contents['filename']

            for field in line_contents:
                if line_contents[field]: details[field] = line_contents[field]

            all_details.append(details)

    return all_details


def get_bids_entity(path, entity):
    head, filename, ext = splitext(path)
    entity_pairs = filename.split('_')
    if entity == 'suffix':
        return entity_pairs[-1]
    for entity_pair in entity_pairs:
        entity_pair_tuple = entity_pair.split('-')
        if len(entity_pair_tuple) == 2:
            entity_i, label_i = entity_pair_tuple
            if entity_i == entity:
                return label_i

    return None


def get_details_from_filenames(file_list):
    all_details = []

    for nifti_file in file_list:
        details = {}
        head, filename, ext = splitext(nifti_file)
        details['filename'] = filename + ext
        details['filedir'] = head

        for entity in get_bids_entities():
            label = get_bids_entity(path=nifti_file, entity=entity)
            if label:
                details[entity] = label

        all_details.append(details)

    return all_details


def update_details_with_jsons(all_details):
    for details in all_details:
        json_file = json_filename(os.path.join(details['filedir'], details['filename']))
        if os.path.exists(json_file):
            json_data = load_json(json_file)
            for field in ['MagneticFieldStrength', 'EchoTime', 'ImageType']:
                if field in json_data:
                    details[field] = json_data[field]
    return all_details


def write_details_to_csv(all_details, csv_file):
    bids_entities = get_bids_entities()
    json_fields = ['MagneticFieldStrength', 'EchoTime']
    all_fields = ['filedir', 'filename', 'DerivativePipeline'] + bids_entities + json_fields

    with open(csv_file, 'w', encoding='utf-8', newline='') as f:
        csv_writer = csv.DictWriter(f, fieldnames=all_fields)
        
        # Write header
        csv_writer.writeheader()
        
        # Sort all_details by filename
        all_details.sort(key=lambda d: (d['filedir'], d['filename']))
        
        # Write rows
        for d in all_details:
            row_dict = {}
            for field in all_fields:
                if field == 'ImageType':
                    if 'part' not in all_fields:
                        row_dict['part'] = 'phase' if any(x.upper() in d.get(field, '') for x in ['P', 'PHASE']) else 'mag'
                if field == 'suffix':
                    if 'part' not in d and d.get(field, '') == 'ph':
                        row_dict['part'] = 'phase'
                if field == 'EchoNumber':
                    if 'echo' not in all_fields:
                        row_dict['echo'] = d.get(field, '')
                row_dict[field] = d.get(field, '')  # If the key doesn't exist in `d`, it will return an empty string
            csv_writer.writerow(row_dict)


def nifti_convert(args):
    logger = make_logger()
    if os.path.exists(args.csv_file):
        logger.log(LogLevel.INFO.value, f"CSV spreadsheet '{args.csv_file}' found! Reading...")
        all_details = get_details_from_csv(args.csv_file)
        logger.log(LogLevel.INFO.value, f"CSV spreadsheet loaded.")
    else:
        logger.log(LogLevel.INFO.value, f"Finding NIfTI files...")
        nifti_files = find_files_with_extension(args.input_dir, ['.nii', '.nii.gz'])
        logger.log(LogLevel.INFO.value, f"{len(nifti_files)} NIfTI files found.")
        logger.log(LogLevel.INFO.value, f"Extracting details from filenames...")
        all_details = get_details_from_filenames(nifti_files)
        logger.log(LogLevel.INFO.value, f"Done reading details.")
        logger.log(LogLevel.INFO.value, f"Updating details with JSON header information...")
        all_details = update_details_with_jsons(all_details)
        logger.log(LogLevel.INFO.value, f"Done reading JSON header files.")
        write_details_to_csv(all_details, args.csv_file)
        logger.log(LogLevel.INFO.value, f"RUN AGAIN AFTER ENTERING RELEVANT BIDS INFORMATION TO {args.csv_file}.")
        script_exit(0)
    
    logger.log(LogLevel.INFO.value, "Computing new NIfTI file names and locations...")
    for details in all_details:
        filedir = details['filedir']
        filename = details['filename']
        _, filename, ext = splitext(filename)
        filename = os.path.join(filedir, filename)

        if any(x not in details for x in ['sub', 'suffix']):
            logger.log(LogLevel.ERROR.value, f"File '{filename + ext}' is missing BIDS-critical information! At least 'sub' and 'suffix' entities are required!")
            script_exit(1)
        
        new_name = ""

        for entity in get_bids_entities():
            if entity in details:
                new_name += "_" if new_name else ""
                new_name += f"{entity}-{details[entity]}" if entity != 'suffix' else details[entity]

        new_name += ext

        new_dir = os.path.join(args.output_dir)
        if 'DerivativePipeline' in details:
            new_dir = os.path.join(new_dir, 'derivatives', details['DerivativePipeline'])
        new_dir = os.path.join(new_dir, f"sub-{details['sub']}")
        if 'ses' in details:
            new_dir = os.path.join(new_dir, f"ses-{details['ses']}")
        new_dir = os.path.join(new_dir, "anat")
        
        details['new_name'] = os.path.join(new_dir, new_name)
    logger.log(LogLevel.INFO.value, "New NIfTI file names and locations determined.")

    print("Summary of identified files and proposed new names (following BIDS standard):")
    for f in all_details:
        print(f"{os.path.split(f['filename'])[1]} \n\t -> {os.path.split(f['new_name'])[1]}")

    if len(all_details) != len(set(details['new_name'] for details in all_details)):
        logger.log(LogLevel.ERROR.value, "Resultant BIDS data contains name conflicts! Correct CSV and run again.")
        script_exit(1)
    
    # if running interactively, show a summary of the renames prior to actioning
    if sys.__stdin__.isatty() and not args.auto_yes:
        print("Confirm copy + renames? (n for no): ")
        if input().strip().lower() in ["n", "no"]:
            script_exit()

    # copy/rename all files
    logger.log(LogLevel.INFO.value, "Copying NIfTI files to new locations with new names...")
    for details in all_details:
        copy(os.path.join(details['filedir'], details['filename']), details['new_name'], always_show=args.auto_yes)
    logger.log(LogLevel.INFO.value, "Done copying NIfTI files.")

    logger.log(LogLevel.INFO.value, "Copying JSON header files if present and generating them if needed...")
    for details in all_details:
        f = json_filename(os.path.join(details['filedir'], details['filename']))
        if os.path.exists(f):
            copy(f, json_filename(details['new_name']), always_show=args.auto_yes)
        else:
            dictionary = {}
            for field in details:
                if field not in get_bids_entities():
                    dictionary[field] = details[field]

            if 'EchoTime' in dictionary:
                dictionary['EchoTime'] = float(dictionary['EchoTime'])
            if 'MagneticFieldStrength' in dictionary:
                dictionary['MagneticFieldStrength'] = float(dictionary['MagneticFieldStrength'])
            if 'ImageType' not in dictionary and 'part' in details:
                dictionary['ImageType'] = ["P", "PHASE"] if details['part'] == 'phase' else ["M", "MAGNITUDE"]
            if 'EchoNumber' not in dictionary and 'echo' in details:
                dictionary['EchoNumber'] = int(details['echo'])
            if 'ProtocolName' not in dictionary and 'acq' in details:
                dictionary['ProtocolName'] = details['acq']
            
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
        args=args
    )

    script_exit()

