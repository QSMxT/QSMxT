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
from dicompare import load_dicom_session, load_nifti_session, assign_acquisition_and_run_numbers

from qsmxt.scripts.qsmxt_functions import get_qsmxt_version, get_diff
from qsmxt.scripts.logger import LogLevel, make_logger, show_warning_summary 
from qsmxt.scripts.nii_fix_ge import fix_ge_polar, fix_ge_complex

from tabulate import tabulate
from collections import defaultdict

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

def auto_assign_initial_labels(table_data):
    # Group rows by acquisition.
    acquisition_groups = defaultdict(list)
    for row in table_data:
        row.setdefault("Type", "Skip")
        row.setdefault("Description", "")
        acquisition_groups[row["Acquisition"]].append(row)

    # Helper: create a grouping key from Count and InversionNumber.
    def get_group_key(row):
        count = row.get("Count")
        inv = row.get("InversionNumber")
        # If inversion is missing or empty, use None.
        inv_key = inv if inv not in [None, ""] else None
        return (count, inv_key)

    for acq_name, rows in acquisition_groups.items():
        # Dictionaries to hold paired rows for later description assignment.
        mag_phase_pairs = defaultdict(list)  # key: (Count, InversionNumber)
        real_imag_pairs = defaultdict(list)

        # Group rows by the (Count, InversionNumber) key.
        groups = defaultdict(list)
        for row in rows:
            key = get_group_key(row)
            groups[key].append(row)

        # For each group, assign as many Mag/Phase and Real/Imag pairs as possible.
        for key, group in groups.items():
            # Mag/Phase pairing.
            mag_candidates = [
                r for r in group if r["Type"] == "Skip" and (
                    (isinstance(r.get("ImageType"), (list, tuple)) and 'M' in r.get("ImageType")) or
                    (not isinstance(r.get("ImageType"), (list, tuple)) and 'M' in str(r.get("ImageType")))
                )
            ]
            pha_candidates = [
                r for r in group if r["Type"] == "Skip" and (
                    (isinstance(r.get("ImageType"), (list, tuple)) and 'P' in r.get("ImageType")) or
                    (not isinstance(r.get("ImageType"), (list, tuple)) and 'P' in str(r.get("ImageType")))
                )
            ]
            n_pairs = min(len(mag_candidates), len(pha_candidates))
            for i in range(n_pairs):
                mag_candidates[i]["Type"] = "Mag"
                pha_candidates[i]["Type"] = "Phase"
                mag_phase_pairs[key].append((mag_candidates[i], pha_candidates[i]))

            # Real/Imag pairing.
            real_candidates = [
                r for r in group if r["Type"] == "Skip" and (
                    (isinstance(r.get("ImageType"), (list, tuple)) and 'REAL' in r.get("ImageType")) or
                    (not isinstance(r.get("ImageType"), (list, tuple)) and 'REAL' in str(r.get("ImageType")))
                )
            ]
            imag_candidates = [
                r for r in group if r["Type"] == "Skip" and (
                    (isinstance(r.get("ImageType"), (list, tuple)) and 'IMAGINARY' in r.get("ImageType")) or
                    (not isinstance(r.get("ImageType"), (list, tuple)) and 'IMAGINARY' in str(r.get("ImageType")))
                )
            ]
            n_pairs = min(len(real_candidates), len(imag_candidates))
            for i in range(n_pairs):
                real_candidates[i]["Type"] = "Real"
                imag_candidates[i]["Type"] = "Imag"
                real_imag_pairs[key].append((real_candidates[i], imag_candidates[i]))

        # Next, try to mark a row as T1w if its ImageType contains 'UNI' or if its
        # SeriesDescription (uppercased) contains 'UNI-DEN' or 'T1W'.
        for row in rows:
            if row["Type"] == "Skip":
                image_type = row.get("ImageType", "")
                series_desc = str(row.get("SeriesDescription", "")).upper()
                if (isinstance(image_type, (list, tuple)) and 'UNI' in image_type) or \
                   (not isinstance(image_type, (list, tuple)) and 'UNI' in str(image_type)) or \
                   ('UNI-DEN' in series_desc or 'T1W' in series_desc):
                    row["Type"] = "T1w"
                    break

        # Fallback: if no row is marked as T1w and the acquisition name contains 'T1',
        # mark the first row as T1w.
        if not any(r["Type"] == "T1w" for r in rows) and 'T1' in acq_name.upper():
            rows[0]["Type"] = "T1w"

        # For each group of Mag/Phase pairs with the same key, if there is more than one pair,
        # assign sequential Description numbers to each pair.
        for key, pairs in mag_phase_pairs.items():
            if len(pairs) > 1:
                for idx, (mag_row, pha_row) in enumerate(pairs, start=1):
                    mag_row["Description"] = str(idx)
                    pha_row["Description"] = str(idx)

        # Do the same for each group of Real/Imag pairs.
        for key, pairs in real_imag_pairs.items():
            if len(pairs) > 1:
                for idx, (real_row, imag_row) in enumerate(pairs, start=1):
                    real_row["Description"] = str(idx)
                    imag_row["Description"] = str(idx)

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

