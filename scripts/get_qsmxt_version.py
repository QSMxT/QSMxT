import os
from scripts.sys_cmd import sys_cmd

def get_qsmxt_version():
    this_dir = os.path.dirname(os.path.abspath(__file__))
    version = sys_cmd(f"git --git-dir {os.path.join(this_dir, '..', '.git')} describe --tags", False, False)
    date = sys_cmd(f"git log -1 --format=%cd", False, False)
    return f"{version} (commit date: {date})"

