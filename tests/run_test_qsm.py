#!/usr/bin/env python3
import osfclient
import pytest
import os

import run_2_qsm as qsm
from scripts.sys_cmd import sys_cmd

@pytest.fixture
def bids_dir():
    if not os.path.exists('bids-osf'):
        if not os.path.exists('bids-osf.tar'):
            print("Downloading test data...")
            file_pointer = next(osfclient.OSF().project("9jc42").storage().files)
            file_handle = open('bids-osf.tar', 'wb')
            file_pointer.write_to(file_handle)
        print("Extracting test data...")
        sys_cmd("tar xf bids-osf.tar")
        sys_cmd("rm bids-osf.tar")
    return 'bids-osf'

def workflow(args, init_workflow, run_workflow, run_args):
    print("=== PREPARING WORKFLOW ===")
    if init_workflow and not run_workflow:
        wf = qsm.init_workflow(args)
    if init_workflow and run_workflow:
        qsm.set_env_variables()
        if run_args:
            args_dict = vars(args)
            for key, value in run_args.items():
                args_dict[key] = value
        wf = qsm.init_workflow(args)
        wf.run(plugin='MultiProc', plugin_args={'n_procs': args.n_procs})

@pytest.mark.parametrize("init_workflow, run_workflow, run_args", [
    (True, True, { 'tgvqsm_iterations' : 1, 'num_echoes' : 2, 'single_pass' : True })
])
def test_args_defaults(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        "qsm"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.abspath("qsm"))
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
        "qsm",
        "--qsm_algorithm", "tgv_qsm"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.abspath("qsm"))
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
    (True, True, { 'num_echoes' : 2 })
])
def test_args_nextqsm_defaults(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        "qsm",
        "--qsm_algorithm", "nextqsm"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.abspath("qsm"))
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
    (True, True, { 'num_echoes' : 2 })
])
def test_args_nextqsm_laplacian(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        "qsm",
        "--qsm_algorithm", "nextqsm",
        "--nextqsm_unwrapping_algorithm", "laplacian"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.abspath("qsm"))
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
        "qsm",
        "--single_pass"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.abspath("qsm"))
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
    (True, True, { 'tgvqsm_iterations' : 1, 'num_echoes' : 2, 'single_pass' : True })
])
def test_args_inhomogeneity_correction_bet(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        "qsm",
        "--inhomogeneity_correction",
        "--masking", "bet"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.abspath("qsm"))
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
    (True, True, { 'tgvqsm_iterations' : 1, 'num_echoes' : 2, 'single_pass' : True })
])
def test_args_inhomogeneity_correction_magnitudebased(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        "qsm",
        "--inhomogeneity_correction",
        "--masking", "magnitude-based"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.abspath("qsm"))
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
        "qsm",
        "--inhomogeneity_correction",
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.abspath("qsm"))
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
    (True, True, { 'tgvqsm_iterations' : 1, 'num_echoes' : 2, 'single_pass' : True })
])
def test_args_addbet(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        "qsm",
        "--add_bet"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.abspath("qsm"))
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
        "qsm",
        "--add_bet",
        "--masking", "bet"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.abspath("qsm"))
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
    (True, True, { 'tgvqsm_iterations' : 1, 'num_echoes' : 2, 'single_pass' : True })
])
def test_args_use_existing_masks(bids_dir, init_workflow, run_workflow, run_args):
    args = qsm.process_args(qsm.parse_args([
        bids_dir,
        "qsm",
        "--use_existing_masks"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.abspath("qsm"))
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
        "qsm",
        "--num_echoes", "3"
    ]))
    
    assert(args.bids_dir == os.path.abspath(bids_dir))
    assert(args.output_dir == os.path.abspath("qsm"))
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

