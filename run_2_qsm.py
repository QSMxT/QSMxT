#!/usr/bin/env python3

import sys
import os.path
import os
import glob
import psutil
import datetime

from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import DataSink
from nipype.pipeline.engine import Workflow, Node, MapNode
from scripts.qsmxt_functions import get_qsmxt_version
from scripts.logger import LogLevel, make_logger, show_warning_summary

from interfaces import nipype_interface_scalephase as scalephase
from interfaces import nipype_interface_tgv_qsm as tgv
from interfaces import nipype_interface_makehomogeneous as makehomogeneous
from interfaces import nipype_interface_nonzeroaverage as nonzeroaverage
from interfaces import nipype_interface_twopass as twopass
from interfaces import nipype_interface_masking as masking
from interfaces import nipype_interface_erode as erode
from interfaces import nipype_interface_bet2 as bet2
from interfaces import nipype_interface_phaseweights as phaseweights
from interfaces import nipype_interface_json as json
from interfaces import nipype_interface_addtojson as addtojson
from interfaces import nipype_interface_axialsampling as sampling

from workflows.unwrapping import unwrapping_workflow
from workflows.nextqsm import nextqsm_B0_workflow, nextqsm_workflow

import argparse


def init_workflow():
    subjects = [
        os.path.split(path)[1]
        for path in glob.glob(os.path.join(args.bids_dir, args.subject_pattern))
        if not args.subjects or os.path.split(path)[1] in args.subjects
    ]
    if not subjects:
        logger.log(LogLevel.ERROR.value, f"No subjects found in {os.path.join(args.bids_dir, args.session_pattern)}")
        exit(1)
    wf = Workflow("workflow_qsm", base_dir=args.work_dir)
    wf.add_nodes([
        node for node in
        [init_subject_workflow(subject) for subject in subjects]
        if node
    ])
    return wf

def init_subject_workflow(
    subject
):
    sessions = [
        os.path.split(path)[1]
        for path in glob.glob(os.path.join(args.bids_dir, subject, args.session_pattern))
        if not args.sessions or os.path.split(path)[1] in args.sessions
    ]
    if not sessions:
        logger.log(LogLevel.ERROR.value, f"No sessions found in: {os.path.join(args.bids_dir, subject, args.session_pattern)}")
        exit(1)
    wf = Workflow(subject, base_dir=os.path.join(args.work_dir, "workflow_qsm"))
    wf.add_nodes([
        node for node in
        [init_session_workflow(subject, session) for session in sessions]
        if node
    ])
    return wf

def init_session_workflow(subject, session):
    # exit if no runs found
    phase_pattern = os.path.join(args.bids_dir, args.phase_pattern.replace("{run}", "").format(subject=subject, session=session))
    phase_files = glob.glob(phase_pattern)
    if not phase_files:
        logger.log(LogLevel.WARNING.value, f"No phase files found matching pattern: {phase_pattern}. Skipping {subject}/{session}")
        return
    for phase_file in phase_files:
        if 'run-' not in phase_file:
            logger.log(LogLevel.WARNING.value, f"No 'run-' identifier found in file: {phase_file}. Skipping {subject}/{session}")
            return

    # identify all runs
    runs = sorted(list(set([
        f"run-{os.path.split(path)[1][os.path.split(path)[1].find('run-') + 4: os.path.split(path)[1].find('_', os.path.split(path)[1].find('run-') + 4)]}"
        for path in phase_files
    ])))
    
    wf = Workflow(session, base_dir=os.path.join(args.work_dir, "workflow_qsm", subject, session))
    wf.add_nodes([
        node for node in
        [init_run_workflow(subject, session, run) for run in runs]
        if node
    ])
    return wf

