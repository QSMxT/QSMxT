#!/bin/python
import argparse
import os
import subprocess

if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="QSMxT DICOM to BIDS converter",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        'dicom',
        help='dicom data folder'
    )

    parser.add_argument(
        'bids',
        help='bids data folder'
    )

    parser.add_argument(
        '--heuristic',
        default=os.path.join(os.path.abspath(os.path.dirname(os.path.abspath(__file__))),'bidsmap.yaml'),
        const='bidsmap.yaml',
        nargs='?',
        help='bidsmap.yaml heuristic file'
    )

    args = parser.parse_args()

    script_dir = os.path.abspath(os.path.dirname(os.path.abspath(__file__)))
    dicom_dir = os.path.abspath(args.dicom)
    bids_dir = os.path.abspath(args.bids)
    heuristic_path = os.path.abspath(args.heuristic)
    
    if not os.path.exists(dicom_dir):
        print(f"QSMxT: Error: DICOM path does not exist: {dicom_dir}")
        exit()

    os.makedirs(bids_dir, exist_ok=True)

    if len(os.listdir(bids_dir)) > 0:
        print(f"QSMxT: Warning: BIDS path is not empty: {bids_dir}")

    subprocess.call(f"bidsmapper -b {heuristic_path} -i 0 {dicom_dir} {bids_dir}", executable='/bin/bash', shell=True)
    subprocess.call(f"bidscoiner -b {heuristic_path} {dicom_dir} {bids_dir}", executable='/bin/bash', shell=True)
    bids_subject_dirs = list(set(os.listdir(bids_dir)) & set(os.listdir(dicom_dir)))

