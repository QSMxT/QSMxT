#!/usr/bin/env python3
import argparse
import os

from bidscoin.bidscoiner import bidscoiner
from bidscoin.bidsmapper import bidsmapper

if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="QSMxT dicomToBids: Converts a sorted DICOM folder to BIDS",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        'dicom',
        help='input DICOM data folder; should be sorted using run_0_dicomSort.py'
    )

    parser.add_argument(
        'bids',
        help='output BIDS data folder; will be created if it does not exist'
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

    bidsmapper(rawfolder    = dicom_dir,
               bidsfolder   = bids_dir,
               bidsmapfile  = heuristic_path,
               templatefile = heuristic_path,
               interactive  = False)

    bidscoiner(rawfolder    = dicom_dir,
               bidsfolder   = bids_dir,
               bidsmapfile  = heuristic_path)