def init_run_workflow(subject, session, run):
    logger.log(LogLevel.INFO.value, f"Creating nipype workflow for {subject}/{session}/{run}...")

    # create copies of command-line arguments that may need to change for this run (if problems occur)
    masking_method = args.masking
    add_bet = args.add_bet
    inhomogeneity_correction = args.inhomogeneity_correction

    # get relevant files from this run
    phase_pattern = os.path.join(args.bids_dir, args.phase_pattern.format(subject=subject, session=session, run=run))
    phase_files = sorted(glob.glob(phase_pattern))[:args.num_echoes_to_process]
    
    magnitude_pattern = os.path.join(args.bids_dir, args.magnitude_pattern.format(subject=subject, session=session, run=run))
    magnitude_files = sorted(glob.glob(magnitude_pattern))[:args.num_echoes_to_process]

    params_pattern = os.path.join(args.bids_dir, args.phase_pattern.format(subject=subject, session=session, run=run).replace("nii.gz", "nii").replace("nii", "json"))
    params_files = sorted(glob.glob(params_pattern))[:args.num_echoes_to_process]

    # handle any errors related to files
    if not phase_files:
        logger.log(LogLevel.WARNING.value, f"Skipping run {subject}/{session}/{run} - no phase files found matching pattern {phase_pattern}.")
        return
    if len(phase_files) != len(params_files):
        logger.log(LogLevel.WARNING.value, f"Skipping run {subject}/{session}/{run} - an unequal number of JSON and phase files are present.")
        return
    if (not magnitude_files and any([masking_method == 'magnitude-based', 'bet' in masking_method, add_bet, inhomogeneity_correction])):
        logger.log(LogLevel.WARNING.value, f"Run {subject}/{session}/{run} will use phase-based masking - no magnitude files found matching pattern: {magnitude_pattern}.")
        masking_method = 'phase-based'
        add_bet = False
        inhomogeneity_correction = False

    # create nipype workflow for this run
    wf = Workflow(run, base_dir=os.path.join(args.work_dir, "workflow_qsm", subject, session, run))

    # datasink
    n_datasink = Node(
        interface=DataSink(base_directory=args.output_dir),
        name='nipype_datasink'
    )

    # create json header for this run
    n_json = Node(
        interface=json.JsonInterface(
            in_dict={
                "QSMxT version" : get_qsmxt_version(),
                "Run command" : str.join(" ", sys.argv),
                "Python interpreter" : sys.executable
            },
            out_file=f"{subject}_{session}_{run}_qsmxt-header.json"
        ),
        name="json_createheader"
        # inputs : 'in_dict'
        # outputs: 'out_file'
    )

    # get files
    n_getfiles = Node(
        IdentityInterface(
            fields=['phase_files', 'magnitude_files', 'params_files']
        ),
        name='nipype_getfiles'
    )
    n_getfiles.inputs.phase_files = phase_files
    n_getfiles.inputs.magnitude_files = magnitude_files
    n_getfiles.inputs.params_files = params_files

    # read echotime and field strengths from json files
    def read_json(in_file):
        import json
        json_file = open(in_file, 'rt')
        data = json.load(json_file)
        te = data['EchoTime']
        b0 = data['MagneticFieldStrength']
        json_file.close()
        return te, b0
    mn_params = MapNode(
        interface=Function(
            input_names=['in_file'],
            output_names=['EchoTime', 'MagneticFieldStrength'],
            function=read_json
        ),
        iterfield=['in_file'],
        name='func_read-json'
    )
    wf.connect([
        (n_getfiles, mn_params, [('params_files', 'in_file')])
    ])

    # scale phase data
    mn_phase_scaled = MapNode(
        interface=scalephase.ScalePhaseInterface(),
        iterfield=['in_file'],
        name='nibabel_numpy_scale-phase'
        # outputs : 'out_file'
    )
    wf.connect([
        (n_getfiles, mn_phase_scaled, [('phase_files', 'in_file')])
    ])

    # resample to axial
    mn_resample_inputs = MapNode(
        interface=sampling.AxialSamplingInterface(
            obliquity_threshold=10
        ),
        iterfield=['in_mag', 'in_pha'],
        name='nibabel_numpy_nilearn_axial-resampling'
    )
    wf.connect([
        (n_getfiles, mn_resample_inputs, [('magnitude_files', 'in_mag')]),
        (mn_phase_scaled, mn_resample_inputs, [('out_file', 'in_pha')])
    ])

    # run homogeneity filter if necessary
    if inhomogeneity_correction:
        mn_inhomogeneity_correction = MapNode(
            interface=makehomogeneous.MakeHomogeneousInterface(),
            iterfield=['in_file'],
            name='mriresearchtools_correct-inhomogeneity'
            # output : out_file
        )
        wf.connect([
            (mn_resample_inputs, mn_inhomogeneity_correction, [('out_mag', 'in_file')])
        ])

    def repeat(magnitude_files, phase_files):
        return magnitude_files, phase_files
    mn_inputs = MapNode(
        interface=Function(
            input_names=['magnitude_files', 'phase_files'],
            output_names=['magnitude_files', 'phase_files'],
            function=repeat
        ),
        iterfield=['magnitude_files', 'phase_files'],
        name='func_repeat-inputs'
    )
    wf.connect([
        (mn_resample_inputs, mn_inputs, [('out_pha', 'phase_files')])
    ])
    if inhomogeneity_correction:
        wf.connect([
            (mn_inhomogeneity_correction, mn_inputs, [('out_file', 'magnitude_files')])
        ])
    else:
        wf.connect([
            (mn_resample_inputs, mn_inputs, [('out_mag', 'magnitude_files')])
        ])

    # masking steps
    mn_mask = add_masking_nodes(wf, masking_method, add_bet, mn_inputs, n_json, n_datasink)

    # qsm steps
    if args.qsm_algorithm == 'tgvqsm':
        wf = add_tgvqsm_workflow(wf, mn_params, mn_inputs, mn_mask, n_datasink, magnitude_files[0])
    elif args.qsm_algorithm == 'nextqsm':
        wf = addNextqsmWorkflow(wf, mn_inputs, mn_params, mn_mask, n_datasink, args.unwrapping_algorithm)
    elif args.qsm_algorithm == 'nextqsm_combined':
        wf = addB0NextqsmB0Workflow(wf, mn_inputs, mn_params, mn_mask, n_datasink)

    return wf

