#!/usr/bin/env python3

# Adapted from Alex Weston
# Digital Innovation Lab, Mayo Clinic
# https://gist.github.com/alex-weston-13/4dae048b423f1b4cb9828734a4ec8b83

import argparse
import os
import sys
import pydicom  # pydicom is using the gdcm package for decompression
import datetime
import re
import shutil

from qsmxt.scripts.qsmxt_functions import get_qsmxt_version, get_diff
from qsmxt.scripts.logger import LogLevel, make_logger, show_warning_summary

def empty_dirs(root_dir='.', recursive=True):
    empty_dirs = []
    for root, dirs, files in os.walk(root_dir, topdown=False):
        #print root, dirs, files
        if recursive:
            all_subs_empty = True  # until proven otherwise
            for sub in dirs:
                full_sub = os.path.join(root, sub)
                if full_sub not in empty_dirs:
                    #print full_sub, "not empty"
                    all_subs_empty = False
                    break
        else:
            all_subs_empty = (len(dirs) == 0)
        if all_subs_empty and len(files) == 0:
            empty_dirs.append(root)
            yield root

def find_empty_dirs(root_dir='.', recursive=True):
    return list(empty_dirs(root_dir, recursive))

def clean_text(string):
    return re.sub(r'[^a-zA-Z0-9_-]', '_', string)

def find_dicoms(input_dir, check_all_files):
    logger = make_logger()
    unsortedList = []
    for root, dirs, files in os.walk(input_dir):
        for f in files:
            file_extension = ".".join(f.split('.')[-1:])
            if file_extension.lower() in ['ima', 'dcm']:
                unsortedList.append(os.path.join(root, f))
    if not unsortedList or check_all_files:
        if not unsortedList:
            logger.log(LogLevel.WARNING.value, "No .IMA or .dcm files found! Checking all files for valid DICOM headers...")
        for root, dirs, files in os.walk(input_dir):
            for f in files:
                try:
                    ds = pydicom.read_file(os.path.join(root, f), force=True)
                    if len(ds) > 1:
                        unsortedList.append(os.path.join(root, f))
                except:
                    pass
    return unsortedList

def dicomsort(input_dir, output_dir, use_patient_names, use_session_incrementer, check_all_files, delete_originals):
    logger = make_logger()
    os.makedirs(output_dir, exist_ok=True)
    logger.log(LogLevel.INFO.value, "Reading file list...")
    unsortedList = find_dicoms(input_dir, check_all_files)
    logger.log(LogLevel.INFO.value, f"{len(unsortedList)} DICOM files found.")
    fail = False

    subjName_dates = []
    subjName_sessionNums = {}

    logger.log(LogLevel.INFO.value, f"Sorting DICOMs in {output_dir}...")
    for dicom_loc in unsortedList:
        # read the file
        try:
            ds = pydicom.read_file(dicom_loc, force=True)
        except:
            logger.log(LogLevel.WARNING.value, f"Failed to read file as DICOM: {dicom_loc}. Skipping...")
            continue
    
        # get patient, study, and series information
        patientName = clean_text(str(ds.get("PatientName", "NA")))
        patientID = clean_text(ds.get("PatientID", "NA"))
        studyDate = clean_text(ds.get("StudyDate", "NA"))
        studyDescription = clean_text(ds.get("StudyDescription", "NA"))
        protocolName = clean_text(ds.get("ProtocolName", "NA"))
        seriesNumber = clean_text(str(ds.get("SeriesNumber", "NA")))
        
        # generate new, standardized file name
        modality = ds.get("Modality","NA")
        studyInstanceUID = ds.get("StudyInstanceUID","NA")
        seriesInstanceUID = ds.get("SeriesInstanceUID","NA")
        instanceNumber = str(ds.get("InstanceNumber","0"))
        fileName = modality + "." + seriesInstanceUID + "." + instanceNumber + ".dcm"

        subj_name = patientName if use_patient_names else patientID
        subj_name = subj_name.replace('-', '').replace('_', '')
        subj_name = subj_name.replace('-', '').replace('_', '')
        if not subj_name:
            subj_name = "NA"
        
        # save files to a 3-tier nested folder structure
        subjFolderName = f"sub-{subj_name}"
        seriesFolderName = f"series-{seriesNumber}_{protocolName}"
        subjName_date = f"{subj_name}_{studyDate}"

        if not any(subj_name in x for x in subjName_dates):
            logger.log(LogLevel.INFO.value, f"Identified subject: {subj_name}")
        
        if subjName_date not in subjName_dates:
            subjName_dates.append(subjName_date)
            if subj_name in subjName_sessionNums.keys():
                subjName_sessionNums[subj_name] += 1
            else:
                subjName_sessionNums[subj_name] = 1
            logger.log(LogLevel.INFO.value, f"Identified session: {subj_name} #{subjName_sessionNums[subj_name]} {studyDate}")

        sesFolderName = f"ses-{subjName_sessionNums[subj_name]}" if (use_session_incrementer or studyDate == "") else f"ses-{studyDate}"
        
        if not os.path.exists(os.path.join(output_dir, subjFolderName, sesFolderName, seriesFolderName)):
            logger.log(LogLevel.INFO.value, f"Identified series: {subjFolderName}/{sesFolderName}/{seriesFolderName}")
            os.makedirs(os.path.join(output_dir, subjFolderName, sesFolderName, seriesFolderName), exist_ok=True)

        # copy the file to the new location, retaining all metadata
        shutil.copy2(dicom_loc, os.path.join(output_dir, subjFolderName, sesFolderName, seriesFolderName, fileName))

        if not os.path.exists(os.path.join(output_dir, subjFolderName, sesFolderName, seriesFolderName, fileName)):
            fail = True

    if not fail and delete_originals:
        for dicom_loc in unsortedList:
            os.remove(dicom_loc)


