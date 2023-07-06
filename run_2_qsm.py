#!/usr/bin/env python3

import sys
import os
import psutil
import glob
import copy
import argparse
import json

from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import DataSink
from nipype.pipeline.engine import Workflow, Node, MapNode
from nipype import config, logging
from scripts.qsmxt_functions import get_qsmxt_version, get_qsmxt_dir, get_diff, print_qsm_premades, gen_plugin_args
from scripts.sys_cmd import sys_cmd
from scripts.logger import LogLevel, make_logger, show_warning_summary, get_logger
from scripts.user_input import get_option, get_string, get_num, get_nums

from interfaces import nipype_interface_romeo as romeo
from interfaces import nipype_interface_processphase as processphase
from interfaces import nipype_interface_makehomogeneous as makehomogeneous
from interfaces import nipype_interface_axialsampling as sampling
from interfaces import nipype_interface_twopass as twopass
from interfaces import nipype_interface_nonzeroaverage as nonzeroaverage

from workflows.qsm import qsm_workflow
from workflows.masking import masking_workflow

def init_workflow(args):
    logger = get_logger('main')
    subjects = [
        os.path.split(path)[1]
        for path in glob.glob(os.path.join(args.bids_dir, args.subject_pattern))
        if not args.subjects or os.path.split(path)[1] in args.subjects
    ]
    if not subjects:
        logger.log(LogLevel.ERROR.value, f"No subjects found in {os.path.join(args.bids_dir, args.subject_pattern)}")
        script_exit(1)
    wf = Workflow("workflow_qsm", base_dir=args.output_dir)
    wf.add_nodes([
        node for node in
        [init_subject_workflow(args, subject) for subject in subjects]
        if node
    ])
    return wf

def init_subject_workflow(args, subject):
    logger = get_logger('main')
    sessions = [
        os.path.split(path)[1]
        for path in glob.glob(os.path.join(args.bids_dir, subject, args.session_pattern))
        if not args.sessions or os.path.split(path)[1] in args.sessions
    ]
    if not sessions:
        logger.log(LogLevel.ERROR.value, f"No sessions found in: {os.path.join(args.bids_dir, subject, args.session_pattern)}")
        script_exit(1)
    wf = Workflow(subject, base_dir=os.path.join(args.output_dir, "workflow_qsm"))
    wf.add_nodes([
        node for node in
        [init_session_workflow(args, subject, session) for session in sessions]
        if node
    ])
    return wf

def init_session_workflow(args, subject, session):
    logger = get_logger('main')
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

    if args.runs: runs = [run for run in runs if run in args.runs]
    
    wf = Workflow(session, base_dir=os.path.join(args.output_dir, "workflow_qsm", subject, session))
    wf.add_nodes([
        node for node in
        [init_run_workflow(copy.deepcopy(args), subject, session, run) for run in runs]
        if node
    ])
    return wf