def add_masking_nodes(wf, masking_method, add_bet, mn_inputs, n_json, n_datasink):

    # do phase weights if necessary
    if masking_method == 'phase-based':
        mn_phaseweights = MapNode(
            interface=phaseweights.RomeoMaskingInterface(),
            iterfield=['phase', 'mag'],
            name='romeo-voxelquality'
            # output: 'out_file'
        )
        mn_phaseweights.inputs.weight_type = "grad+second+mag"
        wf.connect([
            (mn_inputs, mn_phaseweights, [('phase_files', 'phase')]),
            (mn_inputs, mn_phaseweights, [('magnitude_files', 'mag')])
        ])

    # do threshold-based masking if necessary
    if masking_method in ['phase-based', 'magnitude-based']:
        n_threshold_masking = Node(
            interface=masking.MaskingInterface(),
            name='scipy_numpy_nibabel_threshold-masking'
            # inputs : ['in_files']
        )
        if args.threshold: n_threshold_masking.inputs.threshold = args.threshold

        n_add_threshold_to_json = Node(
            interface=addtojson.AddToJsonInterface(
                in_key = "threshold"
            ),
            name="json_add-threshold"
        )
        wf.connect([
            (n_json, n_add_threshold_to_json, [('out_file', 'in_file')]),
            (n_threshold_masking, n_add_threshold_to_json, [('threshold', 'in_num_value')])
        ])
        wf.connect([
            (n_add_threshold_to_json, n_datasink, [('out_file', 'headers')])
        ])

        if masking_method in ['phase-based']:    
            wf.connect([
                (mn_phaseweights, n_threshold_masking, [('out_file', 'in_files')])
            ])
        elif masking_method == 'magnitude-based':
            wf.connect([
                (mn_inputs, n_threshold_masking, [('magnitude_files', 'in_files')])
            ])

    # run bet if necessary
    if masking_method in ['bet', 'bet-firstecho'] or add_bet:
        def get_first(magnitude_files): return [magnitude_files[0] for f in magnitude_files]
        n_getfirst = Node(
            interface=Function(
                input_names=['magnitude_files'],
                output_names=['magnitude_file'],
                function=get_first
            ),
            name='func_get-first'
        )
        wf.connect([
            (mn_inputs, n_getfirst, [('magnitude_files', 'magnitude_files')])
        ])

        mn_bet = MapNode(
            interface=bet2.Bet2Interface(fractional_intensity=args.bet_fractional_intensity),
            iterfield=['in_file'],
            name='fsl-bet'
            # output: 'mask_file'
        )
        if masking_method == 'bet-firstecho':
            wf.connect([
                (n_getfirst, mn_bet, [('magnitude_file', 'in_file')])
            ])
        else:
            wf.connect([
                (mn_inputs, mn_bet, [('magnitude_files', 'in_file')])
            ])
        mn_bet_erode = MapNode(
            interface=erode.ErosionInterface(
                num_erosions=2
            ),
            iterfield=['in_file'],
            name='scipy_numpy_nibabel_erode'
        )
        wf.connect([
            (mn_bet, mn_bet_erode, [('mask_file', 'in_file')])
        ])

        # add bet if necessary
        if add_bet:
            mn_mask_plus_bet = MapNode(
                interface=twopass.TwopassNiftiInterface(),
                name='numpy_nibabel_mask-plus-bet',
                iterfield=['in_file1', 'in_file2'],
            )
            wf.connect([
                (n_threshold_masking, mn_mask_plus_bet, [('masks', 'in_file1')]),
                (mn_bet_erode, mn_mask_plus_bet, [('out_file', 'in_file2')])
            ])

    # link up nodes to get standardised outputs as 'masks' and 'masks_filled' in mn_mask
    def repeat(masks, masks_filled):
        return masks, masks_filled
    mn_mask = MapNode(
        interface=Function(
            input_names=['masks', 'masks_filled'],
            output_names=['masks', 'masks_filled'],
            function=repeat
        ),
        iterfield=['masks', 'masks_filled'],
        name='func_repeat-mask'
    )
    if masking_method in ['bet', 'bet-firstecho']:
        wf.connect([
            (mn_bet, mn_mask, [('mask_file', 'masks')]),
            (mn_bet, mn_mask, [('mask_file', 'masks_filled')]),
        ])
    if masking_method in ['magnitude-based', 'phase-based']:
        wf.connect([
            (n_threshold_masking, mn_mask, [('masks', 'masks')])
        ])
        if not add_bet:
            wf.connect([
                (n_threshold_masking, mn_mask, [('masks_filled', 'masks_filled')])
            ])
        else:
            wf.connect([
                (mn_mask_plus_bet, mn_mask, [('out_file', 'masks_filled')])
            ])

    return mn_mask

