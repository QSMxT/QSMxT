import os
from scripts.sys_cmd import sys_cmd

def get_qsmxt_dir():
    return os.path.split(os.path.os.path.dirname(os.path.abspath(__file__)))[0]

def get_qsmxt_version():
    this_dir = os.path.dirname(os.path.abspath(__file__))
    git_dir = os.path.join(this_dir, '..', '.git')
    version = sys_cmd(f"git --git-dir {git_dir} describe --tags", False, False)
    date = sys_cmd(f"git --git-dir {git_dir} log -1 --format=%cd", False, False)
    return f"{version} (commit date: {date})"

    