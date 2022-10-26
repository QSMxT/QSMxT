#!/usr/bin/env python3

import numpy as np
import nibabel as nib
import argparse
import os
import sys
import datetime

from scripts.qsmxt_functions import get_qsmxt_version
from scripts.logger import LogLevel, make_logger, show_warning_summary

# get labels dictionary by parsing a labels CSV file
def load_labels(label_filepath):
    # read label file
    label_file = open(label_filepath, encoding='utf-8')
    lines = label_file.readlines()
    label_file.close()

    # get all labels numbers and names
    label_nums = []
    label_names = []
    for line in lines:
        label_num, label_name = line.strip().split(',')
        label_nums.append(label_num)
        label_names.append(label_name)

    # fill labels dictionary -- e.g. { 'Accumbens' : [2, 13] }
    labels = {}
    for label_name in sorted(list(set(label_names))):
        labels[label_name] = []
        for i in range(len(label_names)):
            if label_names[i] == label_name:
                labels[label_name].append(int(label_nums[i]))
    
    return labels

# give names to segmentation labels that don't have one
def update_labels(labels, seg):
    for seg_num in sorted(list(set(seg))):
        # get segmentation name 
        if seg_num == 0: continue
        seg_name = None
        for label_name in labels.keys():
            if seg_num in labels[label_name]:
                seg_name = label_name
                break
        if not seg_name: labels[str(int(seg_num))] = [int(seg_num)]

# get statistics for each label based on segmentations and qsm data
def get_stats(labels, seg, qsm):
    label_stats = {}
    for label_name in labels.keys():
        # get qsm values for this label
        qsm_seg = np.zeros_like(seg)
        for label_id in labels[label_name]:
            qsm_seg = np.logical_or(qsm_seg, seg == label_id)
        qsm_values = qsm[np.logical_and(qsm != 0, qsm_seg)]

        # skip if no values
        if len(qsm_values) == 0:
            label_stats[label_name] = []
            continue

        # get statistics and store them
        num_voxels = len(qsm_values)
        min_v = np.min(qsm_values)
        max_v = np.max(qsm_values)
        median = np.median(qsm_values)
        mean = np.mean(qsm_values)
        std  = np.std(qsm_values)
        label_stats[label_name] = [num_voxels, min_v, max_v, median, mean, std]
    return label_stats

def parse_args():
    parser = argparse.ArgumentParser(
        description="QSMxT qsm: QSM Reconstruction Pipeline",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        '--segmentations',
        nargs='+',
        required=True,
        help='Segmentation files to use. This can be either a single segmentation file, or one for each result in the QSM directory.'
    )

    parser.add_argument(
        '--qsm_files',
        nargs='+',
        required=True,
        help='QSM files to analyse using the segmentation/s.'
    )

    parser.add_argument(
        '--output_dir',
        help='Output directory to write the quantitative data to.'
    )

    parser.add_argument(
        '--labels_file',
        default=None,
        help='Optional labels CSV file to include named fields in the output. The CSV should contain '+
             'segmentation numbers in the first column and ROI names in the second. The aseg_labels.csv '+
             'file contains labels for the aseg atlas used in the segmentation pipeline.'
    )

    return parser.parse_args()

def check_output_dir():
    args.output_dir = os.path.abspath(args.output_dir)
    os.makedirs(os.path.abspath(args.output_dir), exist_ok=True)

def init_logger():
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
    return logger

# write "details_and_citations.txt" with the command used to invoke the script and any necessary citations
def write_details_and_citations():
    with open(os.path.join(args.output_dir, "details_and_citations.txt"), 'w', encoding='utf-8') as f:
        # output QSMxT version, run command, and python interpreter
        f.write(f"QSMxT: {get_qsmxt_version()}")
        f.write(f"\nRun command: {str.join(' ', sys.argv)}")
        f.write(f"\nPython interpreter: {sys.executable}")

        f.write("\n\n == References ==")

        # qsmxt, nibabel
        f.write("\n\n - Stewart AW, Robinson SD, O'Brien K, et al. QSMxT: Robust masking and artifact reduction for quantitative susceptibility mapping. Magnetic Resonance in Medicine. 2022;87(3):1289-1300. doi:10.1002/mrm.29048")
        f.write("\n\n - Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT")
        f.write("\n\n - Brett M, Markiewicz CJ, Hanke M, et al. nipy/nibabel. GitHub; 2019. https://github.com/nipy/nibabel")
        f.write("\n\n - Harris CR, Millman KJ, van der Walt SJ, et al. Array programming with NumPy. Nature. 2020;585(7825):357-362. doi:10.1038/s41586-020-2649-2")
        f.write("\n\n")

def get_labels():
    if args.labels_file:
        args.labels_file = os.path.abspath(args.labels_file)
        labels_orig = load_labels(args.labels_file)
    else:
        labels_orig = {}
    return labels_orig

def load_nii_as_array(file):
    nii = nib.load(file)
    return nii.get_fdata().flatten()

def one_segmentation_per_subject():
    files_qsm = sorted(args.qsm_files)
    files_seg = sorted(args.segmentations)
    labels_orig = get_labels()
    for i in range(len(files_seg)):
        logger.log(LogLevel.INFO.value, f"Analysing file {os.path.split(files_qsm[i])[-1]} with segmentation {os.path.split(files_seg[i])[-1]}")

        seg = load_nii_as_array(files_seg[i])
        qsm = load_nii_as_array(files_qsm[i])

        # update labels with this segmentation
        labels = labels_orig.copy()
        update_labels(labels, seg)

        # get statistics for each label name
        label_stats = get_stats(labels, seg, qsm)

        # write header to file
        f_name = (files_seg[i].split('/')[-1]).split('.')[0] + '.csv'
        f = open(os.path.join(args.output_dir, f_name), 'w', encoding='utf-8')
        f.write('roi,num_voxels,min,max,median,mean,std\n')

        # write data to file
        for label_name in labels.keys():
            line = [label_name]
            line.extend(label_stats[label_name])
            line = ",".join([str(x) for x in line])
            f.write(line)
            f.write('\n')

        # close file
        f.close()

def same_segmentation_for_all_subjects():
    files_qsm = sorted(args.qsm_files)
    labels_orig = get_labels()
    
    # single segmentation file
    seg = load_nii_as_array(args.segmentations[0])
    
    # update labels with this segmentation
    labels = labels_orig.copy()
    update_labels(labels, seg)

    # write header to file
    f_name = os.path.split(args.segmentations[0])[1].split('.')[0] + '.csv'
    f = open(os.path.join(args.output_dir, f_name), 'w', encoding='utf-8')
    f.write('subject,roi,num_voxels,min,max,median,mean,std\n')
    
    # for each subject
    for i in range(len(files_qsm)):
        qsm = load_nii_as_array(files_qsm[i])

        # get statistics for each label name
        label_stats = get_stats(labels, seg, qsm)

        # write data to file
        for label_name in labels.keys():
            line = [os.path.split(files_qsm[i])[1], label_name]
            line.extend(label_stats[label_name])
            line = ",".join([str(x) for x in line])
            f.write(line)
            f.write('\n')
    f.close()

def calculate_statistics():
    if len(args.segmentations) > 1:
        one_segmentation_per_subject()
    else:
        same_segmentation_for_all_subjects()


if __name__ == "__main__":
    args = parse_args()
    check_output_dir()
    logger = init_logger()
    write_details_and_citations()
    
    calculate_statistics()
    
    show_warning_summary(logger)
    logger.log(LogLevel.INFO.value, 'Finished')