def init_run_workflow(run_args, subject, session, run):
    logger = get_logger('main')
    logger.log(LogLevel.INFO.value, f"Creating nipype workflow for {subject}/{session}/{run}...")

    # get relevant files from this run
    phase_pattern = os.path.join(run_args.bids_dir, run_args.phase_pattern.format(subject=subject, session=session, run=run))
    phase_files = sorted(glob.glob(phase_pattern))[:run_args.num_echoes]
    
    magnitude_pattern = os.path.join(run_args.bids_dir, run_args.magnitude_pattern.format(subject=subject, session=session, run=run))
    magnitude_files = sorted(glob.glob(magnitude_pattern))[:run_args.num_echoes]

    params_pattern = os.path.join(run_args.bids_dir, run_args.phase_pattern.format(subject=subject, session=session, run=run).replace("nii.gz", "nii").replace("nii", "json"))
    params_files = sorted(glob.glob(params_pattern))[:run_args.num_echoes]
    
    mask_pattern = os.path.join(run_args.bids_dir, run_args.mask_pattern.format(subject=subject, session=session, run=run))
    mask_files = sorted(glob.glob(mask_pattern))[:run_args.num_echoes] if run_args.use_existing_masks else []
    
    # handle any errors related to files and adjust any settings if needed
    if not phase_files:
        logger.log(LogLevel.WARNING.value, f"Skipping run {subject}/{session}/{run} - no phase files found matching pattern {phase_pattern}.")
        return
    if len(phase_files) != len(params_files):
        logger.log(LogLevel.WARNING.value, f"Skipping run {subject}/{session}/{run} - an unequal number of JSON and phase files are present.")
        return
    if run_args.use_existing_masks:
        if not mask_files:
            logger.log(LogLevel.WARNING.value, f"Run {subject}/{session}/{run}: --use_existing_masks specified but no masks found matching pattern: {mask_pattern}. Reverting to {run_args.masking_algorithm} masking.")
        if len(mask_files) > 1 and len(mask_files) != len(phase_files):
            logger.log(LogLevel.WARNING.value, f"Run {subject}/{session}/{run}: --use_existing_masks specified but unequal number of mask and phase files present. Reverting to {run_args.masking_algorithm} masking.")
            mask_files = []
        if len(mask_files) > 1 and run_args.combine_phase:
            logger.log(LogLevel.WARNING.value, f"Run {subject}/{session}/{run}: --combine_phase specified but multiple masks found with --use_existing_masks. The first mask will be used only.")
            mask_files = [mask_files[0] for x in mask_files]
        if mask_files:
            run_args.inhomogeneity_correction = False
            run_args.two_pass = False
            run_args.single_pass = True
            run_args.add_bet = False
    if not magnitude_files:
        if run_args.masking_input == 'magnitude':
            logger.log(LogLevel.WARNING.value, f"Run {subject}/{session}/{run} will use phase-based masking - no magnitude files found matching pattern: {magnitude_pattern}.")
            run_args.masking_input = 'phase'
            run_args.masking_algorithm = 'threshold'
            run_args.inhomogeneity_correction = False
            run_args.add_bet = False
        if run_args.add_bet:
            logger.log(LogLevel.WARNING.value, f"Run {subject}/{session}/{run} cannot use --add_bet option - no magnitude files found matching pattern: {magnitude_pattern}.")
            run_args.add_bet = False
        if run_args.combine_phase:
            logger.log(LogLevel.WARNING.value, f"Run {subject}/{session}/{run} cannot use --combine_phase option - no magnitude files found matching pattern: {magnitude_pattern}.")
            run_args.combine_phase = False
    
    # create nipype workflow for this run
    wf = Workflow(run, base_dir=os.path.join(run_args.output_dir, "workflow_qsm", subject, session, run))

    # datasink
    n_outputs = Node(
        interface=DataSink(base_directory=run_args.output_dir),
        name='nipype_datasink'
    )

    # get files
    n_inputs = Node(
        IdentityInterface(
            fields=['phase', 'magnitude', 'params_files', 'mask']
        ),
        name='nipype_getfiles'
    )
    n_inputs.inputs.phase = phase_files
    n_inputs.inputs.magnitude = magnitude_files
    n_inputs.inputs.params_files = params_files
    if len(mask_files) == 1: mask_files = [mask_files[0] for _ in phase_files]
    n_inputs.inputs.mask = mask_files

    # read echotime and field strengths from json files
    def read_json_me(params_file):
        import json
        json_file = open(params_file, 'rt')
        data = json.load(json_file)
        te = data['EchoTime']
        json_file.close()
        return te
    def read_json_se(params_files):
        import json
        json_file = open(params_files[0], 'rt')
        data = json.load(json_file)
        B0 = data['MagneticFieldStrength']
        json_file.close()
        return B0
    mn_json_params = MapNode(
        interface=Function(
            input_names=['params_file'],
            output_names=['TE'],
            function=read_json_me
        ),
        iterfield=['params_file'],
        name='func_read-json-me'
    )
    wf.connect([
        (n_inputs, mn_json_params, [('params_files', 'params_file')])
    ])
    n_json_params = Node(
        interface=Function(
            input_names=['params_files'],
            output_names=['B0'],
            function=read_json_se
        ),
        iterfield=['params_file'],
        name='func_read-json-se'
    )
    wf.connect([
        (n_inputs, n_json_params, [('params_files', 'params_files')])
    ])

    # read voxel size 'vsz' from nifti file
    def read_nii(nii_file):
        import nibabel as nib
        if isinstance(nii_file, list): nii_file = nii_file[0]
        nii = nib.load(nii_file)
        return str(nii.header.get_zooms()).replace(" ", "")
    n_nii_params = Node(
        interface=Function(
            input_names=['nii_file'],
            output_names=['vsz'],
            function=read_nii
        ),
        name='nibabel_read-nii'
    )
    wf.connect([
        (n_inputs, n_nii_params, [('phase', 'nii_file')])
    ])

    # scale phase data
    mn_phase_scaled = MapNode(
        interface=processphase.ScalePhaseInterface(),
        iterfield=['phase'],
        name='nibabel_numpy_scale-phase'
    )
    wf.connect([
        (n_inputs, mn_phase_scaled, [('phase', 'phase')])
    ])    

    # reorient to canonical
    def as_closest_canonical(phase, magnitude=None, mask=None):
        import os
        import nibabel as nib
        from scripts.qsmxt_functions import extend_fname
        out_phase = extend_fname(phase, "_canonical", out_dir=os.getcwd())
        out_mag = extend_fname(magnitude, "_canonical", out_dir=os.getcwd()) if magnitude else None
        out_mask = extend_fname(mask, "_canonical", out_dir=os.getcwd()) if mask else None
        if nib.aff2axcodes(nib.load(phase).affine) == ('R', 'A', 'S'): return phase, magnitude, mask
        nib.save(nib.as_closest_canonical(nib.load(phase)), out_phase)
        if magnitude: nib.save(nib.as_closest_canonical(nib.load(magnitude)), out_mag)
        if mask: nib.save(nib.as_closest_canonical(nib.load(mask)), out_mask)
        return out_phase, out_mag, out_mask
    mn_inputs_canonical = MapNode(
        interface=Function(
            input_names=['phase'] + (['magnitude'] if magnitude_files else []) + (['mask'] if mask_files else []),
            output_names=['phase', 'magnitude', 'mask'],
            function=as_closest_canonical
        ),
        iterfield=['phase'] + (['magnitude'] if magnitude_files else []) + (['mask'] if mask_files else []),
        name='nibabel_as-canonical'
    )
    wf.connect([
        (mn_phase_scaled, mn_inputs_canonical, [('phase_scaled', 'phase')])
    ])
    if magnitude_files:
        wf.connect([
            (n_inputs, mn_inputs_canonical, [('magnitude', 'magnitude')]),
        ])
    if mask_files:
        wf.connect([
            (n_inputs, mn_inputs_canonical, [('mask', 'mask')]),
        ])
    
    # resample to axial
    n_inputs_resampled = Node(
        interface=IdentityInterface(
            fields=['phase', 'magnitude', 'mask']
        ),
        name='nipype_inputs-resampled'
    )
    if magnitude_files:
        mn_resample_inputs = MapNode(
            interface=sampling.AxialSamplingInterface(
                obliquity_threshold=999 if run_args.obliquity_threshold == -1 else run_args.obliquity_threshold
            ),
            iterfield=['magnitude', 'phase', 'mask'] if mask_files else ['magnitude', 'phase'],
            name='nibabel_numpy_nilearn_axial-resampling'
        )
        wf.connect([
            (mn_inputs_canonical, mn_resample_inputs, [('magnitude', 'magnitude')]),
            (mn_inputs_canonical, mn_resample_inputs, [('phase', 'phase')]),
            (mn_resample_inputs, n_inputs_resampled, [('magnitude', 'magnitude')]),
            (mn_resample_inputs, n_inputs_resampled, [('phase', 'phase')])
        ])
        if mask_files:
            wf.connect([
                (mn_inputs_canonical, mn_resample_inputs, [('mask', 'mask')]),
                (mn_resample_inputs, n_inputs_resampled, [('mask', 'mask')])
            ])
    else:
        wf.connect([
            (mn_inputs_canonical, n_inputs_resampled, [('phase', 'phase')])
        ])
        if mask_files:
            wf.connect([
                (mn_inputs_canonical, n_inputs_resampled, [('mask', 'mask')])
            ])

    # combine phase data if necessary
    n_inputs_combine = Node(
        interface=IdentityInterface(
            fields=['phase_unwrapped', 'frequency']
        ),
        name='phase-combined'
    )
    if run_args.combine_phase:
        n_romeo_combine = Node(
            interface=romeo.RomeoB0Interface(),
            name='mrt_romeo_combine',
        )
        wf.connect([
            (mn_json_params, n_romeo_combine, [('TE', 'TE')]),
            (n_inputs_resampled, n_romeo_combine, [('phase', 'phase')]),
            (n_inputs_resampled, n_romeo_combine, [('magnitude', 'magnitude')]),
            (n_romeo_combine, n_inputs_combine, [('frequency', 'frequency')]),
            (n_romeo_combine, n_inputs_combine, [('phase_unwrapped', 'phase_unwrapped')]),
        ])

    # === MASKING ===
    wf_masking = masking_workflow(
        run_args=run_args,
        mask_files=mask_files,
        magnitude_available=len(magnitude_files) > 0,
        qualitymap_available=False,
        fill_masks=True,
        add_bet=run_args.add_bet and run_args.filling_algorithm != 'bet',
        name="mask",
        index=0
    )
    wf.connect([
        (n_inputs_resampled, wf_masking, [('phase', 'masking_inputs.phase')]),
        (n_inputs_resampled, wf_masking, [('magnitude', 'masking_inputs.magnitude')]),
        (n_inputs_resampled, wf_masking, [('mask', 'masking_inputs.mask')]),
        (mn_json_params, wf_masking, [('TE', 'masking_inputs.TE')])
    ])
    '''
    if mask_files:
        wf.connect([
            (n_inputs_combine, wf_masking, [('mask', 'masking_inputs.mask')])
        ])
    if magnitude_files:
        wf.connect([
            (n_inputs_combine, wf_masking, [('magnitude', 'masking_inputs.magnitude')])
        ])
    '''
    wf.connect([
        (wf_masking, n_outputs, [('masking_outputs.mask', 'mask')])
    ])

    # === QSM ===
    wf_qsm = qsm_workflow(run_args, "qsm", len(magnitude_files) > 0, qsm_erosions=run_args.tgv_erosions)

    wf.connect([
        (n_inputs_resampled, wf_qsm, [('phase', 'qsm_inputs.phase')]),
        (n_inputs_combine, wf_qsm, [('phase_unwrapped', 'qsm_inputs.phase_unwrapped')]),
        (n_inputs_combine, wf_qsm, [('frequency', 'qsm_inputs.frequency')]),
        (n_inputs_resampled, wf_qsm, [('magnitude', 'qsm_inputs.magnitude')]),
        (wf_masking, wf_qsm, [('masking_outputs.mask', 'qsm_inputs.mask')]),
        (mn_json_params, wf_qsm, [('TE', 'qsm_inputs.TE')]),
        (n_json_params, wf_qsm, [('B0', 'qsm_inputs.B0')]),
        (n_nii_params, wf_qsm, [('vsz', 'qsm_inputs.vsz')])
    ])
    wf_qsm.get_node('qsm_inputs').inputs.b0_direction = "(0,0,1)"
    
    n_qsm_average = Node(
        interface=nonzeroaverage.NonzeroAverageInterface(),
        name="nibabel_numpy_qsm-average"
    )
    wf.connect([
        (wf_qsm, n_qsm_average, [('qsm_outputs.qsm', 'in_files')]),
        (wf_masking, n_qsm_average, [('masking_outputs.mask', 'in_masks')])
    ])
    wf.connect([
        (n_qsm_average, n_outputs, [('out_file', 'qsm_final' if not run_args.two_pass else 'qsm_filled')])
    ])

    # two-pass algorithm
    if run_args.two_pass:
        wf_masking_intermediate = masking_workflow(
            run_args=run_args,
            mask_files=mask_files,
            magnitude_available=len(magnitude_files) > 0,
            qualitymap_available=True,
            fill_masks=False,
            add_bet=False,
            name="mask-intermediate",
            index=1
        )
        wf.connect([
            (n_inputs_resampled, wf_masking_intermediate, [('phase', 'masking_inputs.phase')]),
            (n_inputs_resampled, wf_masking_intermediate, [('magnitude', 'masking_inputs.magnitude')]),
            (n_inputs_resampled, wf_masking_intermediate, [('mask', 'masking_inputs.mask')]),
            (wf_masking, wf_masking_intermediate, [('masking_outputs.quality_map', 'masking_inputs.quality_map')]),
            (mn_json_params, wf_masking_intermediate, [('TE', 'masking_inputs.TE')])
        ])
        '''
        if mask_files:
            wf.connect([
                (n_inputs_combine, wf_masking_intermediate, [('mask', 'masking_inputs.mask')])
            ])
        if magnitude_files:
            if run_args.inhomogeneity_correction:
                wf.connect([
                    (mn_inhomogeneity_correction, wf_masking_intermediate, [('magnitude_corrected', 'masking_inputs.magnitude')])
                ])
            else:
                wf.connect([
                    (n_inputs_combine, wf_masking_intermediate, [('magnitude', 'masking_inputs.magnitude')])
                ])
        '''

        wf_qsm_intermediate = qsm_workflow(run_args, "qsm-intermediate", len(magnitude_files) > 0, qsm_erosions=0)
        wf.connect([
            (n_inputs_resampled, wf_qsm_intermediate, [('phase', 'qsm_inputs.phase')]),
            (n_inputs_resampled, wf_qsm_intermediate, [('magnitude', 'qsm_inputs.magnitude')]),
            (n_inputs_combine, wf_qsm_intermediate, [('phase_unwrapped', 'qsm_inputs.phase_unwrapped')]),
            (n_inputs_combine, wf_qsm_intermediate, [('frequency', 'qsm_inputs.frequency')]),
            (mn_json_params, wf_qsm_intermediate, [('TE', 'qsm_inputs.TE')]),
            (n_json_params, wf_qsm_intermediate, [('B0', 'qsm_inputs.B0')]),
            (n_nii_params, wf_qsm_intermediate, [('vsz', 'qsm_inputs.vsz')]),
            (wf_masking_intermediate, wf_qsm_intermediate, [('masking_outputs.mask', 'qsm_inputs.mask')])
        ])
        wf_qsm_intermediate.get_node('qsm_inputs').inputs.b0_direction = "(0,0,1)"
                
        # two-pass combination
        mn_qsm_twopass = MapNode(
            interface=twopass.TwopassNiftiInterface(),
            name='numpy_nibabel_twopass',
            iterfield=['in_file1', 'in_file2', 'mask']
        )
        wf.connect([
            (wf_qsm_intermediate, mn_qsm_twopass, [('qsm_outputs.qsm', 'in_file1')]),
            (wf_masking_intermediate, mn_qsm_twopass, [('masking_outputs.mask', 'mask')]),
            (wf_qsm, mn_qsm_twopass, [('qsm_outputs.qsm', 'in_file2')])
        ])

        # averaging
        n_qsm_twopass_average = Node(
            interface=nonzeroaverage.NonzeroAverageInterface(),
            name="nibabel_numpy_twopass-average"
        )
        wf.connect([
            (mn_qsm_twopass, n_qsm_twopass_average, [('out_file', 'in_files')]),
            (wf_masking_intermediate, n_qsm_twopass_average, [('masking_outputs.mask', 'in_masks')])
        ])
        wf.connect([
            (n_qsm_twopass_average, n_outputs, [('out_file', 'qsm_final')])
        ])
        
    
    return wf

