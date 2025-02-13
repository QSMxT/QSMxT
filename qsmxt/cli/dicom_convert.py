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
import fnmatch

from tabulate import tabulate

from dicompare import load_dicom_session, load_nifti_session

from qsmxt.scripts.qsmxt_functions import get_qsmxt_version, get_diff
from qsmxt.scripts.logger import LogLevel, make_logger, show_warning_summary 
from qsmxt.scripts.nii_fix_ge import fix_ge_polar, fix_ge_complex

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
    if full_path:
        return folders
    folders = [os.path.split(x)[1] for x in folders]
    return folders

def find_and_autoassign_qsm_pairs(row_data):
    """
    Automatically identifies exactly one pair for QSM per (Count, NumEchoes),
    picking the pair (Mag+Phase or Real+Imag) with the smallest difference
    in SeriesNumber. Priority:
       1) Mag + Phase
       2) Real + Imag

    Returns None if at least one valid pair is found, or a message if none is found.
    """

    from collections import defaultdict, Counter

    # Group rows by (Count, NumEchoes). If you only want to group by Count, remove NumEchoes
    grouped = defaultdict(list)
    for r in row_data:
        cval = (r["Count"], r["NumEchoes"])
        grouped[cval].append(r)

    any_pair_found = False

    for (count_val, echo_val), group_rows in grouped.items():
        # Mark everything Skip first
        for row in group_rows:
            row["Type"] = "Skip"

        # Gather potential M/P or R/I based on ImageType
        mag_list   = []
        phase_list = []
        real_list  = []
        imag_list  = []

        def classify(row):
            """Return 'mag', 'phase', 'real', or 'imag' if the row qualifies, else None."""
            # Consider row['ImageType'] (tuple?)
            # We'll unify to uppercase strings:
            image_type = row.get("ImageType", ())
            if isinstance(image_type, (tuple, list)):
                image_type_up = [x.upper() for x in image_type]
            else:
                image_type_up = [str(image_type).upper()]

            # Some helper for checking the row's ImageType
            def has_any(substrings, container):
                return any(s in container for s in substrings)

            # Priority #1: mag
            if has_any(["MAG", "M"], image_type_up):
                return "mag"
            # Priority #2: phase
            if has_any(["PHASE", "P"], image_type_up):
                return "phase"
            # Priority #3: real
            if has_any(["REAL"], image_type_up):
                return "real"
            # Priority #4: imag (imaginary)
            if has_any(["IMAG", "IMAGINARY"], image_type_up):
                return "imag"
            return None

        # Classify each row
        for row in group_rows:
            ctype = classify(row)
            if ctype == "mag":
                mag_list.append(row)
            elif ctype == "phase":
                phase_list.append(row)
            elif ctype == "real":
                real_list.append(row)
            elif ctype == "imag":
                imag_list.append(row)
            # else remains "Skip"

        # Next, we pick exactly 1 mag+phase pair if possible
        # Among all possible pairs, choose the one with the SMALLEST difference in SeriesNumber
        def pick_closest_pair(listA, listB, labelA, labelB):
            """Return (rowA, rowB) that have the minimal |SeriesNumberA - SeriesNumberB|, or None."""
            if not listA or not listB:
                return None, None
            best_pair = None
            best_diff = float("inf")
            for a in listA:
                for b in listB:
                    # We'll assume row['SeriesNumber'] is an integer or numeric
                    # If it's missing, default to 999999 or 0, you can decide
                    #snA = a.get("SeriesNumber", 999999)
                    #snB = b.get("SeriesNumber", 999999)
                    # let's instead use the row idx
                    snA = group_rows.index(a)
                    snB = group_rows.index(b)
                    
                    diff = abs(snA - snB)
                    if diff < best_diff:
                        best_diff = diff
                        best_pair = (a, b)
            if best_pair:
                best_pair[0]["Type"] = labelA  # e.g. "Mag"
                best_pair[1]["Type"] = labelB  # e.g. "Phase"
                return best_pair
            return None, None

        # 1) Try picking best (mag, phase)
        mag_phase_pair = pick_closest_pair(mag_list, phase_list, "Mag", "Phase")
        if mag_phase_pair[0] and mag_phase_pair[1]:
            any_pair_found = True
        else:
            # 2) if no mag+phase, try real+imag
            real_imag_pair = pick_closest_pair(real_list, imag_list, "Real", "Imag")
            if real_imag_pair[0] and real_imag_pair[1]:
                any_pair_found = True
            else:
                # Nothing chosen => entire group remains "Skip"
                pass

    # If no pairs found in any group, do the repeated Count check
    if not any_pair_found:
        counts = Counter(r["Count"] for r in row_data)
        if not any(v >= 2 for v in counts.values()):
            return "Not suitable for QSM (no repeated Count)."
    return None