def main():
    parser = argparse.ArgumentParser(
        description="QSMxT dicomSort: Sorts DICOM files into a folder structure of the form <out_dir>/<Subject>/<Session>/<Series>",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        'input_dir',
        help='Input DICOM directory to be recursively searched for DICOM files.'
    )

    parser.add_argument(
        'output_dir',
        help='Output directory to house sorted DICOMs.'
    )

    parser.add_argument(
        '--use_patient_names',
        action='store_true',
        help='Use the DICOM \'PatientName\' field rather than \'PatientID\' to identify subjects.'
    )

    parser.add_argument(
        '--use_session_incrementer',
        action='store_true',
        help='Use an incrementer to identify scanning sessions rather than the \'SessionDate\' DICOM field.'
    )

    parser.add_argument(
        '--check_all_files',
        action='store_true',
        help='Ignores the DICOM file extensions .dcm and .IMA and instead reads all files for valid DICOM headers. '+
             'This is useful if some of your DICOM files have unusual file extensions or none at all.'
    )

    parser.add_argument(
        '--delete_originals',
        action='store_true',
        help='Delete the original DICOM files and folders after successfully sorting; by ' +
             'default this is on when input_dir == output_dir'
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

    with open(os.path.join(args.output_dir, "references.txt"), 'w', encoding='utf-8') as f:
        # output QSMxT version, run command, and python interpreter
        f.write(f"QSMxT: {get_qsmxt_version()}")
        f.write(f"\nRun command: {str.join(' ', sys.argv)}")
        f.write(f"\nPython interpreter: {sys.executable}")

        f.write("\n\n == References ==")

        # qsmxt, pydicom, sort_dicoms.py
        f.write("\n\n - Stewart AW, Robinson SD, O'Brien K, et al. QSMxT: Robust masking and artifact reduction for quantitative susceptibility mapping. Magnetic Resonance in Medicine. 2022;87(3):1289-1300. doi:10.1002/mrm.29048")
        f.write("\n\n - Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT")
        f.write("\n\n - Mason D, scaramallion, mrbean-bremen, et al. Pydicom/Pydicom: Pydicom 2.3.0. Zenodo; 2022. doi:10.5281/zenodo.6394735")
        f.write("\n\n - Weston A. alex-weston-13/sort_dicoms.py. GitHub; 2020. https://gist.github.com/alex-weston-13/4dae048b423f1b4cb9828734a4ec8b83")
        f.write("\n\n")

    dicomsort(
        input_dir=args.input_dir,
        output_dir=args.output_dir,
        use_patient_names=args.use_patient_names,
        use_session_incrementer=args.use_session_incrementer,
        check_all_files=args.check_all_files,
        delete_originals=args.input_dir == args.output_dir or args.delete_originals
    )

    show_warning_summary(logger)

    logger.log(LogLevel.INFO.value, 'Finished')
    