def add_tgvqsm_workflow(wf, mn_params, mn_inputs, mn_mask, n_datasink, magnitude_file):
    # === Single-pass QSM reconstruction (filled) ===
    mn_qsm_filled = MapNode(
        interface=tgv.QSMappingInterface(
            iterations=args.qsm_iterations,
            alpha=args.qsm_alphas,
            erosions=0 if args.two_pass else 5,
            num_threads=args.qsm_threads,
            out_suffix='_qsm-filled',
            extra_arguments='--ignore-orientation --no-resampling'
        ),
        iterfield=['phase_file', 'TE', 'b0', 'mask_file'],
        name='tgv-qsm_filled'
        # inputs: 'phase_file', 'TE', 'b0', 'mask_file'
        # output: 'out_file'
    )
    mn_qsm_filled.plugin_args = {
        'qsub_args': f'-A {args.qsub_account_string} -l walltime=03:00:00 -l select=1:ncpus={args.qsm_threads}:mem=20gb:vmem=20gb',
        'overwrite': True
    }
    wf.connect([
        (mn_params, mn_qsm_filled, [('EchoTime', 'TE')]),
        (mn_params, mn_qsm_filled, [('MagneticFieldStrength', 'b0')]),
        (mn_mask, mn_qsm_filled, [('masks_filled', 'mask_file')]),
        (mn_inputs, mn_qsm_filled, [('phase_files', 'phase_file')]),
    ])

    # qsm averaging
    n_qsm_filled_average = Node(
        interface=nonzeroaverage.NonzeroAverageInterface(),
        name='numpy_nibabel_qsm-filled-average'
        # input : in_files
        # output : out_file
    )
    wf.connect([
        (mn_qsm_filled, n_qsm_filled_average, [('out_file', 'in_files')])
    ])

    # resample qsm to original
    n_resample_qsm = Node(
        interface=sampling.ResampleLikeInterface(
            in_like=magnitude_file
        ),
        name='nibabel_numpy_nilearn_resample-qsm'
    )
    wf.connect([
        (n_qsm_filled_average, n_resample_qsm, [('out_file', 'in_file')]),
        (n_resample_qsm, n_datasink, [('out_file', 'qsm_singlepass' if args.two_pass else 'qsm_final')]),
    ])

    # === Two-pass QSM reconstruction (not filled) ===
    if args.two_pass:
        mn_qsm = MapNode(
            interface=tgv.QSMappingInterface(
                iterations=args.qsm_iterations,
                alpha=args.qsm_alphas,
                erosions=0,
                num_threads=args.qsm_threads,
                out_suffix='_qsm',
                extra_arguments='--ignore-orientation --no-resampling'
            ),
            iterfield=['phase_file', 'TE', 'b0', 'mask_file'],
            name='tgv-qsm_intermediate'
            # inputs: 'phase_file', 'TE', 'b0', 'mask_file'
            # output: 'out_file'
        )

        # args for PBS
        mn_qsm.plugin_args = {
            'qsub_args': f'-A {args.qsub_account_string} -l walltime=03:00:00 -l select=1:ncpus={args.qsm_threads}:mem=20gb:vmem=20gb',
            'overwrite': True
        }

        wf.connect([
            (mn_params, mn_qsm, [('EchoTime', 'TE')]),
            (mn_params, mn_qsm, [('MagneticFieldStrength', 'b0')]),
            (mn_mask, mn_qsm, [('masks', 'mask_file')]),
            (mn_inputs, mn_qsm, [('phase_files', 'phase_file')])
        ])

        # qsm averaging
        n_qsm_average = Node(
            interface=nonzeroaverage.NonzeroAverageInterface(),
            name='numpy_nibabel_qsm-average'
            # input : in_files
            # output : out_file
        )
        wf.connect([
            (mn_qsm, n_qsm_average, [('out_file', 'in_files')])
        ])

        # Two-pass combination step
        mn_qsm_twopass = MapNode(
            interface=twopass.TwopassNiftiInterface(),
            name='numpy_nibabel_twopass',
            iterfield=['in_file1', 'in_file2']
        )
        wf.connect([
            (mn_qsm, mn_qsm_twopass, [('out_file', 'in_file1')]),
            (mn_qsm_filled, mn_qsm_twopass, [('out_file', 'in_file2')]),
            #(mn_mask, mn_qsm_twopass, [('mask_file', 'in_maskFile')])
        ])

        n_qsm_twopass_average = Node(
            interface=nonzeroaverage.NonzeroAverageInterface(),
            name='numpy_nibabel_twopass-average'
            # input : in_files
            # output: out_file
        )
        wf.connect([
            (mn_qsm_twopass, n_qsm_twopass_average, [('out_file', 'in_files')])
        ])

        # resample qsm to original
        n_resample_qsm_twopass = Node(
            interface=sampling.ResampleLikeInterface(
                in_like=magnitude_file
            ),
            name='nibabel_numpy_nilearn_resample-qsm-twopass'
        )
        wf.connect([
            (n_qsm_twopass_average, n_resample_qsm_twopass, [('out_file', 'in_file')]),
            (n_resample_qsm_twopass, n_datasink, [('out_file', 'qsm_final')]),
        ])

    return wf