import re

def find_and_autoassign_t1w(row_data):
    """
    Automatically identifies exactly one series for T1w.

    Priority of matching (picks first match):
      1) If row_data has exactly one row, auto-assign T1w.
      2) "UNI?DEN" in ImageType or SeriesDescription (regex 'UNI.?DEN').
      3) "T1" in ImageType.
      4) "*t1w*" in SeriesDescription.
      5) "*t1*" in SeriesDescription.
      6) Otherwise, exclude any series with "T1 MAP" or "R1" or "R2" in
         ImageType or SeriesDescription, pick the first from the remainder if exactly one.
    
    If no candidate is found, all remain "Skip".
    """

    # Mark all as Skip
    for row in row_data:
        row["Type"] = "Skip"

    # If there's exactly one row, assign it T1w and return
    if len(row_data) == 1:
        row_data[0]["Type"] = "T1w"
        return

    # 1) Attempt "UNI?DEN" in ImageType or SeriesDescription
    # We'll define a quick function to check
    UNI_DEN_REGEX = re.compile(r'UNI.?DEN', re.IGNORECASE)

    for row in row_data:
        it_str = row.get("ImageType", [])
        sd_str = row.get("SeriesDescription", "")
        if any(UNI_DEN_REGEX.search(x) for x in it_str) or UNI_DEN_REGEX.search(sd_str):
            row["Type"] = "T1w"
            return  # done

    # 2) If no match yet, look for "T1" in ImageType
    for row in row_data:
        it_str = row.get("ImageType", [])
        if "T1" in it_str:
            row["Type"] = "T1w"
            return  # done

    # 3) If no match yet, SeriesDescription matching "*t1w*"
    for row in row_data:
        sd_str = row.get("SeriesDescription", "")
        if "T1W" in sd_str:
            row["Type"] = "T1w"
            return  # done

    # 4) If no match yet, SeriesDescription matching "*t1*"
    for row in row_data:
        sd_str = row.get("SeriesDescription", "")
        if "T1" in sd_str:
            row["Type"] = "T1w"
            return  # done

    def is_excluded_t1_map_r(row):
        it_str = row.get("ImageType", [])
        sd_str = row.get("SeriesDescription", "").upper()
        exclude_keywords = ["T1 MAP", "R1", "R2"]
        return any(k in it_str or k in sd_str for k in exclude_keywords)

    # build list of non-excluded
    candidates = [r for r in row_data if not is_excluded_t1_map_r(r)]
    
    # if candidates has one item exactly
    if len(candidates) == 1:
        candidates[0]["Type"] = "T1w"
        return

    # If we reached here, no T1w assigned (all remain Skip).
    return


def validate_qsm_rows(row_data):
    """
    Checks if at least one valid pair is assigned. 
    For each Count, allowed combinations:
      - 0 or 1 Mag, 0 or 1 Phase, or 
      - 0 or 1 Real, 0 or 1 Imag
    A pair is (Mag, Phase) or (Real, Imag) for that Count.
    If no pairs are found, or if there is any partial mismatch, returns an error message.
    """
    used_rows = [r for r in row_data if r['Type'] in ['Mag','Phase','Real','Imag']]
    if not used_rows:
        return (False, "No pairs have been selected.")
    
    # Group by Count
    grouped = {}
    for r in used_rows:
        cval = r['Count']
        if cval not in grouped:
            grouped[cval] = []
        grouped[cval].append(r)

    valid_pairs_found = False

    for cval, group in grouped.items():
        # Sort them by Type
        assigned_types = [g["Type"] for g in group]
        n_mag = assigned_types.count("Mag")
        n_pha = assigned_types.count("Phase")
        n_real = assigned_types.count("Real")
        n_imag = assigned_types.count("Imag")

        # If we have both Mag and Phase for this Count, that is one valid pair
        # If we also have Real and Imag, that's an additional valid pair
        # No mixing Real+Phase or Imag+Mag for the same row
        # More than one instance of the same label is not allowed here
        if n_mag > 1 or n_pha > 1 or n_real > 1 or n_imag > 1:
            return (False, f"More than one row labeled as Mag/Phase/Real/Imag for the same Count {cval}.")

        # If we have (Mag=1, Phase=1), that is a valid pair
        # If we have (Real=1, Imag=1), that is another valid pair
        # A partial mismatch is not allowed
        has_magpha = (n_mag == 1 and n_pha == 1)
        has_realimag = (n_real == 1 and n_imag == 1)
        partial_magpha = (n_mag == 1 and n_pha == 0) or (n_mag == 0 and n_pha == 1)
        partial_realimag = (n_real == 1 and n_imag == 0) or (n_real == 0 and n_imag == 1)

        if partial_magpha or partial_realimag:
            return (False, f"Partial assignment at Count {cval} is not allowed.")

        if has_magpha or has_realimag:
            valid_pairs_found = True
    
    if not valid_pairs_found:
        return (False, "No valid pairs were found in the assigned rows.")
    
    return (True, "")

