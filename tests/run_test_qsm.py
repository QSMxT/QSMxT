#!/usr/bin/env python3
import os
import osfclient
import pytest
import tempfile
import glob
import nibabel as nib
import shutil
import run_2_qsm as qsm
from scripts.sys_cmd import sys_cmd
from run_5_analysis import load_labels, update_labels, get_stats_ground_truth
from scripts.qsmxt_functions import get_qsmxt_dir

run_workflow = True

@pytest.fixture
def bids_dir():
    tmp_dir = tempfile.gettempdir()
    if not os.path.exists(os.path.join(tmp_dir, 'bids-osf')):
        if not os.path.exists(os.path.join(tmp_dir, 'bids-osf.tar')):
            print("Downloading test data...")
            file_pointer = next(osfclient.OSF().project("9jc42").storage().files)
            file_handle = open(os.path.join(tmp_dir, 'bids-osf.tar'), 'wb')
            file_pointer.write_to(file_handle)
        print("Extracting test data...")
        sys_cmd(f"tar xf {os.path.join(tmp_dir, 'bids-osf.tar')} -C {tmp_dir}")
        sys_cmd(f"rm {os.path.join(tmp_dir, 'bids-osf.tar')}")
    return os.path.join(tmp_dir, 'bids-osf')

def print_metrics(bids_path, qsm_path):
    qsm_file = glob.glob(os.path.join(qsm_path, "qsm_final", "*qsm*nii*"))[0]
    seg_file = glob.glob(os.path.join(bids_path, "sub-1", "ses-1", "extra_data", "*segmentation*nii*"))[0]
    chi_file = glob.glob(os.path.join(bids_path, "sub-1", "ses-1", "extra_data", "*chi*crop*nii*"))[0]

    qsm = nib.load(qsm_file).get_fdata()
    seg = nib.load(seg_file).get_fdata()
    chi = nib.load(chi_file).get_fdata()

    labels = {}
    update_labels(labels, seg)
    label_stats = get_stats_ground_truth(labels, seg, qsm, chi)
    
    for label_name in label_stats.keys():
        if label_stats[label_name]:
            voxels, min_v, max_v, median, mean, std, mean_abs_diff, rms_diff = label_stats[label_name]
            print(f"{label_name}: voxels={voxels}; min={round(min_v, 4)}; max={round(max_v, 4)}; median={round(median, 4)}; mean={round(mean, 4)}; std={round(std, 4)}; mean_abs_diff={round(mean_abs_diff, 2)}; rms_diff={round(rms_diff, 4)}")


