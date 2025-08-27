#!/usr/bin/env python3

import argparse
import os
import sys
import json
import shutil
import datetime
import curses
import re
import pandas as pd
import nibabel as nib
import numpy as np

from dicompare import load_nifti_session

from qsmxt.scripts.qsmxt_functions import get_qsmxt_version, get_diff
from qsmxt.scripts.logger import LogLevel, make_logger, show_warning_summary

def script_exit(exit_code=0):
    logger = make_logger()
    show_warning_summary(logger)
    logger.log(LogLevel.INFO.value, 'Finished')
    exit(exit_code)

def interactive_table(session: pd.DataFrame):
    """
    A curses-based interface with:
      - A "regex row" at row_idx = -1, with one regex input per column (except NIfTI_Path).
      - A data table (row_idx >= 0) with columns:
         0) NIfTI_Path (read-only, toggles full/basename on TAB)
         1) sub
         2) ses
         3) acq
         4) run
         5) echo
         6) part
         7) suffix
         8) MagneticFieldStrength
         9) EchoTime

    Controls:
      - ARROWS = move, ENTER = apply regex (if on regex row) or move down (if on data table)
      - ESC = finish
      - TAB = toggle between showing full path or basename in NIfTI_Path column
      - BACKSPACE = remove last character from the current field (if not empty),
        or if already empty, copy from the field above (if available).
      - DELETE = clear (empty) the current field if not empty, or if already empty, copy from above.
      - We also show context-specific help for each column in a line above the table.
    """

    session.reset_index(drop=True, inplace=True)

    columns = [
        "NIfTI_Path",
        "sub",
        "ses",
        "acq",
        "run",
        "echo",
        "part",
        "suffix",
        "MagneticFieldStrength",
        "EchoTime",
    ]

    # For toggling whether we display full path or basename
    show_full_path = True
    row_idx = -1  # -1 => "regex row"; 0.. => data table
    col_idx = 0
    viewport_top = 0  # Top row of the viewport for scrolling

    # First, handle the display path if it exists (before converting to strings)
    # Keep a hidden full path for file operations (the actual file path without [index])
    session["NIfTI_Path_Full"] = session["NIfTI_Path"].copy()
    
    # Use display path if available (for 4D volumes with [index] notation) for the UI
    if "NIfTI_Path_Display" in session.columns:
        # Replace NIfTI_Path with the display version that includes [0], [1], etc.
        session["NIfTI_Path"] = session["NIfTI_Path_Display"]
    
    # Ensure all columns exist
    for c in columns:
        if c not in session.columns:
            session[c] = None

    # Turn all non-None values to strings
    for c in columns:
        session[c] = session[c].apply(lambda x: str(x) if pd.notnull(x) else None)

    # Regex patterns for each column (except NIfTI_Path)
    regex_map = {}
    for c in columns:
        if c != "NIfTI_Path":
            regex_map[c] = ""

    # Default guesses
    default_patterns = {
        "sub": r"sub-([A-Za-z0-9]+)",
        "ses": r"ses-([A-Za-z0-9]+)",
        "acq": r"acq-([A-Za-z0-9]+)",
        "run": r"run-([0-9]+)",
        "echo": r"echo-([0-9]+)",
        "part": r"part-([A-Za-z]+)",
        "suffix": r"_([A-Za-z0-9]+)\.nii",
        "MagneticFieldStrength": r"_B(\d+\.\d+)",
        "EchoTime": r"_TE(\d+\.\d+)",
    }
    for k,v in default_patterns.items():
        if k in regex_map:
            regex_map[k] = v

    column_help = {
        "NIfTI_Path": "[READ-ONLY]: Press TAB to toggle full path or basename.",
        "sub": "[REQUIRED]: Subject ID e.g. 1 (BIDS: sub-<label>).",
        "ses": "[OPTIONAL]: Session date e.g. 20241231 (BIDS: ses-<label>).",
        "acq": "[OPTIONAL]: Acquisition label (BIDS: acq-<label>).",
        "run": "[REQUIRED for multiple runs]: For multiple runs of an acquisition in a single session e.g. 1, 2, etc.",
        "echo": "[REQUIRED for MEGRE]: Counter for identifying echo chronology.",
        "part": "[REQUIRED for T2starw or MEGRE]: Identifies signal part ('mag', 'phase', 'real', or 'imag').",
        "suffix": "[REQUIRED]: Identifies acquisition type T2starw or MEGRE for single or multi-echo GRE, respectively, T1w for anatomical T1.",
        "MagneticFieldStrength": "[REQUIRED for T2starw/MEGRE]: Magnetic field strength in Teslas e.g. 1.5, 3, 7.",
        "EchoTime": "[REQUIRED for T2starw/MEGRE]: Echo time in seconds e.g. 0.015, 0.03."
    }

    def apply_regex_to_filenames(column):
        pattern = regex_map[column]
        if not pattern:
            return False, f"No regex pattern for {column}."

        try:
            compiled = re.compile(pattern)
        except re.error as e:
            return False, f"Invalid regex for {column}: {e}"

        assigned_count = 0
        for i in range(len(session)):
            fullp = str(session.loc[i, "NIfTI_Path_Full"])
            match = compiled.search(fullp)
            if match:
                # Check if there are capture groups
                if match.groups():
                    val = match.group(1)
                else:
                    # No capture groups, use the entire match
                    val = match.group(0)
                session.loc[i, column] = val
                assigned_count += 1

        return True, f"Regex assigned {column} for {assigned_count} file(s)."

    def copy_from_above_if_exists(r, c):
        """
        Copies the value from row (r-1, c) to (r, c) if (r > 0) and
        the above cell is not empty.
        """
        if r <= 0:
            return  # can't copy from above if we're in the top row or negative
        above_val = session.loc[r-1, c]
        if pd.notnull(above_val) and str(above_val).strip() != "":
            session.loc[r, c] = above_val

    def validate_data(data):
        errors = []
        # col names
        col_names = ["NIfTI_Path", "sub", "ses", "acq", "run", "echo", "part", "suffix", "MagneticFieldStrength", "EchoTime"]
        for i, row in data.iterrows():
            row = row[col_names].drop("NIfTI_Path")
            if row.isnull().all():
                continue

            for c in ["sub", "suffix"]:
                if pd.isnull(row[c]):
                    errors.append((i, col_names.index(c), "Subject ID and suffix are required."))
            
            # if suffix is T2starw or MEGRE, MagneticFieldStrength and EchoTime are required
            if row["suffix"] in ["T2starw", "MEGRE"]:
                if pd.isnull(row["MagneticFieldStrength"]):
                    errors.append((i, col_names.index("MagneticFieldStrength"), "T2starw and MEGRE images require MagneticFieldStrength."))
                if pd.isnull(row["EchoTime"]):
                    errors.append((i, col_names.index("EchoTime"), "T2starw and MEGRE images require EchoTime."))
                
            # if suffix is MEGRE, echo is required
            if row["suffix"] == "MEGRE":
                if pd.isnull(row["echo"]):
                    errors.append((i, col_names.index("echo"), "MEGRE images require echo."))
            
            # if suffix is T2starw or MEGRE, part is required
            if row["suffix"] in ["T2starw", "MEGRE"]:
                if pd.isnull(row["part"]):
                    errors.append((i, col_names.index("part"), "T2starw and MEGRE images require part."))
            
            # if suffix is T2starw, echo is not allowed
            if row["suffix"] == "T2starw":
                if pd.notnull(row["echo"]):
                    errors.append((i, col_names.index("echo"), "T2starw images should not have echo."))

            # part values may only be 'mag', 'phase', 'real', or 'imag'
            if pd.notnull(row["part"]) and str(row["part"]).strip() not in ["mag", "phase", "real", "imag"]:
                errors.append((i, col_names.index("part"), "part should be 'mag', 'phase', 'real', or 'imag'."))

            # suffix values may only be 'T2starw', 'MEGRE', or 'T1w'
            if pd.notnull(row["suffix"]) and str(row["suffix"]).strip() not in ["T2starw", "MEGRE", "T1w"]:
                errors.append((i, col_names.index("suffix"), "suffix should be 'T2starw', 'MEGRE', or 'T1w'."))
            
            # echo values should be positive integers
            if pd.notnull(row["echo"]):
                try:
                    val = int(row["echo"])
                    if val <= 0:
                        errors.append((i, col_names.index("echo"), "echo should be a positive integer."))
                except ValueError:
                    errors.append((i, col_names.index("echo"), "echo should be a positive integer."))

            # MagneticFieldStrength and EchoTime should be positive floats
            for c in ["MagneticFieldStrength", "EchoTime"]:
                if pd.notnull(row[c]):
                    try:
                        val = float(row[c])
                        if val <= 0:
                            errors.append((i, col_names.index(c), f"{c} should be a positive float."))
                    except ValueError:
                        errors.append((i, col_names.index(c), f"{c} should be a positive float."))

        # the same combination of sub, ses, acq, run, echo, part, and suffix should not be repeated
        # BUT skip rows where sub is None/empty since they will be skipped anyway
        data_with_sub = data[data['sub'].notna() & (data['sub'] != '') & (data['sub'] != 'None')]
        dupes = data_with_sub.duplicated(subset=["sub", "ses", "acq", "run", "echo", "part", "suffix"], keep=False)
        for i, dupe in dupes.items():
            if dupe:
                errors.append((i, col_names.index("sub"), "Duplicate combination of sub, ses, acq, run, echo, part, and suffix."))

        # 2) For a given sub/ses/acq/run combo, MagneticFieldStrength should be constant
        # Only validate rows with valid subject IDs
        group_mf = data_with_sub.groupby(["sub","ses","acq","run"], dropna=False)
        for group_keys, grp in group_mf:
            # sub, ses, acq, run => all rows must share the same MagneticFieldStrength (if not null)
            non_null_values = grp["MagneticFieldStrength"].dropna().unique()
            # If there's more than 1 distinct non-null MF strength => error on each row
            if len(non_null_values) > 1:
                for idx in grp.index:
                    errors.append((idx, col_names.index("MagneticFieldStrength"),
                                "MagneticFieldStrength must be constant for sub/ses/acq/run."))

        # 3) For a given sub/ses/acq/run/echo combo, EchoTime should be constant
        # Only validate rows with valid subject IDs
        group_et = data_with_sub.groupby(["sub","ses","acq","run","echo"], dropna=False)
        for group_keys, grp in group_et:
            # sub, ses, acq, run, echo => all rows must share the same EchoTime (if not null)
            non_null_times = grp["EchoTime"].dropna().unique()
            if len(non_null_times) > 1:
                for idx in grp.index:
                    errors.append((idx, col_names.index("EchoTime"),
                                "EchoTime must be constant for sub/ses/acq/run/echo."))
        
        # --- 3) If multiple rows share the same sub/ses/acq/run/part/suffix,
        #         their echo values must be consecutive 1..n.
        #         (Ignoring T2starw that disallows echo anyway).
        grouping_cols = ["sub","ses","acq","run","part","suffix"]
        grouped = data.groupby(grouping_cols, dropna=False)  # dropna=False so we keep invalid combos too
        for group_keys, grp in grouped:
            suffix_val = group_keys[-1]  # suffix is last in group_keys
            if suffix_val != "MEGRE":
                continue

            echo_vals = []
            row_indices = []
            for idx, thisrow in grp.iterrows():
                e = str(thisrow["echo"]).strip() if pd.notnull(thisrow["echo"]) else ""
                # if e is not empty
                if e:
                    try:
                        echo_vals.append(int(e))
                    except ValueError:
                        # already flagged above
                        pass
                row_indices.append(idx)

            sorted_e = sorted(echo_vals)
            if sorted_e != list(range(1, len(sorted_e) + 1)):
                for idx in row_indices:
                    errors.append((idx, col_names.index("echo"), "For multiple echoes, they must start at 1 and be consecutive within the same sub/ses/acq/run/part/suffix."))

        if errors:
            return False, errors
        return True, []

    def curses_ui(stdscr):
        nonlocal row_idx, col_idx, show_full_path, viewport_top

        curses.curs_set(1)
        curses.start_color()

        # normal
        curses.init_pair(1, curses.COLOR_WHITE, curses.COLOR_BLACK)
        normal = curses.color_pair(1)

        # error
        curses.init_pair(2, curses.COLOR_RED, curses.COLOR_BLACK)
        error = curses.color_pair(2)

        # warning
        curses.init_pair(3, curses.COLOR_YELLOW, curses.COLOR_BLACK)
        warning = curses.color_pair(3)

        # success
        curses.init_pair(4, curses.COLOR_GREEN, curses.COLOR_BLACK)
        success = curses.color_pair(4)

        valid, errors = validate_data(session)
        if not valid:
            row_idx, col_idx, message = errors[0]
            status = (error, message)
        else:
            status = (success, "Valid")

        nrows = len(session)
        ncols = len(columns)
        changes = False

        while True:
            stdscr.clear()
            max_y, max_x = stdscr.getmaxyx()
            
            # Check minimum window size - need at least space for headers and one data row
            min_height = 15  # Headers + at least one data row
            min_width = 80   # Reasonable width for columns
            
            if max_y < min_height or max_x < min_width:
                stdscr.addstr(0, 0, "Window too small!", error)
                stdscr.addstr(1, 0, f"Minimum size: {min_width}x{min_height}, Current: {max_x}x{max_y}", error)
                stdscr.addstr(2, 0, "Please resize the window to be larger.", error)
                stdscr.refresh()
                stdscr.getch()
                continue

            # Layout plan:
            #  Line 0-1 : instructions
            #  Line 2   : status
            #  Line 3   : help text for current column
            #  Line 4   : BLANK
            #  Line 5   : regex header
            #  Line 6   : regex row
            #  Line 7   : BLANK
            #  Line 8   : table header
            #  Line 9.. : data rows

            # Quick naive sizing for columns
            col0_width = max_x // 3
            if col0_width < 20:
                col0_width = 20
            other_col_count = ncols - 1
            if other_col_count < 1:
                other_col_count = 1
            remaining_width = max_x - col0_width
            each_col_width = remaining_width // other_col_count
            if each_col_width < 12:
                each_col_width = 12

            def get_col_width(ix):
                return col0_width if ix == 0 else each_col_width

            try:

                # Instructions lines
                stdscr.addstr(0, 0, "=== nifti-convert: Convert NIfTI to BIDS for QSMxT ==="[:max_x])
                stdscr.addstr(1, 0, "INSTRUCTIONS: Fill out the data table below to complete the conversion to BIDS."[:max_x])

                stdscr.addstr(3, 0, "CONTROLS: ARROWS=move; ESC=done; TAB=toggle full paths; DEL=clear / copy above cell"[:max_x])
                stdscr.addstr(4, 0, "REGULAR EXPRESSIONS: You can use the regex row to autopopulate columns based on filenames."[:max_x])

                # Status message
                stdscr.addstr(6, 0, f"STATUS: {status[1]}"[:max_x], status[0])

                if row_idx == -1:
                    help_text = "HELP <REGEX row>: Edit a pattern for each column. Press ENTER to apply."[:max_x]
                else:
                    current_col = columns[col_idx]
                    help_text = f"HELP <{current_col}> {column_help.get(current_col, '')}"[:max_x]
                stdscr.addstr(7, 0, help_text[:max_x])

                # Blank line 4
                regex_header_y = 9
                # Regex header
                regex_header_str = ""
                for ci, cname in enumerate(columns):
                    w = get_col_width(ci)
                    regex_header_str += cname[:w].ljust(w) + " "
                stdscr.addstr(regex_header_y, 0, regex_header_str[:max_x])

                # Regex row
                regex_row_y = regex_header_y + 1
                row_output = ""
                for ci, cname in enumerate(columns):
                    w = get_col_width(ci)
                    if ci == 0:
                        cell_txt = "[NoRegex]"
                    else:
                        regex_val = regex_map.get(cname, "") or ""
                        cell_txt = regex_val[:w]

                    if row_idx == -1 and col_idx == ci:
                        stdscr.attron(curses.A_REVERSE)
                        stdscr.addstr(regex_row_y, len(row_output), cell_txt.ljust(w))
                        stdscr.attroff(curses.A_REVERSE)
                    else:
                        stdscr.addstr(regex_row_y, len(row_output), cell_txt.ljust(w))
                    row_output += cell_txt.ljust(w) + " "

                # Blank line 7
                table_header_y = regex_row_y + 2
                # Table header
                table_header_str = ""
                for ci, cname in enumerate(columns):
                    w = get_col_width(ci)
                    table_header_str += cname[:w].ljust(w) + " "
                stdscr.addstr(table_header_y, 0, table_header_str[:max_x])

                data_start_y = table_header_y + 1
                
                # Calculate viewport dimensions
                available_rows = max_y - data_start_y - 1  # Leave one line at bottom for scroll indicator
                
                # Adjust viewport if current row is outside
                if row_idx >= 0:
                    if row_idx < viewport_top:
                        viewport_top = row_idx
                    elif row_idx >= viewport_top + available_rows:
                        viewport_top = row_idx - available_rows + 1
                
                # Ensure viewport_top is valid
                viewport_top = max(0, min(viewport_top, max(0, nrows - available_rows)))
                
                # Show scroll indicators if needed
                if viewport_top > 0:
                    # Show "more above" indicator
                    stdscr.addstr(data_start_y - 1, max_x - 20, f"▲ {viewport_top} more above ▲", curses.A_BOLD)
                
                # Data rows (only show visible rows within viewport)
                visible_rows = min(available_rows, nrows - viewport_top)
                for display_idx in range(visible_rows):
                    r_i = viewport_top + display_idx
                    if r_i >= nrows:
                        break
                    
                    yy = data_start_y + display_idx
                    if yy >= max_y - 1:  # Leave room for bottom indicator
                        break

                    col_x = 0
                    for ci, cname in enumerate(columns):
                        w = get_col_width(ci)
                        val = ""
                        if ci == 0:
                            # NIfTI_Path - use the display version that has [0], [1] etc
                            if show_full_path:
                                val = str(session.loc[r_i, "NIfTI_Path"])
                            else:
                                val = os.path.basename(str(session.loc[r_i, "NIfTI_Path"]))
                        else:
                            raw_val = session.loc[r_i, cname]
                            val = "" if pd.isnull(raw_val) else str(raw_val)

                        val = val[:w]

                        # highlight if this is the current cell
                        if (r_i == row_idx) and (ci == col_idx) and (row_idx >= 0):
                            stdscr.attron(curses.A_REVERSE)
                            stdscr.addstr(yy, col_x, val.ljust(w))
                            stdscr.attroff(curses.A_REVERSE)
                        else:
                            stdscr.addstr(yy, col_x, val.ljust(w))
                        col_x += w + 1
                
                # Show "more below" indicator if needed
                if viewport_top + available_rows < nrows:
                    remaining = nrows - (viewport_top + available_rows)
                    stdscr.addstr(max_y - 1, max_x - 20, f"▼ {remaining} more below ▼", curses.A_BOLD)
            except curses.error:
                stdscr.clear()
                stdscr.addstr(0, 0, "Window too small!", error)
                stdscr.addstr(1, 0, "Please resize the window to be larger.", error)
                stdscr.refresh()
                stdscr.getch()
                continue

            stdscr.refresh()
            key = stdscr.getch()

            if key == 27:  # ESC
                valid, errors = validate_data(session)
                if not valid:
                    row_idx, col_idx, msg = errors[0]
                    status = (error, msg)
                else:
                    return

            elif key in (curses.KEY_ENTER, 10, 13):
                # ENTER
                if row_idx == -1:
                    # apply regex for col_idx if not 0
                    if col_idx == 0:
                        status = (warning, "Cannot apply regex to NIfTI_Path.")
                    else:
                        c_name = columns[col_idx]
                        valid, msg = apply_regex_to_filenames(c_name)
                        if valid:
                            status = (success, msg)
                        else:
                            status = (error, msg)
                else:
                    row_idx = min(nrows - 1, row_idx + 1)

            elif key == curses.KEY_UP:
                if row_idx == -1:
                    # can't go above regex row
                    pass
                elif row_idx == 0:
                    # moving from first data row to regex row
                    row_idx = -1
                else:
                    row_idx -= 1

            elif key == curses.KEY_DOWN:
                if row_idx == -1:
                    # move from regex row to first data row
                    row_idx = 0
                else:
                    row_idx = min(nrows - 1, row_idx + 1)

            elif key == curses.KEY_LEFT:
                col_idx = max(0, col_idx - 1)

            elif key == curses.KEY_RIGHT:
                col_idx = min(ncols - 1, col_idx + 1)

            elif key == 9:  # TAB => toggle path mode
                show_full_path = not show_full_path

            elif 32 <= key <= 126:
                # typed a normal ASCII char
                if row_idx == -1:
                    # editing regex
                    if col_idx != 0:
                        c_name = columns[col_idx]
                        current_val = regex_map.get(c_name, "") or ""
                        regex_map[c_name] = current_val + chr(key)
                else:
                    # editing table
                    if col_idx != 0:  # NIfTI_Path is read-only
                        c_name = columns[col_idx]
                        curr_val = session.loc[row_idx, c_name]
                        if pd.isnull(curr_val):
                            curr_val = ""
                        session.loc[row_idx, c_name] = curr_val + chr(key)
                        changes = True

            elif key in (curses.KEY_BACKSPACE, 127):
                # BACKSPACE => remove last char if not empty, else copy from above
                if row_idx == -1 and col_idx != 0:
                    # editing regex pattern
                    c_name = columns[col_idx]
                    curr = regex_map.get(c_name, "") or ""
                    if curr:
                        regex_map[c_name] = curr[:-1]
                    # if it's empty already, do nothing for regex row
                elif row_idx >= 0 and col_idx != 0:
                    # editing table
                    c_name = columns[col_idx]
                    curr_val = session.loc[row_idx, c_name]
                    if pd.isnull(curr_val) or str(curr_val).strip() == "":
                        curr_val = None
                    elif curr_val == None:
                        # copy from above if possible
                        copy_from_above_if_exists(row_idx, c_name)
                    else:
                        # remove last character
                        session.loc[row_idx, c_name] = curr_val[:-1]
                        if str(session.loc[row_idx, c_name]).strip() == "":
                            session.loc[row_idx, c_name] = None
                    changes = True

            elif key == curses.KEY_DC:
                # DELETE => clear if not empty, else copy from above
                if row_idx == -1 and col_idx != 0:
                    # editing regex
                    c_name = columns[col_idx]
                    curr = regex_map.get(c_name, "") or ""
                    if not curr:
                        # do nothing for regex row if it's already empty
                        pass
                    else:
                        # clear it
                        regex_map[c_name] = None
                elif row_idx >= 0 and col_idx != 0:
                    c_name = columns[col_idx]
                    curr_val = session.loc[row_idx, c_name]
                    if pd.isnull(curr_val):
                        curr_val = None
                    if curr_val == None:
                        # already empty => copy from above
                        copy_from_above_if_exists(row_idx, c_name)
                    else:
                        # clear
                        session.loc[row_idx, c_name] = None
                changes = True

            if changes:
                valid, errors = validate_data(session)
                if not valid:
                    _, _, msg = errors[0]
                    status = (error, msg)
                else:
                    status = (success, "Valid")
            # else ignore other keys

    curses.wrapper(curses_ui)
    return session