def validate_series_selections(table_data):
    """
    Validate the current series selections.
    
    For each acquisition (grouped by Acquisition), enforce:
      - For each row marked "Mag", at least one row in the same group must be "Phase"
        with the same Count, and vice versa.
      - Similarly for "Real" and "Imag".
      - If a Mag/Phase (or Real/Imag) pair exists but the Count values differ,
        report an error.
      - If more than one pair is selected in the same acquisition, the pairs must be
        differentiated by a non-empty, unique InversionNumber or Description.
        
    Returns a list of error messages.
    """
    errors = []
    groups = {}
    for row in table_data:
        key = row.get("Acquisition", "")
        groups.setdefault(key, []).append(row)
    for series, rows in groups.items():
        mag_rows = [r for r in rows if r.get("Type") == "Mag"]
        phase_rows = [r for r in rows if r.get("Type") == "Phase"]
        real_rows = [r for r in rows if r.get("Type") == "Real"]
        imag_rows = [r for r in rows if r.get("Type") == "Imag"]
        for m in mag_rows:
            matching_phase = [p for p in phase_rows if p.get("Count") == m.get("Count")]
            if not matching_phase:
                errors.append(f"Series '{series}': Mag series (Count={m.get('Count')}) requires at least one Phase series with the same number of images.")
        for p in phase_rows:
            matching_mag = [m for m in mag_rows if m.get("Count") == p.get("Count")]
            if not matching_mag:
                errors.append(f"Series '{series}': Phase series (Count={p.get('Count')}) requires at least one Mag series with the same number of images.")
        for r in real_rows:
            matching_imag = [i for i in imag_rows if i.get("Count") == r.get("Count")]
            if not matching_imag:
                errors.append(f"Series '{series}': Real series (Count={r.get('Count')}) requires at least one Imag series with the same number of images.")
        for i in imag_rows:
            matching_real = [r for r in real_rows if r.get("Count") == i.get("Count")]
            if not matching_real:
                errors.append(f"Series '{series}': Imag series (Count={i.get('Count')}) requires at least one Real series with the same number of images.")
        for m in mag_rows:
            for p in phase_rows:
                if m.get("Count") != p.get("Count"):
                    errors.append(f"Series '{series}': Selected Mag and Phase series have non-matching number of images ({m.get('Count')} vs. {p.get('Count')}).")
        for r in real_rows:
            for i in imag_rows:
                if r.get("Count") != i.get("Count"):
                    errors.append(f"Series '{series}': Selected Real and Imag series have non-matching number of images ({r.get('Count')} vs. {i.get('Count')}).")
        if len(mag_rows) > 1:
            identifiers = []
            for m in mag_rows:
                ident = m.get("InversionNumber")
                if ident is None or ident == "":
                    ident = m.get("Description", "")
                identifiers.append(ident)
            if len(set(identifiers)) < len(identifiers):
                errors.append(f"Series '{series}': Multiple Mag/Phase series selections must be differentiated by InversionNumber or Description.")
        if len(real_rows) > 1:
            identifiers = []
            for r in real_rows:
                ident = r.get("InversionNumber")
                if ident is None or ident == "":
                    ident = r.get("Description", "")
                identifiers.append(ident)
            if len(set(identifiers)) < len(identifiers):
                errors.append(f"Series '{series}': Multiple Real/Imag series selections must be differentiated by InversionNumber or Description.")
    return errors

