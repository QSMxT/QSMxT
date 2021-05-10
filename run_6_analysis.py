#!/usr/bin/env python3

import matplotlib
import matplotlib.pyplot as plt
import seaborn as sns
import numpy as np
import nibabel as nib
import argparse
import os
from glob import glob

# get labels dictionary by parsing a labels CSV file
def load_labels(label_filepath):
    # read label file
    label_file = open(label_filepath)
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
def update_labels(labels, segmentation):
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

if __name__ == "__main__":

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
        '--out_dir',
        help='Output directory to write the quantitative data to.'
    )

    parser.add_argument(
        '--labels_file',
        default=None,
        help='Optional labels CSV file to include named fields in the output. The CSV should contain '+
             'segmentation numbers in the first column and ROI names in the second. The aseg_labels.csv '+
             'file contains labels for the aseg atlas used in the segmentation pipeline.'
    )

    args = parser.parse_args()

    # ensure directories are complete and absolute
    args.out_dir = os.path.abspath(args.out_dir)
    os.makedirs(os.path.abspath(args.out_dir), exist_ok=True)

    files_qsm = sorted(args.qsm_files)
    if args.labels_file:
        args.labels_file = os.path.abspath(args.labels_file)
        labels_orig = load_labels(args.labels_file)
    else:
        labels_orig = {}

    # for each segmentation file
    if len(args.segmentations) > 1:
        files_seg = sorted(args.segmentations)
        for i in range(len(files_seg)):
            print(f"Analysing file {os.path.split(files_qsm[i])[-1]} with segmentation {os.path.split(files_seg[i])[-1]}")

            # load subject and segmentation data
            nii_seg = nib.load(files_seg[i])
            nii_qsm = nib.load(files_qsm[i])
            seg = nii_seg.get_fdata().flatten()
            qsm = nii_qsm.get_fdata().flatten()

            # update labels with this segmentation
            labels = labels_orig.copy()
            update_labels(labels, seg)

            # get statistics for each label name
            label_stats = get_stats(labels, seg, qsm)

            # write header to file
            f_name = (files_seg[i].split('/')[-1]).replace('.nii.gz', '.nii').replace('.nii', '.csv')
            f = open(os.path.join(args.out_dir, f_name), 'w')
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
    else:
        # single segmentation file
        nii_seg = nib.load(args.segmentations[0])
        seg = nii_seg.get_fdata().flatten()

        # update labels with this segmentation
        labels = labels_orig.copy()
        update_labels(labels, seg)

        # write header to file
        f_name = os.path.split(args.segmentations[0])[1].replace('.nii.gz', '.nii').replace('.nii', '.csv')
        f = open(os.path.join(args.out_dir, f_name), 'w')
        f.write('subject,roi,num_voxels,min,max,median,mean,std\n')
        
        # for each subject
        for i in range(len(files_qsm)):

            # load the data
            nii_qsm = nib.load(files_qsm[i])
            qsm = nii_qsm.get_fdata().flatten()

            # get statistics for each label name
            label_stats = get_stats(labels, seg, qsm)

            # write data to file
            for label_name in labels.keys():
                line = [os.path.split(files_qsm[i])[1], label_name]
                line.extend(label_stats[label_name])
                line = ",".join([str(x) for x in line])
                f.write(line)
                f.write('\n')

        # close file
        f.close()