def nifti_convert(nifti_dir, output_dir):
    logger = make_logger()

    session = load_nifti_session(nifti_dir)
    session.reset_index(drop=True, inplace=True)
    # Sort by some BIDS fields if they exist, to get a consistent order
    sort_cols = [
        "sub", "ses", "acq", "run", "echo", "part",
        "suffix", "MagneticFieldStrength", "EchoTime", "NIfTI_Path"
    ]
    # Only use columns that exist
    use_sort_cols = [c for c in sort_cols if c in session.columns]
    session.sort_values(by=use_sort_cols, inplace=True)
    logger.log(LogLevel.INFO.value, f"Found {len(session)} NIfTI files.")

    session = interactive_table(session)

    # for each row in the dataframe, move the file to the new location
    for i in range(len(session)):
        row = session.loc[i]
        
        # Skip files where subject ID is None
        if pd.isna(row['sub']) or row['sub'] is None:
            logger.log(LogLevel.INFO.value, f"Skipping {row['NIfTI_Path_Full']} - no subject ID assigned")
            continue
            
        new_name = f"sub-{row['sub']}"
        if pd.notnull(row["ses"]):
            new_name += f"_ses-{row['ses']}"
        if pd.notnull(row["acq"]):
            new_name += f"_acq-{row['acq']}"
        if pd.notnull(row["run"]):
            new_name += f"_run-{row['run']}"
        if pd.notnull(row["echo"]):
            new_name += f"_echo-{row['echo']}"
        if pd.notnull(row["part"]):
            new_name += f"_part-{row['part']}"
        original_ext = ".nii.gz" if row["NIfTI_Path"].endswith(".nii.gz") else ".nii"
        suffix = str(row['suffix']).rstrip('.')  # Remove trailing dot if present
        new_name += f"_{suffix}{original_ext}"

        anat_dir = os.path.join(output_dir, f"sub-{row['sub']}")
        if pd.notnull(row["ses"]):
            anat_dir = os.path.join(anat_dir, f"ses-{row['ses']}")
        anat_dir = os.path.join(anat_dir, "anat")

        os.makedirs(anat_dir, exist_ok=True)

        output_path = os.path.join(anat_dir, new_name)
        
        # Check if this is a 4D volume that needs splitting
        if 'Volume_Index' in row and pd.notnull(row['Volume_Index']):
            vol_idx = int(row['Volume_Index'])
            logger.log(LogLevel.INFO.value, f"Extracting volume {vol_idx} from {row['NIfTI_Path_Full']} to {output_path}")
            
            # Load the 4D NIfTI and extract the specific volume
            img_4d = nib.load(row['NIfTI_Path_Full'])
            data_4d = img_4d.get_fdata()
            
            # Extract the specific 3D volume
            data_3d = data_4d[:, :, :, vol_idx]
            
            # Create a new 3D NIfTI image
            img_3d = nib.Nifti1Image(data_3d, img_4d.affine, img_4d.header)
            
            # Save the 3D volume
            nib.save(img_3d, output_path)
        else:
            # Regular 3D volume, just copy it
            logger.log(LogLevel.INFO.value, f"Copying {row['NIfTI_Path_Full']} to {output_path}")
            shutil.copy(row["NIfTI_Path_Full"], output_path)
            
        # Create JSON sidecar file
        output_json = os.path.join(anat_dir, new_name.replace(original_ext, ".json"))
        json_data = {}
        
        # If original JSON exists, use it as base
        if 'JSON_Path' in row and pd.notnull(row['JSON_Path']):
            with open(row['JSON_Path'], 'r') as f:
                json_data = json.load(f)
        
        # Add metadata from the table
        if 'MagneticFieldStrength' in row and pd.notnull(row['MagneticFieldStrength']):
            try:
                json_data['MagneticFieldStrength'] = float(row['MagneticFieldStrength'])
            except (ValueError, TypeError):
                pass
                
        if 'EchoTime' in row and pd.notnull(row['EchoTime']):
            try:
                json_data['EchoTime'] = float(row['EchoTime'])
            except (ValueError, TypeError):
                pass
        
        # Don't add VolumeIndex - it's not a valid BIDS field
        
        # Only create JSON file if there's actually metadata to write
        if json_data:
            with open(output_json, 'w') as f:
                json.dump(json_data, f, indent=2)

def main():
    parser = argparse.ArgumentParser(
        description="QSMxT niftiConvert with extended curses UI, multiline regex row, toggling path view, and field copying.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        'input_dir',
        help='Input NIfTI directory.'
    )

    parser.add_argument(
        'output_dir',
        help='Output BIDS directory.'
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
        f.write(f"QSMxT: {get_qsmxt_version()}")
        f.write(f"\nRun command: {str.join(' ', sys.argv)}")
        f.write(f"\nPython interpreter: {sys.executable}")
        f.write("\n\n == References ==\n")
        f.write("\n - Stewart AW, Bollmann S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT")
        f.write("\n - Gorgolewski KJ, Auer T, Calhoun VD, et al. The brain imaging data structure, a format for organizing outputs of neuroimaging experiments. Sci Data. 2016;3(1):160044.\n")

    nifti_convert(args.input_dir, args.output_dir)
    script_exit()

if __name__ == "__main__":
    main()