def validate_t1w_rows(row_data):
    used = [r for r in row_data if r['Type'] == 'T1w']
    if len(used) < 1:
        return (False, "Must select at least one series for T1w.")
    return (True, "")

def detail_screen(stdscr, protocols, user_data):
    """
    Integrates the protocol loop that was previously in interactive_acquisition_selection.
    This function handles rendering and key interactions for each protocol in 'protocols'.
    """

    def do_validation(rows, acq_type):
        if acq_type == "QSM":
            ok, msg = validate_qsm_rows(rows)
            if not ok:
                # Place error message after the table
                error_line = TABLE_ROWS + len(rows) + 1
                stdscr.addstr(error_line, 0, f"ERROR: {msg}")
                return False
        elif acq_type == "T1w":
            ok, msg = validate_t1w_rows(rows)
            if not ok:
                error_line = TABLE_ROWS + len(rows) + 1
                stdscr.addstr(error_line, 0, f"ERROR: {msg}")
                return False
        return True

    protocol_idx = 0

    # Constants for screen lines
    INSTRUCTIONS_LINE_1 = 0
    INSTRUCTIONS_LINE_2 = 1
    PROTOCOL_NAME_LINE = 3
    ACQ_TYPE_LINE = 4
    TABLE_HEADER = 6
    TABLE_ROWS = 7

    detail_idx = PROTOCOL_NAME_LINE

    while True:
        stdscr.clear()

        # Two lines of instructions
        stdscr.addstr(INSTRUCTIONS_LINE_1, 0, "Use arrow keys: (↑/↓) move, (←/→) change, ENTER to confirm.")
        stdscr.addstr(INSTRUCTIONS_LINE_2, 0, "ESC exits or goes back to previous menu.")

        protocol = protocols[protocol_idx]
        rows = user_data[protocol]["Rows"]
        acq_type = user_data[protocol]["AcquisitionType"]

        # Protocol line
        proto_line = f"ProtocolName ({protocol_idx + 1} of {len(protocols)}): {protocol}"
        if detail_idx == PROTOCOL_NAME_LINE:
            stdscr.attron(curses.A_REVERSE)
            stdscr.addstr(PROTOCOL_NAME_LINE, 0, proto_line)
            stdscr.attroff(curses.A_REVERSE)
        else:
            stdscr.addstr(PROTOCOL_NAME_LINE, 0, proto_line)

        # Acquisition type line
        acq_line = f"Acquisition type: {acq_type}"
        if detail_idx == ACQ_TYPE_LINE:
            stdscr.attron(curses.A_REVERSE)
            stdscr.addstr(ACQ_TYPE_LINE, 0, acq_line)
            stdscr.attroff(curses.A_REVERSE)
        else:
            stdscr.addstr(ACQ_TYPE_LINE, 0, acq_line)

        # Table display
        headers = ["SeriesDescription", "ImageType", "Count", "NumEchoes", "Type"]
        table_data = [
            [r["SeriesDescription"], r["ImageType"], r["Count"], r["NumEchoes"], r["Type"]]
            for r in rows
        ]
            
        table_lines = tabulate(table_data, headers=headers, tablefmt="plain").split("\n")
        for i, line in enumerate(table_lines):
            row_y = TABLE_HEADER + i
            if i == 0:
                stdscr.addstr(row_y, 0, line)
            else:
                row_i = row_y
                if detail_idx == row_i:
                    stdscr.attron(curses.A_REVERSE)
                    stdscr.addstr(row_y, 0, line)
                    stdscr.attroff(curses.A_REVERSE)
                else:
                    stdscr.addstr(row_y, 0, line)

        # Run basic validation for display feedback
        valid = do_validation(rows, acq_type)

        stdscr.refresh()
        key = stdscr.getch()

        # If ENTER
        if key == 10:
            # Check all protocols for validity
            invalid_protocol = None
            for prot in protocols:
                test_acq_type = user_data[prot]["AcquisitionType"]
                test_rows = user_data[prot]["Rows"]
                if test_acq_type in ["QSM", "T1w"]:
                    ok, msg = validate_qsm_rows(test_rows) if test_acq_type == "QSM" else validate_t1w_rows(test_rows)
                    if not ok:
                        invalid_protocol = (prot, msg)
                        break

            if invalid_protocol:
                # Show error message for the protocol that failed
                error_line = TABLE_ROWS + len(rows) + 2
                stdscr.addstr(error_line, 0, f"ERROR: Protocol '{invalid_protocol[0]}' is not valid: {invalid_protocol[1]}")
                stdscr.refresh()
                continue  # Return to the loop so the user can fix the issue

            # Count how many are set to skip
            skip_count = sum(1 for p in protocols if user_data[p]["AcquisitionType"] == "Skip")
            if skip_count > 0:
                # Ask user for confirmation
                prompt_line = TABLE_ROWS + len(rows) + 3
                skip_msg = f"{skip_count} protocols are marked skip. Continue anyway? (y/n)"
                stdscr.addstr(prompt_line, 0, skip_msg)
                stdscr.refresh()

                while True:
                    choice = stdscr.getch()
                    if choice in [ord('y'), ord('Y')]:
                        return user_data
                    elif choice in [ord('n'), ord('N')]:
                        break
            else:
                return user_data

        if key == curses.KEY_UP:
            if detail_idx > PROTOCOL_NAME_LINE:
                detail_idx -= 1
            if detail_idx == TABLE_HEADER:
                detail_idx = ACQ_TYPE_LINE

        elif key == curses.KEY_DOWN:
            last_table_line = TABLE_HEADER + len(rows)
            if detail_idx < last_table_line:
                if detail_idx == ACQ_TYPE_LINE and acq_type == "Skip":
                    continue
                detail_idx += 1
            if detail_idx < TABLE_ROWS and detail_idx > ACQ_TYPE_LINE:
                detail_idx = TABLE_ROWS

        elif key == curses.KEY_LEFT or key == curses.KEY_RIGHT:
            dx = 1 if key == curses.KEY_RIGHT else -1

            # If on the protocol line
            if detail_idx == PROTOCOL_NAME_LINE:
                if not valid:
                    continue
                if dx == -1:
                    protocol_idx = max(0, protocol_idx - 1)
                else:
                    protocol_idx = min(len(protocols) - 1, protocol_idx + 1)
                detail_idx = PROTOCOL_NAME_LINE

            elif detail_idx == ACQ_TYPE_LINE:
                possible = ["QSM", "T1w", "Skip"]
                i_acq = possible.index(acq_type)
                new_acq = possible[(i_acq + dx) % len(possible)]
                user_data[protocol]["AcquisitionType"] = new_acq
                acq_type = new_acq
                if new_acq == "QSM":
                    find_and_autoassign_qsm_pairs(rows)
                if new_acq == "T1w":
                    find_and_autoassign_t1w(rows)

            # If we're in the table rows
            elif detail_idx >= TABLE_ROWS:
                row_index = detail_idx - TABLE_ROWS
                if acq_type == "QSM":
                    opts = ["Mag", "Phase", "Real", "Imag", "Skip"]
                elif acq_type == "T1w":
                    opts = ["T1w", "Skip"]
                else:
                    opts = []
                if opts:
                    cur_val = rows[row_index]["Type"]
                    i_val = opts.index(cur_val) if cur_val in opts else 0
                    new_v = (i_val + dx) % len(opts)
                    rows[row_index]["Type"] = opts[new_v]

        if detail_idx < PROTOCOL_NAME_LINE:
            detail_idx = PROTOCOL_NAME_LINE

        last_table_line = TABLE_ROWS + len(rows)
        if detail_idx > last_table_line:
            detail_idx = last_table_line