def interactive_acquisition_selection_series(table_data):
    """
    Interactive UI for editing series-level selections, one Acquisition at a time.
    SHIFT / SHIFT+TAB or ENTER to move forward/backward through acquisitions.
    ENTER at final acquisition asks for confirmation, displays warning if some acquisitions have no selection.
    ESC exits, arrow keys and text editing as normal.
    """
    auto_assign_initial_labels(table_data)
    
    for row in table_data:
        row.setdefault("Type", "Skip")
        row.setdefault("Description", "")

    acquisition_groups = defaultdict(list)
    for row in table_data:
        acquisition_groups[row["Acquisition"]].append(row)

    acquisition_keys = list(acquisition_groups.keys())

    def curses_ui(stdscr):
        curses.curs_set(0)
        if curses.has_colors():
            curses.start_color()
            curses.init_pair(1, curses.COLOR_RED, curses.COLOR_BLACK)

        headers = ["SeriesDescription", "ImageType", "Count", "NumEchoes", "InversionNumber", "Type", "Description"]

        allowed_types = ["Mag", "Phase", "Real", "Imag", "T1w", "Extra", "Skip"]
        current_acq_index = 0
        current_row = 0
        nav_blocked = False

        while True:
            stdscr.clear()
            current_acq = acquisition_keys[current_acq_index]
            rows = acquisition_groups[current_acq]
            nrows = len(rows)
            stdscr.addstr(0, 0, f"Acquisition ({current_acq_index+1} of {len(acquisition_keys)}): {current_acq}")
            stdscr.addstr(1, 0, "Arrow keys = navigate/select type | Text = edit description | SHIFT/ENTER = next | SHIFT+TAB = previous | ESC = quit")

            display_data = [[r.get(h, "") for h in headers] for r in rows]
            table_str = tabulate(display_data, headers=headers, tablefmt="plain")
            for idx, line in enumerate(table_str.splitlines()):
                if idx == current_row + 1:
                    stdscr.attron(curses.A_REVERSE)
                    stdscr.addstr(3 + idx, 0, line[:stdscr.getmaxyx()[1] - 1])
                    stdscr.attroff(curses.A_REVERSE)
                else:
                    stdscr.addstr(3 + idx, 0, line[:stdscr.getmaxyx()[1] - 1])

            errors = validate_series_selections(rows)
            err_y = 4 + len(display_data) + 2
            if errors:
                for err in errors:
                    stdscr.addstr(err_y, 0, f"ERROR: {err}", curses.A_BOLD | curses.color_pair(1))
                    err_y += 1
                if nav_blocked:
                    stdscr.addstr(err_y, 0, "Cannot leave this acquisition until all errors are resolved.", curses.A_BOLD | curses.color_pair(1))
                    err_y += 1
            else:
                stdscr.addstr(err_y, 0, "No errors.", curses.A_BOLD)

            stdscr.refresh()
            key = stdscr.getch()
            nav_blocked = False

            if key == 27:
                return None
            elif key == curses.KEY_UP:
                if current_row > 0:
                    current_row -= 1
            elif key == curses.KEY_DOWN:
                if current_row < nrows - 1:
                    current_row += 1
            elif key == curses.KEY_LEFT:
                cur = rows[current_row]["Type"]
                idx = allowed_types.index(cur) if cur in allowed_types else 0
                rows[current_row]["Type"] = allowed_types[(idx - 1) % len(allowed_types)]
            elif key == curses.KEY_RIGHT:
                cur = rows[current_row]["Type"]
                idx = allowed_types.index(cur) if cur in allowed_types else 0
                rows[current_row]["Type"] = allowed_types[(idx + 1) % len(allowed_types)]
            elif key in (curses.KEY_BACKSPACE, 127, 8):
                desc = rows[current_row]["Description"]
                rows[current_row]["Description"] = desc[:-1]
            elif 32 <= key <= 126:
                ch = chr(key)
                rows[current_row]["Description"] += ch
            elif key in (9, 10):  # TAB or ENTER
                if errors:
                    nav_blocked = True
                else:
                    if current_acq_index == len(acquisition_keys) - 1:
                        stdscr.addstr(err_y + 1, 0, "Confirm selections?", curses.A_BOLD)
                        empty = [k for k, v in acquisition_groups.items() if all(r.get("Type", "Skip") == "Skip" for r in v)]
                        if empty:
                            stdscr.addstr(err_y + 2, 0, f"({len(empty)} acquisitions have no selections)", curses.A_BOLD | curses.color_pair(1))
                        stdscr.addstr(err_y + 3, 0, "(y or n): ", curses.A_BOLD)
                        stdscr.refresh()
                        ch = stdscr.getch()
                        if ch in (ord('y'), ord('Y')):
                            return [r for group in acquisition_groups.values() for r in group]
                    else:
                        current_acq_index += 1
                        current_row = 0
            elif key == 353:  # SHIFT+TAB
                if errors:
                    nav_blocked = True
                else:
                    current_acq_index = (current_acq_index - 1) % len(acquisition_keys)
                    current_row = 0

    return curses.wrapper(curses_ui)