def addB0NextqsmB0Workflow(wf, mn_inputs, mn_params, mn_mask, n_datasink):
    # extract the fieldstrength of the first echo for input to nextqsm Node (not MapNode)
    def first(list=None):
        return list[0]
    n_fieldStrength = Node(Function(input_names="list",
                                    output_names=["fieldStrength"],
                                    function=first),
                            name='extract_fieldStrength')
    n_mask = Node(Function(input_names="list", # TODO try to use B0 phase-based mask
                                        output_names=["out_file"],
                                        function=first),
                                name='extract_Mask')
    wf_unwrapping = unwrapping_workflow("romeoB0")
    wf_nextqsmB0 = nextqsm_B0_workflow()
    
    wf.connect([
        (mn_inputs, wf_unwrapping, [('phase_files', 'inputnode.wrapped_phase')]),
        (mn_inputs, wf_unwrapping, [('magnitude_files', 'inputnode.mag')]),
        (mn_params, wf_unwrapping, [('EchoTime', 'inputnode.TE')]),
        
        (wf_unwrapping, wf_nextqsmB0, [('outputnode.B0', 'inputnode.B0'),]),
        
        (mn_mask, n_mask, [('masks_filled', 'list'),]),
        (n_mask, wf_nextqsmB0, [('out_file', 'inputnode.mask'),]),
        (mn_params, n_fieldStrength, [('MagneticFieldStrength', 'list')]),
        (n_fieldStrength, wf_nextqsmB0, [('fieldStrength', 'inputnode.fieldStrength'),]),
        
        (wf_nextqsmB0, n_datasink, [('outputnode.qsm', 'final_qsm')]),
    ])

    return wf
    
