import os
import shutil
import json
from scripts.sys_cmd import sys_cmd

def get_qsmxt_dir():
    return os.path.split(os.path.os.path.dirname(os.path.abspath(__file__)))[0]

def get_qsmxt_version():
    this_dir = os.path.dirname(os.path.abspath(__file__))
    git_dir = os.path.join(this_dir, '..', '.git')
    version = sys_cmd(f"git --git-dir {git_dir} describe --tags", False, False)
    date = sys_cmd(f"git --git-dir {git_dir} log -1 --format=%cd", False, False)
    return f"{version} (commit date: {date})"

def get_qsm_premades(pipeline_file):
    premades = json.load(open(f"{os.path.join(get_qsmxt_dir(), 'qsm_pipelines.json')}", "r"))

    if pipeline_file:    
        user_premades = json.load(open(pipeline_file, "r"))
        premades.update(user_premades)

    return premades

def extend_fname(original_path, append_text, ext=None, out_dir=None):
    out_dir = out_dir or os.path.split(original_path)[0]
    original_fname = os.path.split(original_path)[1].split('.')[0]
    original_ext = ".".join(os.path.split(original_path)[1].split('.')[1:])
    return os.path.join(out_dir, f"{original_fname}{append_text}.{ext or original_ext}")

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
    if os.environ.get('SINGULARITY_NAME') and 'qsmxt' in os.environ.get('SINGULARITY_NAME'):
        return f"{os.environ.get('SINGULARITY_NAME')} (singularity)"
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
    
def get_diff():
    this_dir = os.path.dirname(os.path.abspath(__file__))
    qsmxt_dir = get_qsmxt_dir()
    diff = sys_cmd(f"git --git-dir {os.path.join(get_qsmxt_dir(), '.git')} --work-tree {get_qsmxt_dir()} diff", False, False)
    return f"{diff}\n" if diff else ""

def gen_plugin_args(pbs_account="", slurm_account="", plugin_args={}, time="00:30:00", num_cpus=1, mem_gb=5, name="QSMxT", slurm_partition=None):
    plugin_args['sbatch_args'] = f"--account={slurm_account} {f'--partition {slurm_partition}' if slurm_partition else ''} --job-name={name} --time={time} --ntasks=1 --cpus-per-task={num_cpus} --mem={mem_gb}gb"
    plugin_args['qsub_args'] = f'-A {pbs_account} -N {name} -l walltime={time} -l select=1:ncpus={num_cpus}:mem={mem_gb}gb'
    return plugin_args
    
