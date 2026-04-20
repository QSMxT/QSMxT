import os
import shutil
import json
from importlib.metadata import version as get_package_version
import subprocess
import psutil
import math
from qsmxt.scripts.logger import LogLevel, make_logger
from qsmxt.scripts.sys_cmd import sys_cmd
from nipype.pipeline.engine import Node, MapNode

def create_node(interface, name, n_procs=1, mem_gb=2, iterfield=None, is_map=False, **kwargs):
    logger = make_logger('main')
    mem_gb = round(mem_gb, 3)
    mem_avail = round(psutil.virtual_memory().available / (1024 ** 3) * 0.90, 3)
    logger.log(LogLevel.DEBUG.value, f"Node {name} has requested {mem_gb} GB.")
    if mem_gb < 2:
        mem_gb = 2
    if mem_gb > mem_avail:
        logger.log(LogLevel.WARNING.value, f"Node {name} has requested {mem_gb} GB of memory, which is greater than the available memory {mem_avail} GB! Segmentation faults may occur.")
        mem_gb = mem_avail
    if is_map:
        return MapNode(interface=interface, name=name, iterfield=iterfield, n_procs=n_procs, mem_gb=mem_gb, **kwargs)
    else:
        return Node(interface=interface, name=name, n_procs=n_procs, mem_gb=mem_gb, **kwargs)

def get_qsmxt_dir():
    path = os.path.abspath(__file__)

    # Check if the path contains 'site-packages'; if so, it's installed as a package
    if 'site-packages' in path:
        return os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(path))), 'qsmxt')
    else:
        # Return the directory up one level from the current file if running from source
        return os.path.dirname(os.path.dirname(path))

def get_qsmxt_version():
    return get_package_version('qsmxt')

def get_qsm_premades(pipeline_file=None):
    with open(f"{os.path.join(get_qsmxt_dir(), 'qsm_pipelines.json')}", "r") as fh:
        premades = json.load(fh)

    if pipeline_file:
        with open(pipeline_file, "r") as fh:
            user_premades = json.load(fh)
        premades.update(user_premades)

    return premades

def extend_fname(original_path, append_text, ext=None, out_dir=None):
    out_dir = out_dir or os.path.split(original_path)[0]
    original_fname = os.path.split(original_path)[1].split('.')[0]
    original_ext = ".".join(os.path.split(original_path)[1].split('.')[1:])
    return os.path.join(out_dir, f"{original_fname}{append_text}.{ext or original_ext}")

def get_fname(path, include_path=True):
    path_noext = ".".join(path.split('.')[:-1])
    if include_path: return path_noext
    return os.path.split(path_noext)[-1]

def print_qsm_premades(pipeline_file):
    premades = get_qsm_premades(pipeline_file)
    print("=== Premade pipelines ===")
    for key, value in premades.items():
        print(f"{key}", end="")
        if "description" in value:
            print(f": {value['description']}")
        else:
            print()

def get_container_version(check_path=True):
    if os.environ.get('APPTAINER_NAME') and 'qsmxt' in os.environ.get('APPTAINER_NAME'):
        return f"{os.environ.get('APPTAINER_NAME')} (apptainer)"
    if os.path.exists("/README.md"):
        with open("/README.md", 'r') as readme_handle:
            lines = readme_handle.readlines()
        for line in lines:
            if '## qsmxt/' in line:
                version = line.split("/")[1].split(" ")[0].strip()
                return f"{version}"
    if check_path and shutil.which("qsmxt_version.py"):
        return sys_cmd("qsmxt_version.py --container_only", print_output=False, print_command=False)
    return "unknown"

def is_git_repo(directory):
    cmd = f"git --git-dir {os.path.join(directory, '.git')} rev-parse"
    try:
        sys_cmd(cmd, print_output=False, print_command=False, raise_exception=True)
        return True
    except subprocess.CalledProcessError:
        return False

def get_diff():
    qsmxt_dir = get_qsmxt_dir()
    if is_git_repo(qsmxt_dir):
        diff = sys_cmd(f"git --git-dir {os.path.join(qsmxt_dir, '.git')} --work-tree {qsmxt_dir} diff", print_command=False, print_output=False)
        return f"{diff}\n" if diff else ""
    else:
        return ""

def gen_plugin_args(pbs_account="", slurm_account="", plugin_args={}, time="00:30:00", num_cpus=1, mem_gb=5, name="QSMxT", slurm_partition=None):
    mem_gb = math.ceil(mem_gb)
    plugin_args['sbatch_args'] = f"--account={slurm_account} {f'--partition {slurm_partition}' if slurm_partition else ''} --job-name={name} --time={time} --ntasks=1 --cpus-per-task={num_cpus} --mem={mem_gb}gb"
    plugin_args['qsub_args'] = f'-A {pbs_account} -N {name} -l walltime={time} -l select=1:ncpus={num_cpus}:mem={mem_gb}gb'
    return plugin_args
    