def interactive_acquisition_selection(grouped):
    def curses_ui(stdscr):
        curses.curs_set(0)

        protocols = grouped["ProtocolName"].unique()
        user_data = {prot: {"AcquisitionType": "Skip", "Rows": []} for prot in protocols}

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

        for prot in protocols:
            user_data[prot]["Rows"] = protocol_rows_map[prot]

        detail_screen(
            stdscr, protocols, user_data,
        )

        return user_data

    return curses.wrapper(curses_ui)

def convert_and_organize(dicom_session, output_dir, dcm2niix_path="dcm2niix"):
    logger = make_logger()
    to_convert = dicom_session[
        (dicom_session['AcquisitionType'] != 'Skip') &
        (dicom_session['Type'] != 'Skip')
    ]

    if to_convert.empty:
        logger.log(LogLevel.ERROR.value, "No acquisitions selected for conversion. Exiting.")
        script_exit(1)
    
    group_cols = [
        "PatientID", "StudyDate", "ProtocolName", "SeriesDescription", 
        "AcquisitionType", "Type", "EchoNumber", "RunNumber"
    ]
    missing_cols = [col for col in group_cols if col not in to_convert.columns]
    if missing_cols:
        logger.log(LogLevel.WARNING.value, f"The following fields are missing from the DICOM session: {', '.join(missing_cols)}")
    if "SeriesInstanceUID" in to_convert.columns:
        group_cols.append("SeriesInstanceUID")
    
    grouped = to_convert.groupby(group_cols)

    if not grouped.groups:
        logger.log(LogLevel.ERROR.value, "No valid acquisitions found. Exiting.")
        script_exit(1)
    
    for grp_keys, grp_data in grouped:
        logger.log(LogLevel.INFO.value, f"Converting {grp_keys}")
        dicom_files = grp_data["DICOM_Path"].unique().tolist()

        tmp_outdir = os.path.join(output_dir, "temp_convert")
        os.makedirs(tmp_outdir, exist_ok=True)
        dcm2niix_base = "temp_output"

        for dicom_file in dicom_files:
            shutil.copy(dicom_file, tmp_outdir)
        
        cmd = f'"{dcm2niix_path}" -o "{tmp_outdir}" -f "{dcm2niix_base}" -z n "{tmp_outdir}"'
        logger.log(LogLevel.INFO.value, f"Running command: '{cmd}'")
        os.system(cmd)
        
        logger.log(LogLevel.INFO.value, "Moving converted NIfTI files into BIDS structure")
        converted_niftis = []
        for fn in os.listdir(tmp_outdir):
            if fn.startswith(dcm2niix_base) and (fn.endswith(".nii") or fn.endswith(".nii.gz")):
                converted_niftis.append(fn)

        for nii_name in converted_niftis:
            row = grp_data.iloc[0]
            subject_id = f"sub-{clean(row['PatientID'])}"
            session_id = f"ses-{clean(row['StudyDate'])}"
            acq_label = f"acq-{clean(row['ProtocolName'])}"
            
            base_name = f"{subject_id}_{session_id}_{acq_label}"
            if row["NumEchoes"] > 1 and row["AcquisitionType"] == "QSM":
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
            
            ext = ".nii" if nii_name.endswith(".nii") else ".nii.gz"
            out_dir = os.path.join(output_dir, subject_id, session_id, "anat")
            os.makedirs(out_dir, exist_ok=True)
            dst_path = os.path.join(out_dir, f"{base_name}{ext}")
            src_path = os.path.join(tmp_outdir, nii_name)
            shutil.move(src_path, dst_path)
            
            json_src = os.path.join(tmp_outdir, nii_name.replace(ext, ".json"))
            if os.path.isfile(json_src):
                json_dst = os.path.join(out_dir, f"{base_name}.json")
                shutil.move(json_src, json_dst)

        shutil.rmtree(tmp_outdir)