def convert_and_organize(dicom_session, output_dir, dcm2niix_path="dcm2niix"):
    logger = make_logger()
    # Only convert series whose Type is not 'Skip'
    to_convert = dicom_session[dicom_session['Type'] != 'Skip']
    if to_convert.empty:
        logger.log(LogLevel.ERROR.value, "No acquisitions selected for conversion. Exiting.")
        script_exit(1)
    
    group_cols = [
        "PatientName", "PatientID", "StudyDate", "Acquisition", "SeriesDescription", 
        "Type", "EchoNumber", "RunNumber"
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
            subject_id = f"sub-{clean(row['PatientID']) or clean(row['PatientName'])}"
            session_id = f"ses-{clean(row['StudyDate'])}"
            # Build an acquisition label based on Acquisition and Description
            acq_label = f"acq-{clean(row['Acquisition'])}"
            if row["Description"]:
                acq_label += f"_desc-{row['Description']}"
            elif row["Type"] == "Extra":
                acq_label += f"_desc-{clean(row['SeriesDescription'])}"
            
            base_name = f"{subject_id}_{session_id}_{acq_label}"
            if row["NumRuns"] > 1:
                base_name += f"_run-{int(row['RunNumber']):02}"
            if row["NumEchoes"] > 1 and row["Type"] in ["Mag", "Phase", "Real", "Imag"]:
                base_name += f"_echo-{int(row['EchoNumber']):02}"
            
            # if InversionNumber is present and is numberic
            if "InversionNumber" in row and pd.notnull(row["InversionNumber"]):
                base_name += f"_inv-{int(row['InversionNumber'])}"
            if row["Type"] in ["Mag", "Phase", "Real", "Imag"]:
                base_name += f"_part-{row['Type'].lower()}"
            
            if row["Type"] == "T1w":
                base_name += "_T1w"
            elif row["Type"] in ["Mag", "Phase", "Real", "Imag"]:
                if row["NumEchoes"] > 1:
                    base_name += "_MEGRE"
                else:
                    base_name += "_T2starw"
            elif row["Type"] == "Extra":
                base_name += nii_name.replace("temp_output", "").replace(".nii", "").replace(".nii.gz", "")
            
            ext = ".nii" if nii_name.endswith(".nii") else ".nii.gz"
            out_dir = os.path.join(output_dir, subject_id, session_id, "anat" if row["Type"] != "Extra" else "extra_data")
            os.makedirs(out_dir, exist_ok=True)
            dst_path = os.path.join(out_dir, f"{base_name}{ext}")
            src_path = os.path.join(tmp_outdir, nii_name)
            shutil.move(src_path, dst_path)
            
            json_src = os.path.join(tmp_outdir, nii_name.replace(ext, ".json"))
            if os.path.isfile(json_src):
                json_dst = os.path.join(out_dir, f"{base_name}.json")
                shutil.move(json_src, json_dst)

        shutil.rmtree(tmp_outdir)

def convert_to_bids(input_dir, output_dir, auto_yes):
    logger = make_logger()
    time_start = time.time()
    dicom_session = load_dicom_session(input_dir, show_progress=True)
    dicom_session = assign_acquisition_and_run_numbers(dicom_session)
    time_end = time.time()
    logger.log(LogLevel.INFO.value, f"Loaded DICOM session in {time_end - time_start:.2f} seconds")
    dicom_session.reset_index(drop=True, inplace=True)
    if '(0043,102F)' in dicom_session.columns:
        private_map = {0: 'M', 1: 'P', 2: 'REAL', 3: 'IMAGINARY'}
        dicom_session['ImageType'].append(dicom_session['(0043,102F)'].map(private_map).fillna(''))
    dicom_session['NumEchoes'] = dicom_session.groupby(['PatientName', 'PatientID', 'StudyDate', 'Acquisition', 'SeriesDescription', 'SeriesInstanceUID'])['EchoTime'].transform('nunique')
    dicom_session['EchoNumber'] = dicom_session.groupby(['PatientName', 'PatientID', 'StudyDate', 'Acquisition', 'SeriesDescription', 'SeriesInstanceUID'])['EchoTime'].rank(method='dense')
    dicom_session['NumRuns'] = dicom_session.groupby(['PatientName', 'PatientID', 'StudyDate', 'Acquisition'])['RunNumber'].transform('nunique')
    #dicom_session['RunNumber'] = dicom_session.groupby(['PatientName', 'PatientID', 'StudyDate', 'Acquisition', 'SeriesDescription', 'ImageType'])['SeriesInstanceUID'].rank(method='dense')
    if "InversionTime" in dicom_session.columns:
        mask = dicom_session["InversionTime"].notnull() & (dicom_session["InversionTime"] != 0)
        dicom_session.loc[mask, "InversionNumber"] = dicom_session[mask].groupby(['PatientName', 'PatientID', 'StudyDate', 'Acquisition'])['InversionTime'].rank(method='dense')
        dicom_session["InversionNumber"] = dicom_session["InversionNumber"].astype(pd.Int64Dtype())
    groupby_fields = ["Acquisition", "SeriesDescription", "ImageType"]
    if "InversionNumber" in dicom_session.columns:
        groupby_fields.append("InversionNumber")
    grouped = dicom_session.groupby(groupby_fields, dropna=False).agg(Count=("InstanceNumber", "count"), NumEchoes=("EchoTime", "nunique")).reset_index()
    if auto_yes or not sys.__stdin__.isatty():
        logger.log(LogLevel.INFO.value, "Auto-assigning initial labels")
        grouped_dict = grouped.to_dict(orient="records")
        auto_assign_initial_labels(grouped_dict)
        selections = pd.DataFrame(grouped_dict)
        logger.log(LogLevel.INFO.value, f"Auto-assigned selections:\n{selections}")
    else:
        logger.log(LogLevel.INFO.value, "Entering interactive mode for series selection")
        selections = interactive_acquisition_selection_series(grouped.to_dict(orient="records"))
        selections = pd.DataFrame(selections)
        logger.log(LogLevel.INFO.value, f"User selections:\n{selections}")
    grouped = grouped.merge(
        selections[["Acquisition", "SeriesDescription", "ImageType", "Type", "Description"]],
        on=["Acquisition", "SeriesDescription", "ImageType"],
        how="left"
    )
    grouped["Type"] = grouped["Type"].fillna("Skip")
    grouped["Description"] = grouped["Description"].fillna("")

    logger.log(LogLevel.INFO.value, "Merging selections into dataframe...")
    for col in ["AcquisitionType", "Type", "Description"]:
        if col in dicom_session.columns:
            dicom_session.drop(columns=col, inplace=True)
    merge_cols = groupby_fields + ["Type", "Description"]
    dicom_session = dicom_session.merge(grouped[merge_cols], on=groupby_fields, how='left')
    dicom_session["Type"] = dicom_session["Type"].fillna("Skip")
    dicom_session["Description"] = dicom_session["Description"].fillna("")
    convert_and_organize(dicom_session, output_dir)

def fix_ge_data(bids_dir):
    logger = make_logger()

    logger.log(LogLevel.INFO.value, "Checking for complex or polar data requiring fixing...")
    nifti_session = load_nifti_session(bids_dir, show_progress=True)

    if 'part' not in nifti_session.columns:
        logger.log(LogLevel.INFO.value, "No 'part' column found in NIfTI session. No complex or polar data to fix.")
        script_exit(0)

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
        auto_yes=args.auto_yes
    )

    fix_ge_data(args.output_dir)

    script_exit()

if __name__ == "__main__":
    main()
