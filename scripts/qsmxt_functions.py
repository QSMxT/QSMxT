import os
import shutil
from scripts.sys_cmd import sys_cmd

def get_qsmxt_dir():
    return os.path.split(os.path.os.path.dirname(os.path.abspath(__file__)))[0]

def get_qsmxt_version():
    this_dir = os.path.dirname(os.path.abspath(__file__))
    git_dir = os.path.join(this_dir, '..', '.git')
    version = sys_cmd(f"git --git-dir {git_dir} describe --tags", False, False)
    date = sys_cmd(f"git --git-dir {git_dir} log -1 --format=%cd", False, False)
    return f"{version} (commit date: {date})"

def get_container_version(check_path=True):
    if os.environ.get('SINGULARITY_NAME'):
        return f"{os.environ.get('SINGULARITY_NAME')} (singularity)"
    if os.path.exists("/README.md"):
        with open("/README.md", 'r') as readme_handle:
            lines = readme_handle.readlines()
        for line in lines:
            if '## qsmxt/' in line:
                version = line.split("/")[1].split(" ")[0].strip()
                return f"{version}"
    if check_path:
        if shutil.which("qsmxt_version.py"):
            return sys_cmd("qsmxt_version.py --container_only", print_output=False, print_command=False)
    else:
        return "unknown"
    
    