def workflow(args, init_workflow, run_workflow, run_args, show_metrics=False):
    assert(not (run_workflow == True and init_workflow == False))
    if init_workflow:
        wf = qsm.init_workflow(args)
    if init_workflow and run_workflow:
        qsm.set_env_variables()
        if run_args:
            args_dict = vars(args)
            for key, value in run_args.items():
                args_dict[key] = value
            wf = qsm.init_workflow(args)
        shutil.rmtree(os.path.join(args.output_dir, "qsm_final"))
        wf.run(plugin='MultiProc', plugin_args={'n_procs': args.n_procs})
        if show_metrics:
            print_metrics(args.bids_dir, args.output_dir)

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflow, { 'tgvqsm_iterations' : 1, 'num_echoes' : 2, 'single_pass' : True })
])
def test_args_defaults(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        os.path.join(tempfile.gettempdir(), "qsm")
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.join(tempfile.gettempdir(), "qsm"))
    assert(args.qsm_algorithm == "tgv_qsm")
    assert(args.masking == "phase-based")
    assert(args.two_pass == True)
    assert(args.single_pass == False)
    assert(args.inhomogeneity_correction == False)
    assert(args.add_bet == False)
    assert(args.use_existing_masks == False)
    assert(0 < args.n_procs <= int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    assert(0 < args.tgvqsm_threads < int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    
    workflow(args, init_workflow, run_workflow, run_args)
            
@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, False, None)
])
def test_args_tgvqsm_defaults(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--qsm_algorithm", "tgv_qsm"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.join(tempfile.gettempdir(), "qsm"))
    assert(args.qsm_algorithm == "tgv_qsm")
    assert(args.masking == "phase-based")
    assert(args.two_pass == True)
    assert(args.single_pass == False)
    assert(args.inhomogeneity_correction == False)
    assert(args.add_bet == False)
    assert(args.use_existing_masks == False)
    assert(0 < args.n_procs <= int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    assert(0 < args.tgvqsm_threads < int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    
    workflow(args, init_workflow, run_workflow, run_args)

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflow, { 'num_echoes' : 2 })
])
def test_args_nextqsm_defaults(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--qsm_algorithm", "nextqsm"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.join(tempfile.gettempdir(), "qsm"))
    assert(args.qsm_algorithm == "nextqsm")
    assert(args.masking == "bet-firstecho")
    assert(args.two_pass == False)
    assert(args.single_pass == True)
    assert(args.inhomogeneity_correction == False)
    assert(args.add_bet == False)
    assert(args.use_existing_masks == False)
    assert(args.nextqsm_unwrapping_algorithm == "romeo")
    assert(0 < args.n_procs <= int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    
    workflow(args, init_workflow, run_workflow, run_args)
    
@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflow, { 'num_echoes' : 2, 'n_procs' : 1 })
])
def test_args_nextqsm_laplacian(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--qsm_algorithm", "nextqsm",
        "--nextqsm_unwrapping_algorithm", "laplacian"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.join(tempfile.gettempdir(), "qsm"))
    assert(args.qsm_algorithm == "nextqsm")
    assert(args.masking == "bet-firstecho")
    assert(args.two_pass == False)
    assert(args.single_pass == True)
    assert(args.inhomogeneity_correction == False)
    assert(args.add_bet == False)
    assert(args.use_existing_masks == False)
    assert(args.nextqsm_unwrapping_algorithm == "laplacian")
    assert(0 < args.n_procs <= int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    
    workflow(args, init_workflow, run_workflow, run_args)

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, False, None)
])
def test_args_singlepass(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--single_pass"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.join(tempfile.gettempdir(), "qsm"))
    assert(args.qsm_algorithm == "tgv_qsm")
    assert(args.masking == "phase-based")
    assert(args.two_pass == False)
    assert(args.single_pass == True)
    assert(args.inhomogeneity_correction == False)
    assert(args.add_bet == False)
    assert(args.use_existing_masks == False)
    assert(0 < args.n_procs <= int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    assert(0 < args.tgvqsm_threads < int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    
    workflow(args, init_workflow, run_workflow, run_args)

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflow, { 'tgvqsm_iterations' : 1, 'num_echoes' : 2, 'single_pass' : True })
])
def test_args_inhomogeneity_correction_bet(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--inhomogeneity_correction",
        "--masking", "bet"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.join(tempfile.gettempdir(), "qsm"))
    assert(args.qsm_algorithm == "tgv_qsm")
    assert(args.masking == "bet")
    assert(args.two_pass == False)
    assert(args.single_pass == True)
    assert(args.inhomogeneity_correction == True)
    assert(args.add_bet == False)
    assert(args.use_existing_masks == False)
    assert(0 < args.n_procs <= int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    assert(0 < args.tgvqsm_threads < int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    
    workflow(args, init_workflow, run_workflow, run_args)

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflow, { 'tgvqsm_iterations' : 1, 'num_echoes' : 2, 'single_pass' : True })
])
def test_args_inhomogeneity_correction_magnitudebased(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--inhomogeneity_correction",
        "--masking", "magnitude-based"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.join(tempfile.gettempdir(), "qsm"))
    assert(args.qsm_algorithm == "tgv_qsm")
    assert(args.masking == "magnitude-based")
    assert(args.two_pass == True)
    assert(args.single_pass == False)
    assert(args.inhomogeneity_correction == True)
    assert(args.add_bet == False)
    assert(args.use_existing_masks == False)
    assert(0 < args.n_procs <= int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    assert(0 < args.tgvqsm_threads < int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    
    workflow(args, init_workflow, run_workflow, run_args)

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, False, None)
])
def test_args_inhomogeneity_correction_invalid(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--inhomogeneity_correction",
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.join(tempfile.gettempdir(), "qsm"))
    assert(args.qsm_algorithm == "tgv_qsm")
    assert(args.masking == "phase-based")
    assert(args.two_pass == True)
    assert(args.single_pass == False)
    assert(args.inhomogeneity_correction == False)
    assert(args.add_bet == False)
    assert(args.use_existing_masks == False)
    assert(0 < args.n_procs <= int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    assert(0 < args.tgvqsm_threads < int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    
    workflow(args, init_workflow, run_workflow, run_args)

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflow, { 'tgvqsm_iterations' : 1, 'num_echoes' : 2, 'single_pass' : True })
])
def test_args_addbet(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--add_bet"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.join(tempfile.gettempdir(), "qsm"))
    assert(args.qsm_algorithm == "tgv_qsm")
    assert(args.masking == "phase-based")
    assert(args.two_pass == True)
    assert(args.single_pass == False)
    assert(args.inhomogeneity_correction == False)
    assert(args.add_bet == True)
    assert(args.use_existing_masks == False)
    assert(0 < args.n_procs <= int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    assert(0 < args.tgvqsm_threads < int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    
    workflow(args, init_workflow, run_workflow, run_args)

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, False, None)
])
def test_args_addbet_invalid(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--add_bet",
        "--masking", "bet"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.join(tempfile.gettempdir(), "qsm"))
    assert(args.qsm_algorithm == "tgv_qsm")
    assert(args.masking == "bet")
    assert(args.two_pass == False)
    assert(args.single_pass == True)
    assert(args.inhomogeneity_correction == False)
    assert(args.add_bet == False)
    assert(args.use_existing_masks == False)
    assert(0 < args.n_procs <= int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    assert(0 < args.tgvqsm_threads < int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    
    workflow(args, init_workflow, run_workflow, run_args)

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflow, { 'tgvqsm_iterations' : 1, 'num_echoes' : 2, 'single_pass' : True })
])
def test_args_use_existing_masks(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--use_existing_masks"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.join(tempfile.gettempdir(), "qsm"))
    assert(args.qsm_algorithm == "tgv_qsm")
    assert(args.masking == "phase-based")
    assert(args.two_pass == True)
    assert(args.single_pass == False)
    assert(args.inhomogeneity_correction == False)
    assert(args.add_bet == False)
    assert(args.use_existing_masks == True)
    assert(0 < args.n_procs <= int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    assert(0 < args.tgvqsm_threads < int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    
    workflow(args, init_workflow, run_workflow, run_args)

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, False, None)
])
def test_args_numechoes(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        os.path.join(tempfile.gettempdir(), "qsm"),
        "--num_echoes", "3"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.join(tempfile.gettempdir(), "qsm"))
    assert(args.qsm_algorithm == "tgv_qsm")
    assert(args.masking == "phase-based")
    assert(args.two_pass == True)
    assert(args.single_pass == False)
    assert(args.inhomogeneity_correction == False)
    assert(args.add_bet == False)
    assert(args.use_existing_masks == False)
    assert(args.num_echoes == 3)
    assert(0 < args.n_procs <= int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    assert(0 < args.tgvqsm_threads < int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    
    workflow(args, init_workflow, run_workflow, run_args)


@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, run_workflow, None)
])
def test_metrics(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        os.path.join(tempfile.gettempdir(), "qsm")
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.join(tempfile.gettempdir(), "qsm"))
    assert(args.qsm_algorithm == "tgv_qsm")
    assert(args.masking == "phase-based")
    assert(args.two_pass == True)
    assert(args.single_pass == False)
    assert(args.inhomogeneity_correction == False)
    assert(args.add_bet == False)
    assert(args.use_existing_masks == False)
    assert(0 < args.n_procs <= int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    assert(0 < args.tgvqsm_threads < int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()))
    
    workflow(args, init_workflow, run_workflow, run_args, show_metrics=True)

# TODO
#  - check file outputs
#  - test axial resampling / obliquity
#  - test for errors that may occur within a run, including:
#    - no phase files present
#    - number of json files different from number of phase files
#    - no magnitude files present - default to phase-based masking
#    - use_existing_masks specified but none found - default to masking method
#    - use_existing_masks specified but number of masks > 1 and mismatches # of echoes 
#    - use_existing_masks specified and masks found:
#      - inhomogeneity_correction, two_pass, and add_bet should all disable