def addNextqsmWorkflow(wf, mn_inputs, mn_params, mn_mask, n_datasink, unwrapping_type):
    wf_unwrapping = unwrapping_workflow(unwrapping_type)
    wf_nextqsm = nextqsm_workflow()
    
    wf.connect([
        (mn_inputs, wf_unwrapping, [('phase_files', 'inputnode.wrapped_phase')]),
        (mn_inputs, wf_unwrapping, [('magnitude_files', 'inputnode.mag')]),
        (mn_params, wf_unwrapping, [('EchoTime', 'inputnode.TE')]),
        
        (wf_unwrapping, wf_nextqsm, [('outputnode.unwrapped_phase', 'inputnode.unwrapped_phase')]),
        (mn_mask, wf_nextqsm, [('masks_filled', 'inputnode.mask')]),
        (mn_params, wf_nextqsm, [('EchoTime', 'inputnode.TE'),
                                 ('MagneticFieldStrength', 'inputnode.fieldStrength')]),
        
        (wf_nextqsm, n_datasink, [('outputnode.qsm', 'qsm_echo'),
                                  ('outputnode.qsm_average', 'qsm_final')])
    ])

    return wf

if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="QSMxT qsm: QSM Reconstruction Pipeline",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        'bids_dir',
        help='Input data folder generated using run_1_dicomConvert.py; can also use a ' +
             'previously existing BIDS folder. Ensure that the --subject_pattern, '+
             '--session_pattern, --magnitude_pattern and --phase_pattern are correct.'
    )

    parser.add_argument(
        'output_dir',
        help='Output QSM folder; will be created if it does not exist.'
    )

    parser.add_argument(
        '--subject_pattern',
        default='sub*',
        help='Pattern used to match subject folders in bids_dir'
    )

    parser.add_argument(
        '--session_pattern',
        default='ses*',
        help='Pattern used to match session folders in subject folders'
    )

    parser.add_argument(
        '--magnitude_pattern',
        default='{subject}/{session}/anat/*{run}*mag*nii*',
        help='Pattern to match magnitude files within the BIDS directory. ' +
             'The {subject}, {session} and {run} placeholders must be present.'
    )

    parser.add_argument(
        '--phase_pattern',
        default='{subject}/{session}/anat/*{run}*phase*nii*',
        help='Pattern to match phase files for qsm within session folders. ' +
             'The {subject}, {session} and {run} placeholders must be present.'
    )

    parser.add_argument(
        '--subjects',
        default=None,
        nargs='*',
        help='List of subject folders to process; by default all subjects are processed.'
    )

    parser.add_argument(
        '--sessions',
        default=None,
        nargs='*',
        help='List of session folders to process; by default all sessions are processed.'
    )

    parser.add_argument(
        '--num_echoes',
        dest='num_echoes_to_process',
        default=None,
        type=int,
        help='The number of echoes to process; by default all echoes are processed.'
    )

    parser.add_argument(
        '--qsm_algorithm',
        default='tgvqsm',
        choices=['tgvqsm', 'nextqsm', 'nextqsm_combined']
    )

    parser.add_argument(
        '--unwrapping_algorithm',
        default='romeo',
        choices=['romeo','romeob0','laplacian'] # Laplacian is only for nextqsm
    )

    parser.add_argument(
        '--masking',
        default='phase-based',
        choices=['magnitude-based', 'phase-based', 'bet', 'bet-firstecho'],
        help='Masking strategy. Magnitude-based and phase-based masking generates a mask by ' +
             'thresholding a lower percentage of the histogram of the signal (adjust using the '+
             '--threshold parameter). For phase-based masking, the spatial phase coherence is '+
             'thresholded and the magnitude is not required. Using BET automatically disables '+
             'the two-pass inversion strategy for artefact mitigation.'
    )

    parser.add_argument(
        '--single_pass',
        action='store_true',
        help='Runs a single QSM inversion per echo, rather than the novel two-pass QSM inversion that '+
             'separates reliable and less reliable phase regions for artefact reduction. '+
             'Use this option to disable the novel inversion and approximately halve the runtime.'
    )

    parser.add_argument(
        '--qsm_iterations',
        type=int,
        default=1000,
        help='Number of iterations used for QSM reconstruction in tgv_qsm.'
    )

    parser.add_argument(
        '--qsm_alphas',
        type=float,
        default=[0.0015, 0.0005],
        nargs=2,
        help='Regularisation alphas for tgv_qsm.'
    )

    parser.add_argument(
        '--inhomogeneity_correction',
        action='store_true',
        help='Applies an inhomogeneity correction to the magnitude prior to masking'
    )

    parser.add_argument(
        '--threshold',
        type=float,
        default=None,
        help='Threshold percentage; anything less than the threshold will be excluded from the mask. ' +
             'By default, the threshold is automatically chosen based on a \'gaussian\' algorithm.'
    )

    parser.add_argument(
        '--bet_fractional_intensity',
        type=float,
        default=0.5,
        help='Fractional intensity for BET masking operations.'
    )

    parser.add_argument(
        '--add_bet',
        action='store_true',
        help='When using magnitude or phase-based masking, this option adds a BET mask to the filled, '+
             'threshold-based mask. This is useful if areas of the sample are missing due to a failure '+
             'of the hole-filling algorithm.'
    )

    parser.add_argument(
        '--pbs',
        default=None,
        dest='qsub_account_string',
        help='Run the pipeline via PBS and use the argument as the QSUB account string.'
    )

    parser.add_argument(
        '--n_procs',
        type=int,
        default=None,
        help='Number of processes to run concurrently for MultiProc. By default, we use the number of CPUs, ' +
             'provided there are 6 GBs of RAM available for each.'
    )

    parser.add_argument(
        '--debug',
        action='store_true',
        help='Enables some nipype settings for debugging.'
    )
    
    args = parser.parse_args()
    
    # ensure directories are complete and absolute
    args.bids_dir = os.path.abspath(args.bids_dir)
    args.output_dir = os.path.abspath(args.output_dir)
    args.work_dir = os.path.abspath(args.output_dir)

    # this script's directory
    this_dir = os.path.dirname(os.path.abspath(__file__))

    os.makedirs(args.work_dir, exist_ok=True)
    os.makedirs(args.output_dir, exist_ok=True)

    # setup logger
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

    # misc environment variables
    os.environ["FSLOUTPUTTYPE"] = "NIFTI"

    # path environment variable
    os.environ["PATH"] += os.pathsep + os.path.join(this_dir, "scripts")

    # add this_dir and cwd to pythonpath
    if "PYTHONPATH" in os.environ: os.environ["PYTHONPATH"] += os.pathsep + this_dir
    else:                          os.environ["PYTHONPATH"]  = this_dir

    # debug options
    if args.debug:
        from nipype import config
        config.enable_debug_mode()
        config.set('execution', 'stop_on_first_crash', 'true')
        config.set('execution', 'remove_unnecessary_outputs', 'false')
        config.set('execution', 'keep_inputs', 'true')
        config.set('logging', 'workflow_level', 'DEBUG')
        config.set('logging', 'interface_level', 'DEBUG')
        config.set('logging', 'utils_level', 'DEBUG')

    # add_bet option only works with non-bet masking methods
    args.add_bet = args.add_bet and 'bet' not in args.masking

    # two-pass option does not work with 'bet' masking
    args.two_pass = 'bet' not in args.masking and not args.single_pass
    args.single_pass = not args.two_pass

    # decide on inhomogeneity correction
    args.inhomogeneity_correction = args.inhomogeneity_correction and (args.add_bet or 'phase-based' not in args.masking)

    # set number of QSM threads
    n_cpus = int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count())
    
    # set number of concurrent processes to run depending on
    # available CPUs and RAM (max 1 per 6 GB of available RAM)
    if not args.n_procs:
        available_ram_gb = psutil.virtual_memory().available / 1e9
        args.n_procs = max(1, min(int(available_ram_gb / 6), n_cpus))
        if available_ram_gb < 6:
            logger.log(LogLevel.WARNING.value, f"Less than 6 GB of memory available ({available_ram_gb} GB). At least 6 GB is recommended. You may need to close background programs.")
        logger.log(LogLevel.INFO.value, f"Running with {args.n_procs} processors.")

    #qsm_threads should be set to adjusted n_procs (either computed earlier or given via cli)
    #args.qsm_threads = args.n_procs if not args.qsub_account_string else 1
    args.qsm_threads = args.n_procs if not args.qsub_account_string else 6

    # make sure tgv_qsm is compiled on the target system before we start the pipeline:
    # process = subprocess.run(['tgv_qsm'])

    # run workflow
    #wf.write_graph(graph2use='flat', format='png', simple_form=False)
    wf = init_workflow()

    # get all node names
    node_names = [node._name.lower() for node in wf._get_all_nodes()]

    def any_string_matches_any_node(strings):
        return any(string in node_name for string in strings for node_name in node_names)

    # write "details_and_citations.txt" with the command used to invoke the script and any necessary citations
    with open(os.path.join(args.output_dir, "details_and_citations.txt"), 'w', encoding='utf-8') as f:
        # output QSMxT version, run command, and python interpreter
        f.write(f"QSMxT: {get_qsmxt_version()}")
        f.write(f"\nRun command: {str.join(' ', sys.argv)}")
        f.write(f"\nPython interpreter: {sys.executable}")

        # qsmxt, nipype, numpy
        f.write("\n\n == References ==")
        f.write("\n\n - Stewart AW, Robinson SD, O'Brien K, et al. QSMxT: Robust masking and artifact reduction for quantitative susceptibility mapping. Magnetic Resonance in Medicine. 2022;87(3):1289-1300. doi:10.1002/mrm.29048")
        f.write("\n\n - Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT")
        f.write("\n\n - Gorgolewski K, Burns C, Madison C, et al. Nipype: A Flexible, Lightweight and Extensible Neuroimaging Data Processing Framework in Python. Frontiers in Neuroinformatics. 2011;5. Accessed April 20, 2022. doi:10.3389/fninf.2011.00013")
        
        if any_string_matches_any_node(['tgv']):
            f.write("\n\n - Langkammer C, Bredies K, Poser BA, et al. Fast quantitative susceptibility mapping using 3D EPI and total generalized variation. NeuroImage. 2015;111:622-630. doi:10.1016/j.neuroimage.2015.02.041")
        if any_string_matches_any_node(['threshold-masking']) and args.threshold is None:
            f.write("\n\n - Balan AGR, Traina AJM, Ribeiro MX, Marques PMA, Traina Jr. C. Smart histogram analysis applied to the skull-stripping problem in T1-weighted MRI. Computers in Biology and Medicine. 2012;42(5):509-522. doi:10.1016/j.compbiomed.2012.01.004")
        if any_string_matches_any_node(['bet']):
            f.write("\n\n - Smith SM. Fast robust automated brain extraction. Human Brain Mapping. 2002;17(3):143-155. doi:10.1002/hbm.10062")
            f.write("\n\n - Liangfu Chen. liangfu/bet2 - Standalone Brain Extraction Tool. GitHub; 2015. https://github.com/liangfu/bet2")
        if any_string_matches_any_node(['romeo']):
            f.write("\n\n - Dymerska B, Eckstein K, Bachrata B, et al. Phase unwrapping with a rapid opensource minimum spanning tree algorithm (ROMEO). Magnetic Resonance in Medicine. 2021;85(4):2294-2308. doi:10.1002/mrm.28563")
        if any_string_matches_any_node(['correct-inhomogeneity']):
            f.write("\n\n - Eckstein K, Trattnig S, Simon DR. A Simple homogeneity correction for neuroimaging at 7T. In: Proc. Intl. Soc. Mag. Reson. Med. International Society for Magnetic Resonance in Medicine; 2019. Abstract 2716. https://index.mirasmart.com/ISMRM2019/PDFfiles/2716.html")
        if any_string_matches_any_node(['mriresearchtools']):
            f.write("\n\n - Eckstein K. korbinian90/MriResearchTools.jl. GitHub; 2022. https://github.com/korbinian90/MriResearchTools.jl")
        if any_string_matches_any_node(['numpy']):
            f.write("\n\n - Harris CR, Millman KJ, van der Walt SJ, et al. Array programming with NumPy. Nature. 2020;585(7825):357-362. doi:10.1038/s41586-020-2649-2")
        if any_string_matches_any_node(['scipy']):
            f.write("\n\n - Virtanen P, Gommers R, Oliphant TE, et al. SciPy 1.0: fundamental algorithms for scientific computing in Python. Nat Methods. 2020;17(3):261-272. doi:10.1038/s41592-019-0686-2")
        if any_string_matches_any_node(['nibabel']):
            f.write("\n\n - Brett M, Markiewicz CJ, Hanke M, et al. nipy/nibabel. GitHub; 2019. https://github.com/nipy/nibabel")
        f.write("\n\n")

    if args.qsub_account_string:
        wf.run(
            plugin='PBSGraph',
            plugin_args={
                'qsub_args': f'-A {args.qsub_account_string} -l walltime=00:30:00 -l select=1:ncpus=1:mem=5gb'
            }
        )
    else:
        wf.run(
            plugin='MultiProc',
            plugin_args={
                'n_procs': args.n_procs
            }
        )

    show_warning_summary(logger)

    logger.log(LogLevel.INFO.value, 'Finished')

