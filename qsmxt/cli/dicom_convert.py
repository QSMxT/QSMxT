#!/usr/bin/env python3

import argparse
import os
import sys
import subprocess
import json
import datetime
import re
import curses
import shutil
import time
import pandas as pd

from tabulate import tabulate

from dicompare import load_dicom_session

from qsmxt.scripts.qsmxt_functions import get_qsmxt_version, get_diff
from qsmxt.scripts.logger import LogLevel, make_logger, show_warning_summary 
#from qsmxt.scripts.nii_fix_ge import fix_ge_polar, fix_ge_complex

from collections import Counter

def sys_cmd(cmd):
    logger = make_logger()
    logger.log(LogLevel.INFO.value, f"Running command: '{cmd}'")
        
    process = subprocess.run(cmd, shell=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    stdout_byte = process.stdout
    stderr_byte = process.stderr

    stdout_str = stdout_byte.decode('UTF-8')
    stderr_str = str(stderr_byte.decode('UTF-8'))
    return_code = process.returncode
    
    if stdout_str:
        logger.log(LogLevel.DEBUG.value, f"Command output: '{stdout_str}'", end="")

    if return_code:
        logger.log(LogLevel.WARNING.value, f"Command '{cmd}' returned error {return_code}: '{stderr_str}'")
    
    return return_code

def load_json(path):
    with open(path, encoding='utf-8') as f:
        j = json.load(f)
    return j

def rename(old, new, always_show=False):
    if always_show or not sys.__stdin__.isatty():
        logger = make_logger()
        logger.log(LogLevel.INFO.value, f'Renaming {old} -> {new}')
    os.makedirs(os.path.split(new)[0], exist_ok=True)
    os.rename(old, new)

def clean(data): 
    cleaned = re.sub(r'[^a-zA-Z0-9]', '', data).lower()
    if data.startswith('sub-'):
        return f'sub-{cleaned[3:]}'
    elif data.startswith('ses-'):
        return f'ses-{cleaned[3:]}'
    return cleaned

def get_folders_in(folder, full_path=False):
    folders = list(filter(os.path.isdir, [os.path.join(folder, d) for d in os.listdir(folder)]))
    if full_path: return folders
    folders = [os.path.split(folder)[1] for folder in folders]
    return folders

def find_and_autoassign_qsm_pairs(row_data):
    """
    Attempt to find a single (Mag, Phase) pair. If multiple pairs, pick the first.
    If no pairs but at least 2 rows share the same Count ignoring M/P, user can fix manually.
    If no two rows share the same Count at all => "Not suitable..."
    Returns None on success or "Not suitable..." message.
    """
    mag_candidates = []
    phase_candidates = []
    for r in row_data:
        it_up = str(r['ImageType']).upper()
        if 'M' in it_up:
            mag_candidates.append(r)
        elif 'P' in it_up:
            phase_candidates.append(r)

    valid_pairs = []
    for m in mag_candidates:
        for p in phase_candidates:
            if m['Count'] == p['Count']:
                valid_pairs.append((m, p))

    if valid_pairs:
        mrow, prow = valid_pairs[0]
        for rr in row_data:
            rr["Type"] = "Skip"
        mrow["Type"] = "Mag"
        prow["Type"] = "Phase"
        return None

    # No valid pairs
    c = Counter(r['Count'] for r in row_data)
    if not any(v >= 2 for v in c.values()):
        return "Not suitable for QSM (no two series share the same Count)."
    return None

def validate_qsm_rows(row_data):
    """Must have exactly 1 Mag and 1 Phase with same Count."""
    mag_rows = [r for r in row_data if r['Type'] == 'Mag']
    phase_rows = [r for r in row_data if r['Type'] == 'Phase']
    if len(mag_rows) != 1:
        return (False, f"Must select exactly 1 Mag, found {len(mag_rows)}.")
    if len(phase_rows) != 1:
        return (False, f"Must select exactly 1 Phase, found {len(phase_rows)}.")
    if mag_rows[0]['Count'] != phase_rows[0]['Count']:
        return (False, f"Mag/Phase count mismatch: {mag_rows[0]['Count']} vs {phase_rows[0]['Count']}.")
    return (True, "")

def validate_t1w_rows(row_data):
    """Must have ≥1 row labeled T1w."""
    used = [r for r in row_data if r['Type'] == 'T1w']
    if len(used) < 1:
        return (False, "Must select at least one series for T1w.")
    return (True, "")

def detail_screen(stdscr, protocols, protocol_idx, user_data, detail_idx):
    """
    detail_idx = -2 -> ProtocolName line
    detail_idx = -1 -> Acquisition type line
    detail_idx >= 0 -> Table row index
    """
    curses.curs_set(0)

    protocol = protocols[protocol_idx]
    rows = user_data[protocol]["Rows"]
    acq_type = user_data[protocol]["AcquisitionType"]
    detail_error = ""

    while True:
        stdscr.clear()
        stdscr.addstr(0, 0, "(↑/↓ move highlight; ←/→ change; ENTER confirm; ESC back)")

        # Protocol line
        proto_line = f"ProtocolName ({protocol_idx+1} of {len(protocols)}): {protocol}"
        if detail_idx == -2:
            stdscr.attron(curses.A_REVERSE)
            stdscr.addstr(1, 0, proto_line)
            stdscr.attroff(curses.A_REVERSE)
        else:
            stdscr.addstr(1, 0, proto_line)

        # Acquisition type line
        acq_line = f"Acquisition type: {acq_type}"
        if detail_idx == -1:
            stdscr.attron(curses.A_REVERSE)
            stdscr.addstr(2, 0, acq_line)
            stdscr.attroff(curses.A_REVERSE)
        else:
            stdscr.addstr(2, 0, acq_line)

        start_row = 4
        headers = ["SeriesDescription", "ImageType", "Count", "NumEchoes", "Type"]
        table_data = []
        for r in rows:
            table_data.append([
                r["SeriesDescription"],
                r["ImageType"],
                r["Count"],
                r["NumEchoes"],
                r["Type"]
            ])

        table_lines = tabulate(table_data, headers=headers, tablefmt="plain").split("\n")
        for i, line in enumerate(table_lines):
            row_y = start_row + i
            if i == 0:
                # table header
                stdscr.addstr(row_y, 0, line)
            else:
                row_i = i - 1
                if row_i == detail_idx:
                    stdscr.attron(curses.A_REVERSE)
                    stdscr.addstr(row_y, 0, line)
                    stdscr.attroff(curses.A_REVERSE)
                else:
                    stdscr.addstr(row_y, 0, line)

        # Check T1 validity if acq_type is T1
        if acq_type == "T1w":
            ok, msg = validate_t1w_rows(rows)
            if not ok and not detail_error:
                detail_error = msg

        if detail_error:
            stdscr.addstr(start_row + len(table_lines) + 2, 0, f"ERROR: {detail_error}")

        stdscr.refresh()
        key = stdscr.getch()

        if key == 27:  # ESC -> back
            return ("back", detail_error, detail_idx)

        elif key == curses.KEY_UP:
            if detail_idx > -2:
                detail_idx -= 1

        elif key == curses.KEY_DOWN:
            if detail_idx < len(rows) - 1:
                if detail_idx == -1 and acq_type == "Skip":
                    continue
                if detail_idx == -1 and detail_error and detail_error.startswith("Not suitable"):
                    continue
                detail_idx += 1

        elif key == curses.KEY_LEFT or key == curses.KEY_RIGHT:
            dx = 1 if key == curses.KEY_RIGHT else -1
            
            # If on protocol line, switch to previous protocol
            if detail_idx == -2:
                if detail_error:
                    continue
                elif dx == -1:
                    return ("prev_proto", detail_error, detail_idx)
                else:
                    return ("next_proto", detail_error, detail_idx)
                
            # If on acquisition line, cycle QSM/T1/Skip left
            elif detail_idx == -1:
                possible = ["QSM", "T1w", "Skip"]
                i_acq = possible.index(acq_type)
                new_acq = possible[(i_acq + dx) % len(possible)]
                user_data[protocol]["AcquisitionType"] = new_acq
                acq_type = new_acq
                detail_error = ""
                if acq_type == "QSM":
                    auto_err = find_and_autoassign_qsm_pairs(rows)
                    if auto_err:
                        detail_error = auto_err
                    for r in rows:
                        if r["Type"] == "T1w":
                            r["Type"] = "Skip"
                elif acq_type == "T1w":
                    for r in rows:
                        if r["Type"] in ("Mag", "Phase"):
                            r["Type"] = "Skip"
                    check_ok, check_msg = validate_t1w_rows(rows)
                    if not check_ok:
                        detail_error = check_msg
                else:
                    for r in rows:
                        r["Type"] = "Skip"
            else:
                if acq_type == "QSM":
                    opts = ["Mag", "Phase", "Skip"]
                elif acq_type == "T1w":
                    opts = ["T1w", "Skip"]
                else:
                    opts = []
                if opts:
                    cur_val = rows[detail_idx]["Type"]
                    i_val = opts.index(cur_val) if cur_val in opts else 0
                    new_v = (i_val + dx) % len(opts)
                    rows[detail_idx]["Type"] = opts[new_v]
                    detail_error = ""

        elif key in (curses.KEY_ENTER, 10, 13):
            # Example QSM validation
            if acq_type == "QSM":
                if detail_error.startswith("Not suitable"):
                    detail_error = "Cannot confirm QSM: Not suitable. Pick T1w or Skip."
                    continue
                ok, msg = validate_qsm_rows(rows)
                if not ok:
                    detail_error = msg
                    continue
            elif acq_type == "T1w":
                ok, msg = validate_t1w_rows(rows)
                if not ok:
                    detail_error = msg
                    continue
            return ("done", detail_error, detail_idx)

        # Clamp
        if detail_idx < -2:
            detail_idx = -2
        if detail_idx > len(rows) - 1:
            detail_idx = len(rows) - 1


def interactive_acquisition_selection(dicom_session):
    def curses_ui(stdscr):
        curses.curs_set(0)

        # Build user_data for each protocol
        protocols = sorted(dicom_session['ProtocolName'].unique().tolist())
        user_data = {}
        for prot in protocols:
            user_data[prot] = {
                "AcquisitionType": "Skip",
                "Rows": []
            }

        # Group by ProtocolName, SeriesDescription, ImageType, also include the number of distinct EchoTime values as NumEchoes
        grouped = (
            dicom_session
            .groupby(["ProtocolName", "SeriesDescription", "ImageType"])
            .agg(Count=("InstanceNumber", "count"), NumEchoes=("EchoTime", "nunique"))
            .reset_index()
        )

        # Build rows
        protocol_rows_map = {}
        for prot in protocols:
            subset = grouped[grouped["ProtocolName"] == prot]
            row_dicts = []
            for _, row in subset.iterrows():
                row_dicts.append({
                    "SeriesDescription": row["SeriesDescription"],
                    "ImageType": row["ImageType"],
                    "Count": row["Count"],
                    "NumEchoes": row["NumEchoes"],
                    "Type": "Skip"
                })
            protocol_rows_map[prot] = row_dicts

        # Populate user_data
        for prot in protocols:
            if not user_data[prot]["Rows"]:
                user_data[prot]["Rows"] = protocol_rows_map[prot]

        protocol_idx = 0
        detail_idx = -2  # start on the protocol line

        while True:
            ret, err, detail_idx = detail_screen(
                stdscr, protocols, protocol_idx, user_data, detail_idx
            )
            if ret == "prev_proto":
                protocol_idx = max(0, protocol_idx - 1)
            elif ret == "next_proto":
                protocol_idx = min(len(protocols) - 1, protocol_idx + 1)
            elif ret == "done":
                # Move to next protocol or break
                if protocol_idx == len(protocols) - 1:
                    break
                else:
                    protocol_idx += 1
                    detail_idx = -2
            elif ret == "back":
                break

        return user_data

    return curses.wrapper(curses_ui)

def convert_and_organize(dicom_session, output_dir, dcm2niix_path="dcm2niix"):
    """
    dicom_session: DataFrame with columns including:
      - PatientID
      - StudyDate
      - ProtocolName
      - AcquisitionType (T1w, QSM, Skip)
      - Type (Mag, Phase, T1w, Skip)
      - The DICOM_Path column or equivalent
    output_dir: the BIDS root
    dcm2niix_path: path to the dcm2niix executable
    
    This function:
      1) Groups DICOM files by relevant fields
      2) Calls dcm2niix to convert each group
      3) Renames/moves the output to a BIDS-like path
    """
    # Filter out any rows that are marked Skip
    # or group only rows that are QSM or T1w, etc.
    to_convert = dicom_session.query("AcquisitionType != 'Skip' and Type != 'Skip'")
    
    # Group by patient ID, date, protocol, plus AcquisitionType
    group_cols = ["PatientID", "StudyDate", "ProtocolName", "SeriesDescription", "AcquisitionType", "Type", "EchoNumber", "RunNumber"]
    
    # If multiple SeriesInstanceUID or SeriesDescription exist, include them in grouping
    if "SeriesInstanceUID" in to_convert.columns:
        group_cols.append("SeriesInstanceUID")
    
    grouped = to_convert.groupby(group_cols)
    
    for grp_keys, grp_data in grouped:
        # grp_data is all rows belonging to that group
        # We'll gather the DICOM file paths and run dcm2niix on them
        # Keep them in a temporary folder or pass them directly
        
        # Build a small temp folder for these DICOM files, or pass the top-level directory
        # For safety, collecting them in a temp folder might be clearer
        # (Here we just get a unique list of DICOM files)
        dicom_files = grp_data["DICOM_Path"].unique().tolist()

        # create temp_convert folder
        tmp_outdir = os.path.join(output_dir, "temp_convert")
        os.makedirs(tmp_outdir, exist_ok=True)
        
        # Some unique name for the output base
        # dcm2niix will produce something like "something.nii.gz" and "something.json"
        # We'll rename them afterwards
        dcm2niix_base = "temp_output"

        # Copy all relevant DICOM files into the temp folder
        for dicom_file in dicom_files:
            shutil.copy(dicom_file, tmp_outdir)
        
        # Build command
        cmd = f'"{dcm2niix_path}" -o "{tmp_outdir}" -f "{dcm2niix_base}" -z n "{tmp_outdir}"'
        # If needed, you can pass all files or just the folder containing them
        # Adjust accordingly
        os.system(cmd)
        
        # After this, check what was generated in tmp_outdir
        # Typically, you might see "temp_output.nii" and "temp_output.json" or multiple if multiple series
        # We'll find them by pattern:
        converted_niftis = []
        for fn in os.listdir(tmp_outdir):
            if fn.startswith(dcm2niix_base) and (fn.endswith(".nii") or fn.endswith(".nii.gz")):
                converted_niftis.append(fn)
        
        # Move them into final BIDS path
        for nii_name in converted_niftis:
            # Example: we only handle the first row for building the base name, or pick any
            row = grp_data.iloc[0]

            subject_id = f"sub-{row['PatientID']}"
            session_id = f"ses-{row['StudyDate']}"
            acq_label = f"acq-{clean(row['ProtocolName'])}"
            
            base_name = f"{subject_id}_{session_id}_{acq_label}"

            if row["NumEchoes"] > 1:
                base_name += f"_echo-{int(row['EchoNumber']):02}"

            if row["NumRuns"] > 1:
                base_name += f"_run-{int(row['RunNumber']):02}"
            
            if row["AcquisitionType"] == "QSM":
                base_name += f"_part-{row['Type'].lower()}"

            if row["AcquisitionType"] == "T1w":
                base_name += "_T1w"
            elif row["AcquisitionType"] == "QSM" and row["NumEchoes"] > 1:
                base_name += "_MEGRE"
            elif row["AcquisitionType"] == "QSM" and row["NumEchoes"] == 1:
                base_name += "_T2starw"
            
            # Original extension
            ext = '.nii' if nii_name.endswith('.nii') else '.nii.gz'
            
            # Decide the subdir: sub-XX/ses-XX/anat/
            out_dir = os.path.join(output_dir, subject_id, session_id, "anat")
            os.makedirs(out_dir, exist_ok=True)
            dst_path = os.path.join(out_dir, f"{base_name}{ext}")
            src_path = os.path.join(tmp_outdir, nii_name)
            
            # Move the nifti
            shutil.move(src_path, dst_path)
            
            # Also move the json sidecar
            json_src = os.path.join(tmp_outdir, nii_name.replace(ext, '.json'))
            if os.path.isfile(json_src):
                json_dst = os.path.join(out_dir, f"{base_name}.json")
                shutil.move(json_src, json_dst)

        # delete temp_convert folder
        shutil.rmtree(tmp_outdir)

def convert_to_nifti(input_dir, output_dir, *args, **kwargs):
    # Load DICOM session and time how long it takes
    time_start = time.time()
    dicom_session = load_dicom_session(input_dir, parallel_workers=12, show_progress=True)
    time_end = time.time()
    print(f"Loaded DICOM session in {time_end - time_start:.2f} seconds")
    
    # Reset index to avoid issues with groupby
    dicom_session.reset_index(drop=True, inplace=True)

    # Determine EchoNumber for each row - rank EchoTime values within each ProtocolName
    dicom_session['NumEchoes'] = dicom_session.groupby('ProtocolName')['EchoTime'].transform('nunique')
    dicom_session['EchoNumber'] = dicom_session.groupby('ProtocolName')['EchoTime'].rank(method='dense')

    # Determine RunNumber for each row. Runs are individual runs of specific ProtocolNames for a given subject on a given day.
    dicom_session['NumRuns'] = dicom_session.groupby(['PatientID', 'StudyDate', 'ProtocolName'])['SeriesInstanceUID'].transform('nunique')
    dicom_session['RunNumber'] = dicom_session.groupby(['PatientID', 'StudyDate', 'ProtocolName', 'SeriesDescription'])['SeriesInstanceUID'].rank(method='dense')

    # Present interactive interface
    selections = interactive_acquisition_selection(dicom_session)

    # updating dicom_session with final user-chosen 'Type'
    if 'AcquisitionType' not in dicom_session.columns:
        dicom_session['AcquisitionType'] = 'Skip'

    if 'Type' not in dicom_session.columns:
        dicom_session['Type'] = 'Skip'

    for prot, data_dict in selections.items():
        acq_type = data_dict["AcquisitionType"]
        # Mark the entire protocol with that acquisition type
        mask_prot = (dicom_session['ProtocolName'] == prot)
        dicom_session.loc[mask_prot, 'AcquisitionType'] = acq_type

        # If QSM, also apply the row-level "Mag"/"Phase"/"Skip"
        if acq_type == "QSM":
            for row_info in data_dict["Rows"]:
                # Find matching lines in dicom_session
                rmask = (
                    mask_prot &
                    (dicom_session['SeriesDescription'] == row_info['SeriesDescription']) &
                    (dicom_session['ImageType'] == row_info['ImageType'])
                )
                dicom_session.loc[rmask, 'Type'] = row_info['Type']
        elif acq_type == "T1w":
            for row_info in data_dict["Rows"]:
                rmask = (
                    mask_prot &
                    (dicom_session['SeriesDescription'] == row_info['SeriesDescription']) &
                    (dicom_session['ImageType'] == row_info['ImageType'])
                )
                dicom_session.loc[rmask, 'Type'] = row_info['Type']

    # Convert and organize
    convert_and_organize(dicom_session, output_dir)
    
def script_exit(exit_code=0):
    logger = make_logger()
    show_warning_summary(logger)
    logger.log(LogLevel.INFO.value, 'Finished')
    exit(exit_code)

def main():
    parser = argparse.ArgumentParser(
        description="QSMxT dicomConvert: Converts DICOM files to NIfTI/BIDS",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        'input_dir',
        help='Sorted DICOM directory generated using dicom_sort.py of the format {subject}/{session}/{series}'
    )

    parser.add_argument(
        'output_dir',
        help='Output BIDS directory.'
    )

    parser.add_argument(
        '--auto_yes',
        action='store_true',
        help='Force running non-interactively. This option is useful when used as part of a script or on a testing server.'
    )

    parser.add_argument(
        '--qsm_protocol_patterns',
        default=['*t2starw*', '*qsm*'],
        nargs='*',
        help='Patterns used to identify series acquired for QSM. These patterns will be used to match the \'ProtocolName\' '+
             'field. If no series are found matching these protocols, you will be prompted to select the appropriate '+
             'series\' interactively.'
    )

    parser.add_argument(
        '--t1w_protocol_patterns',
        default=['*t1w*'],
        nargs='*',
        help='Patterns used to identify series containing T1-weighted brain images. These series may be used during the '+
             'qsmxt.py script for automated brain segmentation and registration to the QSM space.'
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

    # write "references.txt" with the command used to invoke the script and any necessary citations
    with open(os.path.join(args.output_dir, "references.txt"), 'w', encoding='utf-8') as f:
        # output QSMxT version, run command, and python interpreter
        f.write(f"QSMxT: {get_qsmxt_version()}")
        f.write(f"\nRun command: {str.join(' ', sys.argv)}")
        f.write(f"\nPython interpreter: {sys.executable}")

        f.write("\n\n == References ==")

        # qsmxt, dcm2niix, bids
        f.write("\n\n - Stewart AW, Robinson SD, O'Brien K, et al. QSMxT: Robust masking and artifact reduction for quantitative susceptibility mapping. Magnetic Resonance in Medicine. 2022;87(3):1289-1300. doi:10.1002/mrm.29048")
        f.write("\n\n - Stewart AW, Shaw T, Bollmann S. QSMxT: A Complete QSM Processing Framework. GitHub; 2022. https://github.com/QSMxT/QSMxT")
        f.write("\n\n - Li X, Morgan PS, Ashburner J, Smith J, Rorden C. The first step for neuroimaging data analysis: DICOM to NIfTI conversion. J Neurosci Methods. 2016;264:47-56. doi:10.1016/j.jneumeth.2016.03.001")
        f.write("\n\n - Rorden C et al. Rordenlab/Dcm2niix. GitHub; 2022. https://github.com/rordenlab/dcm2niix")
        f.write("\n\n - Gorgolewski KJ, Auer T, Calhoun VD, et al. The brain imaging data structure, a format for organizing and describing outputs of neuroimaging experiments. Sci Data. 2016;3(1):160044. doi:10.1038/sdata.2016.44")
        f.write("\n\n")
    
    convert_to_nifti(
        input_dir=args.input_dir,
        output_dir=args.output_dir,
        qsm_protocol_patterns=[pattern.lower() for pattern in args.qsm_protocol_patterns],
        t1w_protocol_patterns=[pattern.lower() for pattern in args.t1w_protocol_patterns],
        auto_yes=args.auto_yes
    )

    script_exit()

if __name__ == "__main__":
    main()