def convert_to_bids(input_dir, output_dir, auto_yes, qsm_protocol_patterns, t1w_protocol_patterns):
    logger = make_logger()
    time_start = time.time()
    dicom_session = load_dicom_session(input_dir, parallel_workers=12, show_progress=True)
    time_end = time.time()
    logger.log(LogLevel.INFO.value, f"Loaded DICOM session in {time_end - time_start:.2f} seconds")
    
    dicom_session.reset_index(drop=True, inplace=True)

    # If we have GE private tags
    if '(0043,102F)' in dicom_session.columns:
        private_map = {0: 'M', 1: 'P', 2: 'REAL', 3: 'IMAGINARY'}
        dicom_session['ImageType'].append(dicom_session['(0043,102F)'].map(private_map).fillna(''))

    # Additional columns
    dicom_session['NumEchoes'] = dicom_session.groupby('ProtocolName')['EchoTime'].transform('nunique')
    dicom_session['EchoNumber'] = dicom_session.groupby('ProtocolName')['EchoTime'].rank(method='dense')
    dicom_session['NumRuns'] = (
        dicom_session
        .groupby(['PatientID','StudyDate','ProtocolName','SeriesDescription', 'ImageType'])['SeriesInstanceUID']
        .transform('nunique')
    )
    dicom_session['RunNumber'] = (
        dicom_session
        .groupby(['PatientID','StudyDate','ProtocolName','SeriesDescription', 'ImageType'])['SeriesInstanceUID']
        .rank(method='dense')
    )

    # Ensure 'AcquisitionType' and 'Type' exist
    if 'AcquisitionType' not in dicom_session.columns:
        dicom_session['AcquisitionType'] = 'Skip'
    if 'Type' not in dicom_session.columns:
        dicom_session['Type'] = 'Skip'

    groupby_fields = ["ProtocolName", "SeriesDescription", "ImageType"]

    grouped = (
        dicom_session
        .groupby(groupby_fields)
        .agg(Count=("InstanceNumber", "count"), NumEchoes=("EchoTime", "nunique"))
        .reset_index()
    )

    # 1) If user is interactive
    if not auto_yes and sys.__stdin__.isatty():
        logger.log(LogLevel.INFO.value, "Entering interactive mode for acquisition selection")
        selections = interactive_acquisition_selection(grouped)
        logger.log(LogLevel.INFO.value, f"User selections:\n{selections}")

        # Write back user selections
        for prot, data_dict in selections.items():
            acq_type = data_dict["AcquisitionType"]
            mask_prot = (grouped['ProtocolName'] == prot)
            grouped.loc[mask_prot, 'AcquisitionType'] = acq_type

            if acq_type in ["QSM","T1w"]:
                for row_info in data_dict["Rows"]:
                    rmask = (
                        mask_prot
                        & (grouped['SeriesDescription'] == row_info['SeriesDescription'])
                        & (grouped['ImageType'] == row_info['ImageType'])
                    )
                    grouped.loc[rmask,'Type'] = row_info['Type']

    # 2) Else do auto-assignment
    else:
        # Auto-assign 'AcquisitionType' based on protocol patterns
        for prot in grouped['ProtocolName'].unique():
            if any(fnmatch.fnmatch(prot.lower(), p.lower()) for p in qsm_protocol_patterns):
                grouped.loc[grouped['ProtocolName']==prot,'AcquisitionType'] = 'QSM'
            elif any(fnmatch.fnmatch(prot.lower(), p.lower()) for p in t1w_protocol_patterns):
                grouped.loc[grouped['ProtocolName']==prot,'AcquisitionType'] = 'T1w'
            # else remains 'Skip'

        # Now group by ProtocolName and run your auto-assign pair logic
        for prot, group_df in grouped.groupby('ProtocolName'):
            logger.log(LogLevel.INFO.value, f"Auto-assigning series for protocol {prot}")
            acq_type = group_df['AcquisitionType'].iloc[0]
            if acq_type == 'QSM':
                # Turn sub-DataFrame into python dicts for find_and_autoassign_qsm_pairs (if that expects a list of dict)
                records = group_df.to_dict('records')
                find_and_autoassign_qsm_pairs(records)
                # Write back
                updated = pd.DataFrame(records)
                logger.log(LogLevel.INFO.value, f"Auto-assignment:\n{updated}")
                # Align on index
                for i, idx in enumerate(group_df.index):
                    grouped.at[idx,'Type'] = updated.at[i,'Type']

            elif acq_type == 'T1w':
                records = group_df.to_dict('records')
                find_and_autoassign_t1w(records)
                # Write back
                updated = pd.DataFrame(records)
                logger.log(LogLevel.INFO.value, f"Auto-assignment:\n{updated}")
                for i, idx in enumerate(group_df.index):
                    grouped.at[idx,'Type'] = updated.at[i,'Type']

    # update dicom_session with the new AcquisitionType and Type
    logger.log(LogLevel.INFO.value, "Merging selections into dataframe...")
    cols_to_drop = ["AcquisitionType", "Type"]
    dicom_session.drop(columns=cols_to_drop, errors='ignore', inplace=True)

    # Now merge only the columns we want from grouped:
    merge_cols = groupby_fields + ["AcquisitionType","Type"]
    dicom_session = dicom_session.merge(
        grouped[merge_cols],
        on=groupby_fields,
        how='left'
    )

    # If some rows had no match in `grouped`, fill them with 'Skip'
    dicom_session['AcquisitionType'] = dicom_session['AcquisitionType'].fillna('Skip')
    dicom_session['Type'] = dicom_session['Type'].fillna('Skip')

    # Finally, do your conversion
    convert_and_organize(dicom_session, output_dir)


