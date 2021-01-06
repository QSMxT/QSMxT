#!/bin/python
import argparse
import os
import subprocess
import sys
import glob

def which(program):
    def is_exe(fpath):
        return os.path.exists(fpath) and os.access(fpath, os.X_OK) and os.path.isfile(fpath)

    def ext_candidates(fpath):
        yield fpath
        for ext in os.environ.get("PATHEXT", "").split(os.pathsep):
            yield fpath + ext

    fpath, fname = os.path.split(program)
    if fpath:
        if is_exe(program):
            return program
    else:
        for path in os.environ["PATH"].split(os.pathsep):
            exe_file = os.path.join(path, program)
            for candidate in ext_candidates(exe_file):
                if is_exe(candidate):
                    return candidate

    return None


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="QSMxT DICOM to BIDS converter",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        'dicom',
        default='dicom',
        const='dicom',
        nargs='?',
        help='dicom data folder'
    )

    parser.add_argument(
        'bids',
        default='bids',
        const='bids',
        nargs='?',
        help='bids data folder'
    )

    parser.add_argument(
        '--heuristic',
        default='bidsmap.yaml',
        const='bidsmap.yaml',
        nargs='?',
        help='bidsmap.yaml heuristic file'
    )

    args = parser.parse_args()

    script_dir = os.path.abspath(os.path.dirname(os.path.abspath(__file__)))
    dicom_path = os.path.abspath(args.dicom)
    bids_path = os.path.abspath(args.bids)
    heuristic_path = os.path.abspath(args.heuristic)
    
    retcode = 0

    if not os.path.exists(dicom_path):
        print(f"QSMxT: Error: DICOM path does not exist: {dicom_path}")
        retcode = 1
    if not os.path.exists(bids_path):
        try:
            os.mkdir(bids_path)
        except:
            print(f"QSMxT: Error: BIDS path could not be created: {bids_path}")
            retcode = 1
    if len(os.listdir(bids_path)) > 0:
        print(f"QSMxT: Warning: BIDS path is not empty: {bids_path}")

    if retcode: sys.exit(retcode)
    retcode = subprocess.call(f"bidsmapper -b {heuristic_path} -i 0 {dicom_path} {bids_path}", executable='/bin/bash', shell=True)
    retcode = retcode + subprocess.call(f"bidscoiner {dicom_path} {bids_path}", executable='/bin/bash', shell=True)
    bids_subject_dirs = list(set(os.listdir(bids_path)) & set(os.listdir(dicom_path)))
    sys.exit(retcode)