def parse_args(args, return_run_command=False):
    parser = argparse.ArgumentParser(
        description="QSMxT: QSM Reconstruction Pipeline",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    def argparse_bool(user_in):
        if user_in is None: return None
        if isinstance(user_in, bool): return user_in
        user_in = user_in.strip().lower()
        if user_in in ['on', 'true', 'yes']: return True
        if user_in in ['off', 'false', 'no']: return False
        raise ValueError(f"Invalid boolean value {user_in}; use on/yes/true or off/false/no")

    parser.add_argument(
        'bids_dir',
        nargs='?',
        default=None,
        type=os.path.abspath,
        help='Input data folder generated using run_1_dicomConvert.py. You can also use a ' +
             'previously existing BIDS folder. In this case, ensure that the --subject_pattern, '+
             '--session_pattern, --magnitude_pattern and --phase_pattern are correct for your data.'
    )

    parser.add_argument(
        'output_dir',
        nargs='?',
        default=None,
        type=os.path.abspath,
        help='Output QSM folder; will be created if it does not exist.'
    )

    parser.add_argument(
        '--pipeline_file',
        default=None,
        help=f"Specify a JSON file to use from which custom premade pipelines will be made available. "+
             f"See {os.path.join(get_qsmxt_dir(), 'qsm_pipelines.json')} for the default pipelines."
    )
    
    parser.add_argument(
        '--premade',
        default=None,
        help="Specify a premade pipeline to use as the default. By default, this is 'default'. The "+
             "name of the pipeline must be present in either " +
            f"{os.path.join(get_qsmxt_dir(), 'qsm_pipelines.json')} or in --pipeline_file."
    )
    
    parser.add_argument(
        '--subject_pattern',
        default=None,
        help='Pattern used to match subject folders in bids_dir'
    )

    parser.add_argument(
        '--session_pattern',
        default=None,
        help='Pattern used to match session folders in subject folders'
    )

    parser.add_argument(
        '--magnitude_pattern',
        default=None,
        help='Pattern to match magnitude files within the BIDS directory. ' +
             'The {subject}, {session} and {run} placeholders must be present.'
    )

    parser.add_argument(
        '--phase_pattern',
        default=None,
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
        '--runs',
        default=None,
        nargs='*',
        help='List of runs to process (e.g. \'run-1\'); by default all runs are processed.'
    )

    parser.add_argument(
        '--num_echoes',
        dest='num_echoes',
        default=None,
        type=int,
        help='The number of echoes to process; by default all echoes are processed.'
    )

    parser.add_argument(
        '--obliquity_threshold',
        type=int,
        default=None,
        help="The 'obliquity' as measured by nilearn from which oblique-acquired acqisitions should be " +
             "axially resampled. Use -1 to disable resampling completely."
    )

    parser.add_argument(
        '--combine_phase',
        nargs='?',
        type=argparse_bool,
        const=True,
        default=None,
        help="Combines multi-echo phase images by generating a field map using ROMEO."
    )

    parser.add_argument(
        '--qsm_algorithm',
        default=None,
        choices=['tgv', 'tv', 'nextqsm', 'rts'],
        help="QSM algorithm. The tgv algorithm is based on doi:10.1016/j.neuroimage.2015.02.041 from "+
             "Langkammer et al., and includes unwrapping and background field removal steps as part of a "+
             "combined optimisation. The NeXtQSM option requires NeXtQSM installed (available by default in the "+
             "QSMxT container) and uses a deep learning model implemented in Tensorflow based on "+
             "doi:10.48550/arXiv.2107.07752 from Cognolato et al., and combines the QSM inversion with a "+
             "background field removal step. The RTS algorithm is based on doi:10.1016/j.neuroimage.2017.11.018 "+
             "from Kames C. et al., and solves only the dipole-inversion step, requiring separate unwrapping and "+
             "background field removal steps. "
    )

    parser.add_argument(
        '--tgv_iterations',
        type=int,
        default=None,
        help='Number of iterations used by tgv.'
    )

    parser.add_argument(
        '--tgv_alphas',
        type=float,
        default=None,
        nargs=2,
        help='Regularisation alphas used by tgv.'
    )

    parser.add_argument(
        '--tgv_erosions',
        type=int,
        default=None,
        help='Number of erosions applied by tgv.'
    )
    
    parser.add_argument(
        '--unwrapping_algorithm',
        default=None,
        choices=['romeo', 'romeo-combined', 'laplacian'],
        help="Phase unwrapping algorithm. ROMEO is based on doi:10.1002/mrm.28563 from Eckstein et al. "+
             "Laplacian is based on doi:10.1364/OL.28.001194 and doi:10.1002/nbm.3064 from Schofield MA. "+
             "et al. and Zhou D. et al., respectively. ROMEO is the default when --qsm_algorithm is set to "+
             "rts or nextqsm, and no unwrapping is applied by default when --qsm_algorithm is set to tgv."
    )

    parser.add_argument(
        '--bf_algorithm',
        default=None,
        choices=['vsharp', 'pdf'],
        help='Background field correction algorithm. V-SHARP is based on doi:10.1002/mrm.23000 PDF is '+
             'based on doi:10.1002/nbm.1670.'
    )

    parser.add_argument(
        '--masking_algorithm',
        default=None,
        choices=['threshold', 'bet'],
        help='Masking algorithm. Threshold-based masking uses a simple binary threshold applied to the '+
             '--masking_input, followed by a hole-filling strategy determined by the --filling_algorithm. '+
             'BET masking generates a mask using the Brain Extraction Tool (BET) based on '+
             'doi:10.1002/hbm.10062 from Smith SM. The default algorithm is \'threshold\'.'
    )

    parser.add_argument(
        '--masking_input',
        default=None,
        choices=['phase', 'magnitude'],
        help='Input to the masking algorithm. Phase-based masking may reduce artefacts near the ROI '+
             'boundary (see doi:10.1002/mrm.29368 from Hagberg et al.). Phase-based masking creates a '+
             'quality map based on the second-order spatial phase gradients using ROMEO '+
             '(doi:10.1002/mrm.28563 from Eckstein et al.). The default masking input is the phase, '+
             'but is forcibly set to the magnitude if BET-masking is used.'
    )

    parser.add_argument(
        '--threshold_value',
        type=float,
        nargs='+',
        default=None,
        help='Masking threshold for when --masking_algorithm is set to threshold. Values between 0 and 1'+
             'represent a percentage of the multi-echo input range. Values greater than 1 represent an '+
             'absolute threshold value. Lower values will result in larger masks. If no threshold is '+
             'provided, the --threshold_algorithm is used to select one automatically.'
    )

    parser.add_argument(
        '--threshold_algorithm',
        default=None,
        choices=['otsu', 'gaussian'],
        help='Algorithm used to select a threshold for threshold-based masking if --threshold_value is '+
             'left unspecified. The gaussian method is based on doi:10.1016/j.compbiomed.2012.01.004 '+
             'from Balan AGR. et al. The otsu method is based on doi:10.1109/TSMC.1979.4310076 from Otsu '+
             'et al.'
    )

    parser.add_argument(
        '--filling_algorithm',
        default=None,
        choices=['morphological', 'gaussian', 'both', 'bet'],
        help='Algorithm used to fill holes for threshold-based masking. By default, a gaussian smoothing '+
             'operation is applied first prior to a morphological hole-filling operation. Note that gaussian '+
             'smoothing may fill some unwanted regions (e.g. connecting the skull and brain tissue), whereas '+
             'morphological hole-filling alone may fail to fill desired regions if they are not fully enclosed.'+
             'The BET option is applicable to two-pass QSM only, and will use ONLY a BET mask as the filled '+
             'version of the mask.'
    )

    parser.add_argument(
        '--threshold_algorithm_factor',
        default=None,
        nargs='+',
        type=float,
        help='Factor to multiply the algorithmically-determined threshold by. Larger factors will create '+
             'smaller masks.'
    )

    parser.add_argument(
        '--mask_erosions',
        type=int,
        nargs='+',
        default=None,
        help='Number of erosions applied to masks prior to QSM processing steps. Note that some algorithms '+
             'may erode the mask further (e.g. V-SHARP and TGV-QSM).'
    )

    parser.add_argument(
        '--inhomogeneity_correction',
        nargs='?',
        type=argparse_bool,
        const=True,
        default=None,
        help='Applies an inhomogeneity correction to the magnitude prior to masking based on '+
             'https://index.mirasmart.com/ISMRM2019/PDFfiles/2716.html from Eckstein et al. This option '+
             'is only relevant when the --masking_input is the magnitude.'
    )

    parser.add_argument(
        '--add_bet',
        nargs='?',
        type=argparse_bool,
        const=True,
        default=None,
        help='Combines the chosen masking method with BET. This option is only relevant when the '+
             '--masking_algorithm is set to threshold.'
    )

    parser.add_argument(
        '--bet_fractional_intensity',
        type=float,
        default=None,
        help='Fractional intensity for BET masking operations.'
    )
    
    parser.add_argument(
        '--use_existing_masks',
        nargs='?',
        type=argparse_bool,
        const=True,
        default=None,
        help='This option will use existing masks from the BIDS folder, where possible, instead of '+
             'generating new ones. The masks will be selected based on the --mask_pattern argument. '+
             'A single mask may be present (and will be applied to all echoes), or a mask for each '+
             'echo can be used. When existing masks cannot be found, the --masking_algorithm will '+
             'be used as a fallback.'
    )
    
    parser.add_argument(
        '--mask_pattern',
        default=None,
        help='Pattern used to identify mask files to be used when the --use_existing_masks option '+
             'is enabled.'
    )

    
    parser.add_argument(
        '--two_pass',
        nargs='?',
        type=argparse_bool,
        const=True,
        default=None,
        help='Setting this to \'on\' will perform a QSM reconstruction in a two-stage fashion to reduce '+
             'artefacts; combines the results from two QSM images reconstructed using masks that separate '+
             'more reliable and less reliable phase regions. Note that this option requires threshold-based '+
             'masking, doubles reconstruction time, and in some cases can deteriorate QSM contrast in some '+
             'regions, depending on other parameters such as the threshold. Applications where two-pass QSM '+
             'may improve results include body imaging, lesion imaging, and imaging of other strong '+
             'susceptibility sources. This method is based on doi:10.1002/mrm.29048 from Stewart et al. By '+
             'default, two-pass is enabled for the RTS algorithm only.'
    )

    parser.add_argument(
        '--pbs',
        default=None,
        dest='pbs',
        help='Run the pipeline via PBS and use the argument as the account string.'
    )

    parser.add_argument(
        '--slurm',
        metavar=('ACCOUNT_STRING', 'PARITITON'),
        nargs=2,
        default=(None, None),
        dest='slurm',
        help='Run the pipeline via SLURM and use the argument as the account string.'
    )

    parser.add_argument(
        '--n_procs',
        type=int,
        default=None,
        help='Number of processes to run concurrently for MultiProc. By default, the number of available '+
             'CPUs is used.'
    )

    parser.add_argument(
        '--debug',
        action='store_true',
        default=None,
        help='Enables some nipype settings for debugging.'
    )

    parser.add_argument(
        '--list_premades',
        action='store_true',
        default=None,
        help='List the possible premade pipelines only.'
    )

    parser.add_argument(
        '--dry',
        action='store_true',
        default=None,
        help='Creates the nipype pipeline using the chosen settings, but does not execute it. Useful for '+
             'debugging purposes, or for creating a references file.'
    )

    parser.add_argument(
        '--auto_yes',
        action='store_true',
        default=None,
        help='Runs the pipeline in non-interactive mode.'
    )

    logger = get_logger('pre')
    
    # parse explicit arguments ONLY
    args = parser.parse_args(args)

    # if listing premades, skip the rest
    if args.list_premades:
        if return_run_command:
            return args, str.join(' ', sys.argv)
        return args

    # bids and output are required
    if args.bids_dir is None or args.output_dir is None:
        logger.log(LogLevel.ERROR.value, "Values for --bids_dir and --output_dir are required!")
        script_exit(1)

    explicit_args = {}
    for k in args.__dict__:
        if args.__dict__[k] is not None:
            explicit_args[k] = args.__dict__[k]

    # get implicit args based on usual defaults
    pipeline_file = f"{os.path.join(get_qsmxt_dir(), 'qsm_pipelines.json')}"
    with open(pipeline_file, "r") as json_file:
        premades = json.load(json_file)
    implicit_args = premades['default']
    
    # update implicit args based on any premade pipelines
    if args.pipeline_file:
        with open(args.pipeline_file, "r") as json_file:
            user_premades = json.load(json_file)
        premades.update(user_premades)
    if 'premade' in explicit_args.keys():
        if explicit_args['premade'] in premades:
            for key, value in premades[explicit_args['premade']].items():
                if key not in explicit_args or explicit_args[key] == value:
                    implicit_args[key] = value
        else:
            logger.log(LogLevel.ERROR.value, f"Chosen premade pipeline '{explicit_args['premade']}' not found!")
            if args.auto_yes: script_exit(1)
            del explicit_args['premade']
    elif 'premade' in implicit_args.keys():
        if implicit_args['premade'] in premades:
            for key, value in premades[implicit_args['premade']].items():
                implicit_args[key] = value
        else:
            logger.log(LogLevel.ERROR.value, f"Chosen premade pipeline '{implicit_args['premade']}' not found!")
            del implicit_args['premade']
    
    # remove any unnecessary explicit args
    for key, value in implicit_args.items():
        if key in explicit_args and explicit_args[key] == value:
            del explicit_args[key]

    # create final args
    final_args = implicit_args.copy()
    for key, value in explicit_args.items():
        final_args[key] = value

    # get adjustments from the user
    if not final_args['auto_yes']:
        final_args2, implicit_args = get_interactive_args(final_args.copy(), explicit_args, implicit_args, premades)
        for key, val in final_args2.items():
            if key not in implicit_args or implicit_args[key] != val:
                explicit_args[key] = val
            final_args[key] = val

    # remove any unnecessary explicit args
    for key, value in implicit_args.items():
        if key in explicit_args and explicit_args[key] == value:
            del explicit_args[key]
    
    # update the arguments using the computed ones
    keys = set(vars(args)) & set(final_args)
    for key in keys:
        vars(args)[key] = final_args[key]
    
    # compute the minimum run command to re-execute the built pipeline non-interactively
    if return_run_command:
        run_command = f"run_2_qsm.py {explicit_args['bids_dir']} {explicit_args['output_dir']}"
        if 'premade' in explicit_args and explicit_args['premade'] != 'default':
            run_command += f" --premade '{explicit_args['premade']}'"
        for key, value in explicit_args.items():
            if key in ['bids_dir', 'output_dir', 'auto_yes', 'premade', 'multiproc', 'mem_avail', 'n_procs', 'single_pass']: continue
            elif value == True: run_command += f' --{key}'
            elif value == False: run_command += f' --{key} off'
            elif isinstance(value, str): run_command += f" --{key} '{value}'"
            elif isinstance(value, (int, float)) and value != False: run_command += f" --{key} {value}"
            elif isinstance(value, list):
                run_command += f" --{key}"
                for val in value:
                    run_command += f" {val}"
        run_command += ' --auto_yes'
        return args, run_command
    return args

def get_interactive_args(args, explicit_args, implicit_args, premades):
    class dotdict(dict):
        """dot.notation access to dictionary attributes"""
        __getattr__ = dict.get
        __setattr__ = dict.__setitem__
        __delattr__ = dict.__delitem__
    args = dotdict(args)
        
    # allow user to update the premade if none was chosen
    if not args.premade:
        print("\n=== Premade pipelines ===")

        for key, value in premades.items():
            print(f"{key}", end="")
            if "description" in value:
                print(f": {value['description']}")
            else:
                print()

        args.premade = get_option(
            prompt="\nSelect a premade to begin [default - 'default']: ",
            options=premades.keys(),
            default='default'
        )
        for key, value in premades[args.premade].items():
            if key not in explicit_args and key != 'premade':
                args[key] = value
            implicit_args[key] = value

    # pipeline customisation
    while True:
        args = process_args(args)
        print("\n(1) Masking:")
        print(f" - Use existing masks if available: {'Yes' if args.use_existing_masks else 'No'}")
        if args.masking_algorithm == 'threshold':
            print(f" - Masking algorithm: threshold ({args.masking_input}-based{('; inhomogeneity-corrected)' if args.masking_input == 'magnitude' and args.inhomogeneity_correction else ')')}")
            print(f"   - Two-pass artefact reduction: {'Enabled' if args.two_pass else 'Disabled'}")
            if args.threshold_value:
                if len(args.threshold_value) >= 2 and all(args.threshold_value) and args.two_pass:
                    if int(args.threshold_value[0]) == float(args.threshold_value[0]) and int(args.threshold_value[1]) == float(args.threshold_value[1]):
                        print(f"   - Threshold: {int(args.threshold_value[0])}, {int(args.threshold_value[1])} (hardcoded voxel intensities)")
                    else:
                        print(f"   - Threshold: {float(args.threshold_value[0])}%, {float(args.threshold_value[1])}% (hardcoded percentiles of the signal histogram)")
                elif len(args.threshold_value) == 1 and all(args.threshold_value):
                    if int(args.threshold_value[0]) == float(args.threshold_value[0]):
                        print(f"   - Threshold: {int(args.threshold_value[0])} (hardcoded voxel intensity)")
                    else:
                        print(f"   - Threshold: {float(args.threshold_value[0])}% (hardcoded percentile of per-echo histogram)")
            else:
                print(f"   - Threshold algorithm: {args.threshold_algorithm}", end="")
                if len(args.threshold_algorithm_factor) >= 2 and args.two_pass:
                    print(f" (x{args.threshold_algorithm_factor[0]} for single-pass; x{args.threshold_algorithm_factor[1]} for two-pass)")
                elif len(args.threshold_algorithm_factor):
                    print(f" (x{args.threshold_algorithm_factor[0]})")
                else:
                    print()
            print(f"   - Hole-filling algorithm: {'morphological+gaussian' if args.filling_algorithm == 'both' else args.filling_algorithm}{'+bet' if args.add_bet else ''}{f' (bet fractional intensity = {args.bet_fractional_intensity})' if args.add_bet else ''}")
            if args.two_pass and len(args.mask_erosions) == 2:
                print(f"   - Erosions: {args.mask_erosions[0]} erosions for single-pass; {args.mask_erosions[1]} erosions for two-pass")
        else:
            print(f" - Masking algorithm: {args.masking_algorithm}{f' (fractional intensity = {args.bet_fractional_intensity})' if 'bet' in args.masking_algorithm else ''}")
            print(f"   - Erosions: {args.mask_erosions[0]}")
        
        print("\n(2) Phase processing:")
        print(f" - Axial resampling: " + (f"Enabled (obliquity threshold = {args.obliquity_threshold})" if args.obliquity_threshold != -1 else "Disabled"))
        print(f" - Multi-echo combination: " + ("B0 mapping (using ROMEO)" if args.combine_phase else "Susceptibility averaging"))
        if args.qsm_algorithm not in ['tgv']:
            print(f" - Phase unwrapping: {args.unwrapping_algorithm}")
            if args.qsm_algorithm not in ['nextqsm']:
                print(f" - Background field removal: {args.bf_algorithm}")
        print(f" - Dipole inversion: {args.qsm_algorithm}")
        
        user_in = get_option(
            prompt="\nEnter a number to customize; enter 'run' to run: ",
            options=['1', '2', 'run'],
            default=None
        )
        if user_in == 'run': break
        
        if user_in == '1': # MASKING
            print("=== MASKING ===")

            print("\n== Existing masks ==")
            args.use_existing_masks = 'yes' == get_option(
                prompt=f"Use existing masks if available [default: {'yes' if args.use_existing_masks else 'no'}]: ",
                options=['yes', 'no'],
                default='yes' if args.use_existing_masks else 'no'
            )
            if args.use_existing_masks:
                args.mask_pattern = get_string(
                    prompt=f"Enter mask file pattern [default: {args.mask_pattern}]: ",
                    default=args.mask_pattern
                )
            
            print("\n== Masking algorithm ==")
            print("threshold: ")
            print("     - required for the two-pass artefact reduction algorithm (https://doi.org/10.1002/mrm.29048)")
            print("     - required for applications other than in vivo human brain")
            print("     - more robust to severe pathology")
            print("bet: Applies the Brain Extraction Tool (standalone version)")
            print("     - the standard in most QSM pipelines")
            print("     - robust in healthy human brains")
            print("     - Paper: https://doi.org/10.1002/hbm.10062")
            print("     - Code: https://github.com/liangfu/bet2")
            print("\nNOTE: Even if you are using premade masks, a masking method is required as a backup.\n")
            args.masking_algorithm = get_option(
                prompt=f"Select masking algorithm [default - {args.masking_algorithm}]: ",
                options=['bet', 'threshold'],
                default=args.masking_algorithm
            )

            if 'bet' in args.masking_algorithm:
                args.bet_fractional_intensity = get_num(
                    prompt=f"\nBET fractional intensity [default - {args.bet_fractional_intensity}]: ",
                    min_val=0,
                    max_val=1,
                    default=args.bet_fractional_intensity
                )

            if args.masking_algorithm == 'threshold':
                print("\n== Threshold input ==")
                print("Select the input to be used in the thresholding algorithm.\n")
                print("magnitude: use the MRI signal magnitude")
                print("  - standard approach")
                print("  - requires magnitude images")
                print("phase: use a phase quality map")
                print("  - phase quality map produced by ROMEO (https://doi.org/10.1002/mrm.28563)")
                print("  - measured between 0 and 100")
                print("  - some evidence that phase-based masks are more reliable near the brain boundary (https://doi.org/10.1002/mrm.29368)")

                args.masking_input = get_option(
                    prompt=f"\nSelect threshold input [default - {args.masking_input}]: ",
                    options=['magnitude', 'phase'],
                    default=args.masking_input
                )

                if args.masking_input == 'magnitude':
                    args.inhomogeneity_correction = 'yes' == get_option(
                        prompt=f"\nApply inhomogeneity correction to magnitude [default: {'yes' if args.inhomogeneity_correction else 'no'}]: ",
                        options=['yes', 'no'],
                        default='yes' if args.inhomogeneity_correction else 'no'
                    )

                print("\n== Two-pass Artefact Reduction ==")
                print("Select whether to use the two-pass artefact reduction algorithm (https://doi.org/10.1002/mrm.29048).\n")
                print("  - reduces artefacts, particularly near strong susceptibility sources")
                print("  - sometimes requires tweaking of the mask to maintain accuracy in high-susceptibility regions")
                print("  - single-pass results will still be included in the output")
                print("  - doubles the runtime of the pipeline")
                args.two_pass = 'on' == get_option(
                    f"\nSelect on or off [default - {'on' if args.two_pass else 'off'}]: ",
                    options=['on', 'off'],
                    default='on' if args.two_pass else 'off'
                )

                print("\n== Threshold value ==")
                print("Select an algorithm to automate threshold selection, or enter a custom threshold.\n")
                print("otsu: Automate threshold selection using the Otsu algorithm (https://doi.org/10.1109/TSMC.1979.4310076)")
                print("gaussian: Automate threshold selection using a Gaussian algorithm (https://doi.org/10.1016/j.compbiomed.2012.01.004)")
                print("\nHardcoded threshold:")
                print(" - Use an integer to indicate an absolute signal intensity")
                print(" - Use a floating-point value from 0-1 to indicate a percentile of the per-echo signal histogram")
                if args.two_pass: print(" - Use two values to specify different thresholds for each pass in two-pass QSM")
                while True:
                    user_in = input(f"\nSelect threshold algorithm or value [default - {args.threshold_value if args.threshold_value != None else args.threshold_algorithm if args.threshold_algorithm else 'otsu'}]: ")
                    if user_in == "":
                        break
                    elif user_in in ['otsu', 'gaussian']:
                        args.threshold_algorithm = user_in
                        break
                    else:
                        try:
                            user_in = [float(val) for val in user_in.split(" ")]
                        except ValueError:
                            continue
                        if not (1 <= len(user_in) <= 2):
                            continue
                        if all(val == int(val) for val in user_in):
                            args.threshold_value = [int(val) for val in user_in]
                        else:
                            args.threshold_value = user_in
                        break

                if args.threshold_value != None: args.threshold_algorithm = None
                if args.threshold_value == None and not args.threshold_algorithm:
                    args.threshold_algorithm = 'otsu'

                if args.threshold_algorithm in ['otsu', 'gaussian']:
                    args.threshold_value = None
                    print("\n== Threshold algorithm factors ==")
                    print("The threshold algorithm can be tweaked by multiplying it by some factor.")
                    print("Use two values to specify different factors for each pass in two-pass QSM")
                    args.threshold_algorithm_factor = get_nums(
                        prompt=f"\nEnter threshold algorithm factor(s) (space-separated) [default - {str(args.threshold_algorithm_factor)}]: ",
                        default=args.threshold_algorithm_factor,
                        min_val=0,
                        max_n=2
                    )
                    
                print("\n== Filled mask algorithm ==")
                print("Threshold-based masking requires an algorithm to create a filled mask.\n")
                print("gaussian:")
                print(" - applies the scipy gaussian_filter function to the threshold mask")
                print(" - may fill some unwanted regions (e.g. connecting skull to brain)")
                print("morphological:")
                print(" - applies the scipy binary_fill_holes function to the threshold mask")
                print("both:")
                print(" - applies both methods (gaussian followed by morphological) to the threshold mask")
                print("bet:")
                print(" - uses a BET mask as the filled mask")
                args.filling_algorithm = get_option(
                    prompt=f"\nSelect hole-filling algorithm: [default - {args.filling_algorithm}]: ",
                    options=['gaussian', 'morphological', 'both', 'bet'],
                    default=args.filling_algorithm
                )
                if args.filling_algorithm != 'bet':
                    args.add_bet = 'yes' == get_option(
                        prompt=f"\nInclude a BET mask in the hole-filling operation (yes or no) [default - {'yes' if args.add_bet else 'no'}]: ",
                        options=['yes', 'no'],
                        default='yes' if args.add_bet else 'no'
                    )
                if args.add_bet:
                    args.bet_fractional_intensity = get_num(
                        prompt=f"\nBET fractional intensity [default - {args.bet_fractional_intensity}]: ",
                        default=args.bet_fractional_intensity
                    )
        
            print("\n== Erosions ==")
            print("The number of times to erode the mask.")
            print("Use two values to specify different erosion for each pass in two-pass QSM")
            args.mask_erosions = get_nums(
                prompt=f"\nEnter number of erosions [default - {str(args.mask_erosions)}]: ",
                default=args.mask_erosions,
                min_val=0,
                max_n=2,
                dtype=int
            )
        if user_in == '2': # PHASE PROCESSING
            print("== Resample to axial ==")
            print("This step will perform axial resampling for oblique acquisitions.")
            args.obliquity_threshold = get_num(
                prompt=f"\nEnter an obliquity threshold to cause resampling or -1 for none [default - {args.obliquity_threshold}]: ",
                default=args.obliquity_threshold
            )

            print("\n== Combine phase ==")
            print("This step will combine multi-echo phase data by generating a field map using ROMEO.")
            print("This will also force the use of ROMEO for the phase unwrapping step.")
            args.combine_phase = 'yes' == get_option(
                prompt=f"\nCombine multi-echo phase data [default - {'yes' if args.combine_phase else 'no'}]: ",
                options=['yes', 'no'],
                default='yes' if args.combine_phase else 'no'
            )
            if args.combine_phase: args.unwrapping_algorithm = 'romeo'

            print("\n== QSM Algorithm ==")
            print("rts: Rapid Two-Step QSM")
            print("   - https://doi.org/10.1016/j.neuroimage.2017.11.018")
            print("   - Compatible with two-pass artefact reduction algorithm")
            print("   - Fast runtime")
            print("tv: Fast quantitative susceptibility mapping with L1-regularization and automatic parameter selection")
            print("   - https://doi.org/10.1002/mrm.25029")
            print("tgv: Total Generalized Variation")
            print("   - https://doi.org/10.1016/j.neuroimage.2015.02.041")
            print("   - Combined unwrapping, background field removal and dipole inversion")
            print("   - Most stable with custom masks")
            print("   - Long runtime")
            print("   - Compatible with two-pass artefact reduction algorithm")
            print("nextqsm: NeXtQSM")
            print("   - https://doi.org/10.1016/j.media.2022.102700")
            print('   - Uses deep learning to solve the background field removal and dipole inversion steps')
            print('   - High memory requirements (>=12gb recommended)')
            args.qsm_algorithm = get_option(
                prompt=f"\nSelect QSM algorithm [default - {args.qsm_algorithm}]: ",
                options=['rts', 'tv', 'tgv', 'nextqsm'],
                default=args.qsm_algorithm
            )

            if args.qsm_algorithm in ['rts', 'nextqsm'] and not args.combine_phase:
                print("\n== Unwrapping algorithm ==")
                print("romeo: (https://doi.org/10.1002/mrm.28563)")
                print(" - quantitative")
                print("laplacian: (https://doi.org/10.1364/OL.28.001194; https://doi.org/10.1002/nbm.3064)")
                print(" - non-quantitative")
                print(" - popular for its numerical simplicity")
                args.unwrapping_algorithm = get_option(
                    prompt=f"\nSelect unwrapping algorithm [default - {args.unwrapping_algorithm}]: ",
                    options=['romeo', 'laplacian'],
                    default=args.unwrapping_algorithm
                )

            if args.qsm_algorithm in ['rts']:
                print("\n== Background field removal ==")
                print("vsharp: V-SHARP algorithm (https://doi.org/10.1002/mrm.23000)")
                print(" - fast")
                print(" - involves a mask erosion step that impacts the next steps")
                print(" - less reliable with threshold-based masks")
                print(" - not compatible with artefact reduction algorithm")
                print("pdf: Projection onto Dipole Fields algorithm (https://doi.org/10.1002/nbm.1670)")
                print(" - slower")
                print(" - more accurate")
                print(" - does not require an additional erosion step")
                args.bf_algorithm = get_option(
                    prompt=f"\nSelect background field removal algorithm [default - {args.bf_algorithm}]: ",
                    options=['vsharp', 'pdf'],
                    default=args.bf_algorithm
                )
    return args.copy(), implicit_args

def create_logger(name, logpath=None):
    logger = make_logger(
        name=name,
        logpath=logpath,
        printlevel=LogLevel.INFO,
        writelevel=LogLevel.INFO,
        warnlevel=LogLevel.WARNING,
        errorlevel=LogLevel.ERROR
    )
    return logger

def process_args(args):
    # default QSM algorithms
    if not args.qsm_algorithm:
        args.qsm_algorithm = 'rts'

    # default masking settings for QSM algorithms
    if not args.masking_algorithm:
        if args.qsm_algorithm == 'nextqsm':
            args.masking_algorithm = 'bet'
        else:
            args.masking_algorithm = 'threshold'
    
    # force masking input to magnitude if bet is the masking method
    args.masking_input = 'magnitude' if 'bet' in args.masking_algorithm else args.masking_input

    # default threshold settings
    if args.masking_algorithm == 'threshold':
        if not (args.threshold_value or args.threshold_algorithm):
            args.threshold_algorithm = 'otsu'
        if not args.filling_algorithm:
            args.filling_algorithm = 'both'

    # default unwrapping settings for QSM algorithms
    if not args.unwrapping_algorithm:
        if args.qsm_algorithm in ['nextqsm', 'rts', 'tv']:
            args.unwrapping_algorithm = 'romeo'
    if args.combine_phase and args.unwrapping_algorithm != 'romeo':
        args.unwrapping_algorithm = 'romeo'

    if args.qsm_algorithm == 'tgv':
        args.unwrapping_algorithm = None

    # add_bet option only works with non-bet masking and filling methods
    args.add_bet &= 'bet' not in args.masking_algorithm
    args.add_bet &= 'bet' != args.filling_algorithm

    # default two-pass settings for QSM algorithms
    if args.two_pass is None:
        if args.qsm_algorithm in ['rts', 'tgv', 'tv']:
            args.two_pass = True
        else:
            args.two_pass = False
    
    # two-pass does not work with bet masking, nextqsm, or vsharp
    args.two_pass &= 'bet' not in args.masking_algorithm
    args.two_pass &= args.qsm_algorithm != 'nextqsm'
    args.two_pass &= not (args.bf_algorithm == 'vsharp' and args.qsm_algorithm in ['tv', 'rts', 'nextqsm'])

    # single-pass variable once confirmed
    args.single_pass = not args.two_pass

    # 'bet' hole-filling not applicable for single-pass
    if args.single_pass and args.filling_algorithm == 'bet':
        args.filling_algorithm == 'both'

    # decide on inhomogeneity correction
    args.inhomogeneity_correction &= (args.add_bet or args.masking_input == 'magnitude' or args.filling_algorithm == 'bet')
    
    # set number of concurrent processes to run depending on available resources
    if not args.n_procs:
        args.n_procs = int(os.environ["NCPUS"] if "NCPUS" in os.environ else os.cpu_count())
    

    # get rough estimate of available memory
    args.mem_avail = psutil.virtual_memory().available / (1024 ** 3)
    
    # determine whether multiproc will be used
    args.multiproc = not (args.pbs or any(args.slurm))

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

    return args

def set_env_variables(args):
    # misc environment variables
    os.environ["FSLOUTPUTTYPE"] = "NIFTI"

    # path environment variable
    os.environ["PATH"] += os.pathsep + os.path.join(get_qsmxt_dir(), "scripts")

    # add this_dir and cwd to pythonpath
    if "PYTHONPATH" in os.environ: os.environ["PYTHONPATH"] += os.pathsep + get_qsmxt_dir()
    else:                          os.environ["PYTHONPATH"]  = get_qsmxt_dir()

def write_citations(wf):
    # get all node names
    node_names = [node._name.lower() for node in wf._get_all_nodes()]

    def any_string_matches_any_node(strings):
        return any(string in node_name for string in strings for node_name in node_names)

    # write "references.txt" with the command used to invoke the script and any necessary references
    with open(os.path.join(args.output_dir, "references.txt"), 'w', encoding='utf-8') as f:
        # qsmxt, nipype, numpy
        f.write("== Citations ==")
        f.write(f"\n\n - QSMxT{'' if not args.two_pass else ' and two-pass combination method'}: Stewart AW, Robinson SD, O'Brien K, et al. QSMxT: Robust masking and artifact reduction for quantitative susceptibility mapping. Magnetic Resonance in Medicine. 2022;87(3):1289-1300. doi:10.1002/mrm.29048")
        f.write("\n\n - QSMxT: Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT")
        
        if any_string_matches_any_node(['correct-inhomogeneity']):
            f.write("\n\n - Inhomogeneity correction: Eckstein K, Trattnig S, Simon DR. A Simple homogeneity correction for neuroimaging at 7T. In: Proc. Intl. Soc. Mag. Reson. Med. International Society for Magnetic Resonance in Medicine; 2019. Abstract 2716. https://index.mirasmart.com/ISMRM2019/PDFfiles/2716.html")
        if any_string_matches_any_node(['bet']):
            f.write("\n\n - Brain extraction: Smith SM. Fast robust automated brain extraction. Human Brain Mapping. 2002;17(3):143-155. doi:10.1002/hbm.10062")
            f.write("\n\n - Brain extraction: Liangfu Chen. liangfu/bet2 - Standalone Brain Extraction Tool. GitHub; 2015. https://github.com/liangfu/bet2")
        if any_string_matches_any_node(['threshold-masking']) and args.threshold_algorithm == 'gaussian':
            f.write("\n\n - Threshold selection algorithm - gaussian: Balan AGR, Traina AJM, Ribeiro MX, Marques PMA, Traina Jr. C. Smart histogram analysis applied to the skull-stripping problem in T1-weighted MRI. Computers in Biology and Medicine. 2012;42(5):509-522. doi:10.1016/j.compbiomed.2012.01.004")
        if any_string_matches_any_node(['threshold-masking']) and args.threshold_algorithm == 'otsu':
            f.write("\n\n - Threshold selection algorithm - Otsu: Otsu, N. (1979). A threshold selection method from gray-level histograms. IEEE transactions on systems, man, and cybernetics, 9(1), 62-66. doi:10.1109/TSMC.1979.4310076")
        if any_string_matches_any_node(['qsmjl_laplacian-unwrapping']):
            f.write("\n\n - Unwrapping algorithm - Laplacian: Schofield MA, Zhu Y. Fast phase unwrapping algorithm for interferometric applications. Optics letters. 2003 Jul 15;28(14):1194-6. doi:10.1364/OL.28.001194")
            f.write("\n\n - Unwrapping algorithm - Laplacian: Zhou D, Liu T, Spincemaille P, Wang Y. Background field removal by solving the Laplacian boundary value problem. NMR in Biomedicine. 2014 Mar;27(3):312-9. doi:10.1002/nbm.3064")
        if any_string_matches_any_node(['mrt_laplacian-unwrapping']):
            f.write("\n\n - Unwrapping algorithm - Laplacian: Schofield MA, Zhu Y. Fast phase unwrapping algorithm for interferometric applications. Optics letters. 2003 Jul 15;28(14):1194-6. doi:10.1364/OL.28.001194")
        if any_string_matches_any_node(['romeo']):
            f.write("\n\n - Unwrapping algorithm - ROMEO: Dymerska B, Eckstein K, Bachrata B, et al. Phase unwrapping with a rapid opensource minimum spanning tree algorithm (ROMEO). Magnetic Resonance in Medicine. 2021;85(4):2294-2308. doi:10.1002/mrm.28563")
        if any_string_matches_any_node(['vsharp']):
            f.write("\n\n - Background field removal - V-SHARP: Wu B, Li W, Guidon A et al. Whole brain susceptibility mapping using compressed sensing. Magnetic resonance in medicine. 2012 Jan;67(1):137-47. doi:10.1002/mrm.23000")
        if any_string_matches_any_node(['pdf']):
            f.write("\n\n - Background field removal - PDF: Liu, T., Khalidov, I., de Rochefort et al. A novel background field removal method for MRI using projection onto dipole fields. NMR in Biomedicine. 2011 Nov;24(9):1129-36. doi:10.1002/nbm.1670")
        if any_string_matches_any_node(['nextqsm']):
            f.write("\n\n - QSM algorithm - NeXtQSM: Cognolato, F., O'Brien, K., Jin, J. et al. (2022). NeXtQSM—A complete deep learning pipeline for data-consistent Quantitative Susceptibility Mapping trained with hybrid data. Medical Image Analysis, 102700. doi:10.1016/j.media.2022.102700")
        if any_string_matches_any_node(['rts']):
            f.write("\n\n - QSM algorithm - RTS: Kames C, Wiggermann V, Rauscher A. Rapid two-step dipole inversion for susceptibility mapping with sparsity priors. Neuroimage. 2018 Feb 15;167:276-83. doi:10.1016/j.neuroimage.2017.11.018")
        if any_string_matches_any_node(['tv']):
            f.write("\n\n - QSM algorithm - TV: Bilgic B, Fan AP, Polimeni JR, Cauley SF, Bianciardi M, Adalsteinsson E, Wald LL, Setsompop K. Fast quantitative susceptibility mapping with L1-regularization and automatic parameter selection. Magnetic resonance in medicine. 2014 Nov;72(5):1444-59")
        if any_string_matches_any_node(['tgv']):
            f.write("\n\n - QSM algorithm - TGV: Langkammer C, Bredies K, Poser BA, et al. Fast quantitative susceptibility mapping using 3D EPI and total generalized variation. NeuroImage. 2015;111:622-630. doi:10.1016/j.neuroimage.2015.02.041")
        if any_string_matches_any_node(['qsmjl']):
            f.write("\n\n - Julia package - QSM.jl: kamesy. GitHub; 2022. https://github.com/kamesy/QSM.jl")
        if any_string_matches_any_node(['mrt']):
            f.write("\n\n - Julia package - MriResearchTools: Eckstein K. korbinian90/MriResearchTools.jl. GitHub; 2022. https://github.com/korbinian90/MriResearchTools.jl")
        if any_string_matches_any_node(['nibabel']):
            f.write("\n\n - Python package - nibabel: Brett M, Markiewicz CJ, Hanke M, et al. nipy/nibabel. GitHub; 2019. https://github.com/nipy/nibabel")
        if any_string_matches_any_node(['scipy']):
            f.write("\n\n - Python package - scipy: Virtanen P, Gommers R, Oliphant TE, et al. SciPy 1.0: fundamental algorithms for scientific computing in Python. Nat Methods. 2020;17(3):261-272. doi:10.1038/s41592-019-0686-2")
        if any_string_matches_any_node(['numpy']):
            f.write("\n\n - Python package - numpy: Harris CR, Millman KJ, van der Walt SJ, et al. Array programming with NumPy. Nature. 2020;585(7825):357-362. doi:10.1038/s41586-020-2649-2")
        f.write("\n\n - Python package - Nipype: Gorgolewski K, Burns C, Madison C, et al. Nipype: A Flexible, Lightweight and Extensible Neuroimaging Data Processing Framework in Python. Frontiers in Neuroinformatics. 2011;5. Accessed April 20, 2022. doi:10.3389/fninf.2011.00013")
        f.write("\n\n")

def script_exit(error_code=0):
    show_warning_summary(logger)
    logger.log(LogLevel.INFO.value, 'Finished')
    exit(error_code)

if __name__ == "__main__":
    # create initial logger
    logger = create_logger(name='pre')
    logger.log(LogLevel.INFO.value, f"Running QSMxT {get_qsmxt_version()}")

    # parse explicit arguments
    logger.log(LogLevel.INFO.value, f"Parsing arguments...")
    args, run_command = parse_args(sys.argv[1:], return_run_command=True)

    # list premade pipelines and exit if needed
    if args.list_premades:
        print_qsm_premades(args.pipeline_file)
        script_exit()

    # create output directory if it doesn't exist
    os.makedirs(args.output_dir, exist_ok=True)
    
    # overwrite logger with one that logs to file
    logpath = os.path.join(args.output_dir, f"qsmxt_log.log")
    logger.log(LogLevel.INFO.value, f"Starting log file: {logpath}")
    logger = create_logger(
        name='main',
        logpath=logpath
    )
    logger.log(LogLevel.INFO.value, f"Running QSMxT {get_qsmxt_version()}")
    logger.log(LogLevel.INFO.value, f"Python interpreter: {sys.executable}")
    logger.log(LogLevel.INFO.value, f"Command: {run_command}")

    # print diff if needed
    diff = get_diff()
    if diff:
        logger.log(LogLevel.WARNING.value, f"QSMxT's working directory is not clean! Writing git diff to {os.path.join(args.output_dir, 'diff.txt')}...")
        diff_file = open(os.path.join(args.output_dir, "diff.txt"), "w")
        diff_file.write(diff)
        diff_file.close()
    
    # process args and make any necessary corrections
    args = process_args(args)

    # write command to file
    with open(os.path.join(args.output_dir, 'command.txt'), 'w') as command_file:
        command_file.write(f"{run_command}\n")

    # write settings to file
    with open(os.path.join(args.output_dir, 'settings.json'), 'w') as settings_file:
        json.dump({ "pipeline" : vars(args) }, settings_file)
    
    # set environment variables
    set_env_variables(args)
    
    # build workflow
    wf = init_workflow(args)
    
    # write citations to file
    write_citations(wf)

    config.update_config({'logging': { 'log_directory': args.output_dir, 'log_to_file': True }})
    logging.update_logging(config)

    # run workflow
    if not args.dry:
        if args.slurm[0] is not None:
            wf.run(
                plugin='SLURM',
                plugin_args=gen_plugin_args(slurm_account=args.slurm[0], slurm_partition=args.slurm[1])
            )
        if args.pbs:
            wf.run(
                plugin='PBSGraph',
                plugin_args=gen_plugin_args(pbs_account=args.pbs)
            )
        else:
            logger.log(LogLevel.INFO.value, f"Running using MultiProc plugin with n_procs={args.n_procs}")
            plugin_args = { 'n_procs' : args.n_procs }
            if os.environ.get("PBS_JOBID"):
                jobid = os.environ.get("PBS_JOBID").split(".")[0]
                plugin_args['memory_gb'] = float(sys_cmd(f"qstat -f {jobid} | grep Resource_List.mem", print_output=False, print_command=False).split(" = ")[1].split("gb")[0])
            wf.run(
                plugin='MultiProc',
                plugin_args=plugin_args
            )

    script_exit()