def fix_ge_data(bids_dir):
    logger = make_logger()

    logger.log(LogLevel.INFO.value, "Checking for complex or polar data requiring fixing...")
    nifti_session = load_nifti_session(bids_dir, show_progress=True)

    group_cols = [col for col in ["sub", "ses", "acq", "run", "echo", "suffix"] if col in nifti_session.columns]
    grouped = nifti_session.groupby(group_cols)

    for grp_keys, grp_data in grouped:
        if not grp_data["part"].isin(["real", "imag"]).all():
            continue
        
        real_nii_path = grp_data[grp_data["part"] == "real"]["NIfTI_Path"].values[0]
        imag_nii_path = grp_data[grp_data["part"] == "imag"]["NIfTI_Path"].values[0]

        logger.log(LogLevel.INFO.value, f"Fixing complex data (real={real_nii_path}; imag={imag_nii_path})")

        fix_ge_complex(
            real_nii_path=real_nii_path,
            imag_nii_path=imag_nii_path,
            delete_originals=True
        )
    
    for grp_keys, grp_data in grouped:
        if grp_data["part"].isin(["mag", "phase"]).all() and "GE" in grp_data["Manufacturer"].values[0].upper():
            mag_nii_path = grp_data[grp_data["part"] == "mag"]["NIfTI_Path"].values[0]
            phase_nii_path = grp_data[grp_data["part"] == "phase"]["NIfTI_Path"].values[0]

            logger.log(LogLevel.INFO.value, f"Fixing GE polar data (mag={mag_nii_path}; phase={phase_nii_path})")

            fix_ge_polar(
                mag_nii_path=mag_nii_path,
                phase_nii_path=phase_nii_path,
                delete_originals=True
            )

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
        help='Run non-interactively if desired.'
    )

    parser.add_argument(
        '--qsm_protocol_patterns',
        default=['*t2starw*', '*qsm*', '*aspire*'],
        nargs='*'
    )

    parser.add_argument(
        '--t1w_protocol_patterns',
        default=['*t1w*', '*mp2rage*'],
        nargs='*'
    )

    args = parser.parse_args()

    args.input_dir = os.path.abspath(args.input_dir)
    args.output_dir = os.path.abspath(args.output_dir)

    os.makedirs(args.output_dir, exist_ok=True)

    logger = make_logger(
        logpath=os.path.join(
            args.output_dir, 
            f"log_{str(datetime.datetime.now()).replace(':', '-').replace(' ', '_').replace('.', '')}.txt"
        ),
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
        logger.log(
            LogLevel.WARNING.value, 
            f"Working directory not clean! Writing diff to {os.path.join(args.output_dir, 'diff.txt')}..."
        )
        with open(os.path.join(args.output_dir, "diff.txt"), "w") as diff_file:
            diff_file.write(diff)

    with open(os.path.join(args.output_dir, "references.txt"), 'w', encoding='utf-8') as f:
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

    convert_to_bids(
        input_dir=args.input_dir,
        output_dir=args.output_dir,
        auto_yes=args.auto_yes,
        qsm_protocol_patterns=[pattern.lower() for pattern in args.qsm_protocol_patterns],
        t1w_protocol_patterns=[pattern.lower() for pattern in args.t1w_protocol_patterns]
    )

    fix_ge_data(args.output_dir)

    script_exit()

if __name__ == "__main__":
    main()
