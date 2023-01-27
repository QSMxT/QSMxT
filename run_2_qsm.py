#!/usr/bin/env python3

import sys
import os
import glob
import copy
import datetime
import argparse

from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import DataSink
from nipype.pipeline.engine import Workflow, Node, MapNode
from nipype import config, logging
from scripts.qsmxt_functions import get_qsmxt_version, get_qsmxt_dir, get_diff
from scripts.sys_cmd import sys_cmd
from scripts.logger import LogLevel, make_logger, show_warning_summary, get_logger

from interfaces import nipype_interface_romeo as romeo
from interfaces import nipype_interface_scalephase as scalephase
from interfaces import nipype_interface_makehomogeneous as makehomogeneous
from interfaces import nipype_interface_json as json
from interfaces import nipype_interface_axialsampling as sampling
from interfaces import nipype_interface_addtojson as addtojson
from interfaces import nipype_interface_twopass as twopass
from interfaces import nipype_interface_nonzeroaverage as nonzeroaverage

from workflows.qsm import qsm_workflow
from workflows.masking import masking_workflow


def init_workflow(args):
    logger = get_logger()
    subjects = [
        os.path.split(path)[1]
        for path in glob.glob(os.path.join(args.bids_dir, args.subject_pattern))
        if not args.subjects or os.path.split(path)[1] in args.subjects
    ]
    if not subjects:
        logger.log(LogLevel.ERROR.value, f"No subjects found in {os.path.join(args.bids_dir, args.session_pattern)}")
        exit(1)
    wf = Workflow("workflow_qsm", base_dir=args.output_dir)
    wf.add_nodes([
        node for node in
        [init_subject_workflow(args, subject) for subject in subjects]
        if node
    ])
    return wf

def init_subject_workflow(
    args, subject
):
    logger = get_logger()
    sessions = [
        os.path.split(path)[1]
        for path in glob.glob(os.path.join(args.bids_dir, subject, args.session_pattern))
        if not args.sessions or os.path.split(path)[1] in args.sessions
    ]
    if not sessions:
        logger.log(LogLevel.ERROR.value, f"No sessions found in: {os.path.join(args.bids_dir, subject, args.session_pattern)}")
        exit(1)
    wf = Workflow(subject, base_dir=os.path.join(args.output_dir, "workflow_qsm"))
    wf.add_nodes([
        node for node in
        [init_session_workflow(args, subject, session) for session in sessions]
        if node
    ])
    return wf

def init_session_workflow(args, subject, session):
    logger = get_logger()
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
    
    wf = Workflow(session, base_dir=os.path.join(args.output_dir, "workflow_qsm", subject, session))
    wf.add_nodes([
        node for node in
        [init_run_workflow(copy.deepcopy(args), subject, session, run) for run in runs]
        if node
    ])
    return wf

def init_run_workflow(run_args, subject, session, run):
    logger = get_logger()
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
        
    
    # create nipype workflow for this run
    wf = Workflow(run, base_dir=os.path.join(run_args.output_dir, "workflow_qsm", subject, session, run))

    # datasink
    n_outputs = Node(
        interface=DataSink(base_directory=run_args.output_dir),
        name='nipype_datasink'
    )

    # create json header for this run
    json_dict = {
        "QSMxT version" : get_qsmxt_version(),
        "Run command" : str.join(" ", sys.argv),
        "Python interpreter" : sys.executable,
        "Inhomogeneity correction" : run_args.inhomogeneity_correction,
        "QSM algorithm" : f"{run_args.qsm_algorithm}",
        "Masking algorithm" : (f"{run_args.masking_algorithm}" + (f" plus BET" if run_args.add_bet else "")) if not mask_files else ("Predefined (one mask)" if len(mask_files) == 1 else "Predefined (multi-echo mask)"),
        "Two-pass algorithm" : "on" if run_args.two_pass else "off"
    }
    if run_args.qsm_algorithm not in ['tgv']: json_dict["Unwrapping algorithm"] = run_args.unwrapping_algorithm
    if run_args.qsm_algorithm not in ['tgv']: json_dict["BF removal algorithm"] = run_args.bf_algorithm
    n_json = Node(
        interface=json.JsonInterface(
            in_dict=json_dict,
            out_file=f"{subject}_{session}_{run}_qsmxt-header.json"
        ),
        name="json_createheader"
        # inputs : 'in_dict'
        # outputs: 'out_file'
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
        b0_strength = data['MagneticFieldStrength']
        json_file.close()
        return b0_strength
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
            output_names=['b0_strength'],
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
        interface=scalephase.ScalePhaseInterface(),
        iterfield=['phase'],
        name='nibabel_numpy_scale-phase'
        # outputs : 'out_file'
    )
    wf.connect([
        (n_inputs, mn_phase_scaled, [('phase', 'phase')])
    ])    

    # reorient to canonical
    def as_closest_canonical(phase, magnitude=None, mask=None):
        import os
        import nibabel as nib
        out_phase = os.path.abspath(f"{os.path.split(phase)[-1].split('.')[0]}_canonical.nii")
        out_mag = os.path.abspath(f"{os.path.split(magnitude)[-1].split('.')[0]}_canonical.nii") if magnitude else None
        out_mask = os.path.abspath(f"{os.path.split(mask)[-1].split('.')[0]}_canonical.nii") if mask else None
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
    if magnitude_files:
        mn_resample_inputs = MapNode(
            interface=sampling.AxialSamplingInterface(
                obliquity_threshold=run_args.obliquity_threshold
            ),
            iterfield=['magnitude', 'phase', 'mask'] if mask_files else ['magnitude', 'phase'],
            name='nibabel_numpy_nilearn_axial-resampling'
        )
        wf.connect([
            (mn_inputs_canonical, mn_resample_inputs, [('magnitude', 'magnitude')]),
            (mn_inputs_canonical, mn_resample_inputs, [('phase', 'phase')])
        ])
        if mask_files:
            wf.connect([
                (mn_inputs_canonical, mn_resample_inputs, [('mask', 'mask')])
            ])

    # combine phase data if necessary
    n_inputs_combine = Node(
        interface=IdentityInterface(
            fields=['phase', 'phase_unwrapped', 'frequency', 'mask', 'TE', 'magnitude']
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
            (mn_resample_inputs, n_romeo_combine, [('phase', 'phase'), ('magnitude', 'magnitude')]),
            (n_romeo_combine, n_inputs_combine, [('frequency', 'frequency'), ('phase_wrapped', 'phase'), ('phase_unwrapped', 'phase_unwrapped'), ('magnitude', 'magnitude'), ('mask', 'mask'), ('TE', 'TE')])
        ])
        if mask_files: wf.connect([(mn_resample_inputs, n_romeo_combine, [('out_mask', 'mask')])])
    else:
        wf.connect([
            (mn_resample_inputs, n_inputs_combine, [('phase', 'phase'), ('magnitude', 'magnitude'), ('mask', 'mask')]),
            (mn_json_params, n_inputs_combine, [('TE', 'TE')])
        ])

    # run homogeneity filter if necessary
    if run_args.inhomogeneity_correction:
        mn_inhomogeneity_correction = MapNode(
            interface=makehomogeneous.MakeHomogeneousInterface(),
            iterfield=['magnitude'],
            name='mrt_correct-inhomogeneity'
        )
        wf.connect([
            (n_inputs_combine, mn_inhomogeneity_correction, [('magnitude', 'magnitude')])
        ])

    # === MASKING ===
    wf_masking = masking_workflow(run_args, mask_files, len(magnitude_files) > 0, fill_masks=True, add_bet=run_args.add_bet, name="mask", index=0)

    if magnitude_files:
        wf.connect([
            (n_inputs_combine, wf_masking, [('phase', 'masking_inputs.phase')])
        ])
        if mask_files:
            wf.connect([
                (n_inputs_combine, wf_masking, [('mask', 'masking_inputs.mask')])
            ])
        if run_args.inhomogeneity_correction:
            wf.connect([
                (mn_inhomogeneity_correction, wf_masking, [('magnitude_corrected', 'masking_inputs.magnitude')])
            ])
        else:
            wf.connect([
                (n_inputs_combine, wf_masking, [('magnitude', 'masking_inputs.magnitude')])
            ])
    else:
        wf.connect([
            (n_inputs_combine, wf_masking, [('phase', 'masking_inputs.phase')])
        ])
        if mask_files:
            wf.connect([
                (n_inputs_combine, wf_masking, [('mask', 'masking_inputs.mask')])
            ])
    
    wf.connect([
        (wf_masking, n_outputs, [('masking_outputs.mask', 'mask')])
    ])

    # add threshold to json output
    if run_args.masking_algorithm == 'threshold':
        n_addtojson = Node(
            interface=addtojson.AddToJsonInterface(
                in_key = "Masking threshold"
            ),
            name="json_add-threshold"
        )
        wf.connect([
            (n_json, n_addtojson, [('out_file', 'in_file')]),
            (wf_masking, n_addtojson, [('masking_outputs.threshold', 'in_arr_value')])
        ])
        n_json = n_addtojson
    wf.connect([
        (n_json, n_outputs, [('out_file', 'qsm_headers')])
    ])

    # === QSM ===
    wf_qsm = qsm_workflow(run_args, "qsm")

    wf.connect([
        (n_inputs_combine, wf_qsm, [('phase', 'qsm_inputs.phase')]),
        (n_inputs_combine, wf_qsm, [('phase_unwrapped', 'qsm_inputs.phase_unwrapped')]),
        (n_inputs_combine, wf_qsm, [('frequency', 'qsm_inputs.frequency')]),
        (n_inputs_combine, wf_qsm, [('magnitude', 'qsm_inputs.magnitude')]),
        (wf_masking, wf_qsm, [('masking_outputs.mask', 'qsm_inputs.mask')]),
        (n_inputs_combine, wf_qsm, [('TE', 'qsm_inputs.TE')]),
        (n_json_params, wf_qsm, [('b0_strength', 'qsm_inputs.b0_strength')]),
        (n_nii_params, wf_qsm, [('vsz', 'qsm_inputs.vsz')])
    ])
    wf_qsm.get_node('qsm_inputs').inputs.b0_direction = "(0,0,1)"
    
    n_qsm_average = Node(
        interface=nonzeroaverage.NonzeroAverageInterface(),
        name="nibabel_numpy_qsm-average"
    )
    wf.connect([
        (wf_qsm, n_qsm_average, [('qsm_outputs.qsm', 'in_files')]),
    ])
    wf.connect([
        (n_qsm_average, n_outputs, [('out_file', 'qsm_final' if not run_args.two_pass else 'qsm_filled')])
    ])

    # two-pass algorithm
    if run_args.two_pass:
        wf_masking_intermediate = masking_workflow(run_args, mask_files, len(magnitude_files) > 0, fill_masks=False, add_bet=False, name="mask-intermediate", index=1)
        wf.connect([
            (wf_masking, wf_masking_intermediate, [('masking_inputs.phase', 'masking_inputs.phase')]),
            (wf_masking, wf_masking_intermediate, [('masking_inputs.mask', 'masking_inputs.mask')]),
            (wf_masking, wf_masking_intermediate, [('masking_inputs.magnitude', 'masking_inputs.magnitude')])
        ])

        wf_qsm_intermediate = qsm_workflow(run_args, "qsm-intermediate")
        wf.connect([
            (n_inputs_combine, wf_qsm_intermediate, [('phase', 'qsm_inputs.phase')]),
            (n_inputs_combine, wf_qsm_intermediate, [('phase_unwrapped', 'qsm_inputs.phase_unwrapped')]),
            (n_inputs_combine, wf_qsm_intermediate, [('frequency', 'qsm_inputs.frequency')]),
            (n_inputs_combine, wf_qsm_intermediate, [('magnitude', 'qsm_inputs.magnitude')]),
            (wf_masking_intermediate, wf_qsm_intermediate, [('masking_outputs.mask', 'qsm_inputs.mask')]),
            (n_inputs_combine, wf_qsm_intermediate, [('TE', 'qsm_inputs.TE')]),
            (n_json_params, wf_qsm_intermediate, [('b0_strength', 'qsm_inputs.b0_strength')]),
            (n_nii_params, wf_qsm_intermediate, [('vsz', 'qsm_inputs.vsz')])
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
        ])
        wf.connect([
            (n_qsm_twopass_average, n_outputs, [('out_file', 'qsm_final')])
        ])
        
    
    return wf

def parse_args(args):
    parser = argparse.ArgumentParser(
        description="QSMxT: QSM Reconstruction Pipeline",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        'bids_dir',
        type=os.path.abspath,
        help='Input data folder generated using run_1_dicomConvert.py. You can also use a ' +
             'previously existing BIDS folder. In this case, ensure that the --subject_pattern, '+
             '--session_pattern, --magnitude_pattern and --phase_pattern are correct for your data.'
    )

    parser.add_argument(
        'output_dir',
        type=os.path.abspath,
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
        dest='num_echoes',
        default=None,
        type=int,
        help='The number of echoes to process; by default all echoes are processed.'
    )

    parser.add_argument(
        '--obliquity_threshold',
        type=int,
        default=10,
        help="TODO" #TODO
    )

    parser.add_argument(
        '--combine_phase',
        action='store_true'
    )

    parser.add_argument(
        '--qsm_algorithm',
        default='rts',
        choices=['tgv', 'nextqsm', 'rts'],
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
        default=1000,
        help='Number of iterations used by tgv.'
    )

    parser.add_argument(
        '--tgv_alphas',
        type=float,
        default=[0.0015, 0.0005],
        nargs=2,
        help='Regularisation alphas used by tgv.'
    )

    parser.add_argument(
        '--tgv_erosions',
        type=int,
        default=3,
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
        default='pdf',
        choices=['vsharp', 'pdf'],
        help='Background field correction algorithm. V-SHARP is based on doi:10.1002/mrm.23000 PDF is '+
             'based on doi:10.1002/nbm.1670.'
    )

    parser.add_argument(
        '--masking_algorithm',
        default=None,
        choices=['threshold', 'bet', 'bet-firstecho'],
        help='Masking algorithm. Threshold-based masking uses a simple binary threshold applied to the '+
             '--masking_input, followed by a hole-filling strategy determined by the --filling_algorithm. '+
             'BET masking generates a mask using the Brain Extraction Tool (BET) based on '+
             'doi:10.1002/hbm.10062 from Smith SM., with the \'bet-firstecho\' option generating only a '+
             'single BET mask based on the first echo. The default algorithm is \'threshold\' except for '+
             'when the --qsm_algorithm is set to \'nextqsm\', which will change the default to '+
             '\'bet-firstecho\'.'
    )

    parser.add_argument(
        '--masking_input',
        default='phase',
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
        default=[None],
        help='Masking threshold for when --masking_algorithm is set to threshold. Values between 0 and 1'+
             'represent a percentage of the multi-echo input range. Values greater than 1 represent an '+
             'absolute threshold value. Lower values will result in larger masks. If no threshold is '+
             'provided, the --threshold_algorithm is used to select one automatically.'
    )

    parser.add_argument(
        '--threshold_algorithm',
        default='otsu',
        choices=['otsu', 'gaussian'],
        help='Algorithm used to select a threshold for threshold-based masking if --threshold_value is '+
             'left unspecified. The gaussian method is based on doi:10.1016/j.compbiomed.2012.01.004 '+
             'from Balan AGR. et al. The otsu method is based on doi:10.1109/TSMC.1979.4310076 from Otsu '+
             'et al.'
    )

    parser.add_argument(
        '--filling_algorithm',
        default='both',
        choices=['morphological', 'smoothing', 'both'],
        help='Algorithm used to fill holes for threshold-based masking. By default, a gaussian smoothing '+
             'operation is applied first prior to a morphological hole-filling operation. Note that gaussian '+
             'smoothing may fill some unwanted regions (e.g. connecting the skull and brain tissue), whereas '+
             'morphological hole-filling alone may fail to fill desired regions if they are not fully enclosed.'
    )

    parser.add_argument(
        '--threshold_algorithm_factor',
        default=[1.7, 1.0],
        nargs='+',
        type=float,
        help='Factor to multiply the algorithmically-determined threshold by. Larger factors will create '+
             'smaller masks.'
    )

    parser.add_argument(
        '--mask_erosions',
        type=int,
        nargs='+',
        default=[3, 0],
        help='Number of erosions applied to masks prior to QSM processing steps. Note that some algorithms '+
             'may erode the mask further (e.g. V-SHARP and TGV-QSM).'
    )

    parser.add_argument(
        '--inhomogeneity_correction',
        action='store_true',
        help='Applies an inhomogeneity correction to the magnitude prior to masking based on '+
             'https://index.mirasmart.com/ISMRM2019/PDFfiles/2716.html from Eckstein et al. This option '+
             'is only relevant when the --masking_input is the magnitude.'
    )

    parser.add_argument(
        '--add_bet',
        action='store_true',
        help='Combines the chosen masking method with BET. This option is only relevant when the '+
             '--masking_algorithm is set to threshold.'
    )

    parser.add_argument(
        '--bet_fractional_intensity',
        type=float,
        default=0.5,
        help='Fractional intensity for BET masking operations.'
    )
    
    parser.add_argument(
        '--use_existing_masks',
        action='store_true',
        help='This option will use existing masks from the BIDS folder, where possible, instead of '+
             'generating new ones. The masks will be selected based on the --mask_pattern argument. '+
             'A single mask may be present (and will be applied to all echoes), or a mask for each '+
             'echo can be used. When existing masks cannot be found, the --masking_algorithm will '+
             'be used as a fallback.'
    )
    
    parser.add_argument(
        '--mask_pattern',
        default='{subject}/{session}/extra_data/*{run}*mask*nii*',
        help='Pattern used to identify mask files to be used when the --use_existing_masks option '+
             'is enabled.'
    )

    parser.add_argument(
        '--two_pass',
        choices=['on', 'off'],
        default='on',
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
        default=None,
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
        help='Enables some nipype settings for debugging.'
    )

    parser.add_argument(
        '--dry',
        action='store_true',
        help='Creates the nipype pipeline using the chosen settings, but does not execute it. Useful for '+
             'debugging purposes, or for creating a citations file.'
    )
    
    args = parser.parse_args(args)
    
    return args

def create_logger(args):
    logger = make_logger(
        logpath=os.path.join(args.output_dir, f"log_{str(datetime.datetime.now()).replace(':', '-').replace(' ', '_').replace('.', '')}.txt"),
        printlevel=LogLevel.INFO,
        writelevel=LogLevel.INFO,
        warnlevel=LogLevel.WARNING,
        errorlevel=LogLevel.ERROR
    )
    return logger

def process_args(args):
    # default masking settings for QSM algorithms
    if not args.masking_algorithm:
        if args.qsm_algorithm == 'nextqsm':
            args.masking_algorithm = 'bet-firstecho'
        else:
            args.masking_algorithm = 'threshold'

    # default unwrapping settings for QSM algorithms
    if not args.unwrapping_algorithm:
        if args.qsm_algorithm in ['nextqsm', 'rts']:
            args.unwrapping_algorithm = 'romeo'

    # default two-pass settings for QSM algorithms
    if not args.two_pass:
        if args.qsm_algorithm in ['rts', 'tgv']:
            args.two_pass = 'on'
        else:
            args.two_pass = 'off'
    args.two_pass = True if args.two_pass == 'on' else False

    # add_bet option only works with non-bet masking methods
    args.add_bet &= 'bet' not in args.masking_algorithm

    # two-pass option only works with non-bet masking methods
    args.two_pass &= 'bet' not in args.masking_algorithm
    args.single_pass = not args.two_pass

    # two-pass option does not work with nextqsm or v-sharp
    args.two_pass &= args.qsm_algorithm != 'nextqsm'
    args.two_pass &= args.bf_algorihtm != 'vsharp'

    # force masking input to magnitude if bet is the masking method
    args.masking_input = 'magnitude' if 'bet' in args.masking_algorithm else args.masking_input

    # decide on inhomogeneity correction
    args.inhomogeneity_correction &= (args.add_bet or args.masking_input == 'magnitude')
    
    # set number of concurrent processes to run depending on available resources
    if not args.n_procs:
        args.n_procs = int(os.environ["NCPUS"] if "NCPUS" in os.environ else os.cpu_count())

    # determine whether multiproc will be used
    args.multiproc = not (args.pbs or args.slurm)

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

def write_references(wf):
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
        if any_string_matches_any_node(['tgv']):
            f.write("\n\n - QSM algorithm - TGV: Langkammer C, Bredies K, Poser BA, et al. Fast quantitative susceptibility mapping using 3D EPI and total generalized variation. NeuroImage. 2015;111:622-630. doi:10.1016/j.neuroimage.2015.02.041")
        if any_string_matches_any_node(['qsmjl']):
            f.write("\n\n - Julia package - QSM.jl: kamesy. GitHub; 2022. https://github.com/kamesy/QSM.jl")
        if any_string_matches_any_node(['mrt']):
            f.write("\n\n - Julia package - MriResearchTools: Eckstein K. korbinian90/MriResearchTools.jl. GitHub; 2022. https://github.com/korbinian90/MriResearchTools.jl")
        if any_string_matches_any_node(['nibabel']):
            f.write("\n\n - Python package - Nibabel: Brett M, Markiewicz CJ, Hanke M, et al. nipy/nibabel. GitHub; 2019. https://github.com/nipy/nibabel")
        if any_string_matches_any_node(['scipy']):
            f.write("\n\n - Python package - Scipy: Virtanen P, Gommers R, Oliphant TE, et al. SciPy 1.0: fundamental algorithms for scientific computing in Python. Nat Methods. 2020;17(3):261-272. doi:10.1038/s41592-019-0686-2")
        if any_string_matches_any_node(['numpy']):
            f.write("\n\n - Python package - Numpy: Harris CR, Millman KJ, van der Walt SJ, et al. Array programming with NumPy. Nature. 2020;585(7825):357-362. doi:10.1038/s41586-020-2649-2")
        f.write("\n\n - Nipype package: Gorgolewski K, Burns C, Madison C, et al. Nipype: A Flexible, Lightweight and Extensible Neuroimaging Data Processing Framework in Python. Frontiers in Neuroinformatics. 2011;5. Accessed April 20, 2022. doi:10.3389/fninf.2011.00013")
        

        f.write("\n\n")

if __name__ == "__main__":
    # parse command-line arguments
    args = parse_args(sys.argv[1:])
    
    # create output directory if it doesn't exist
    os.makedirs(args.output_dir, exist_ok=True)
    
    # setup logger
    logger = create_logger(args)
    logger.log(LogLevel.INFO.value, f"Running QSMxT {get_qsmxt_version()}")
    logger.log(LogLevel.INFO.value, f"Command: {str.join(' ', sys.argv)}")
    logger.log(LogLevel.INFO.value, f"Python interpreter: {sys.executable}")

    # print diff if needed
    diff = get_diff()
    if diff:
        logger.log(LogLevel.WARNING.value, f"Working directory not clean! Writing diff to {os.path.join(args.output_dir, 'diff.txt')}...")
        diff_file = open("diff.txt", "w")
        diff_file.write(diff)
        diff_file.close()
    
    # process args and make any necessary corrections
    args = process_args(args)
    
    # set environment variables
    set_env_variables(args)
    
    # build workflow
    wf = init_workflow(args)
    
    # write references to file
    write_references(wf)

    config.update_config({'logging': { 'log_directory': args.output_dir, 'log_to_file': True }})
    logging.update_logging(config)

    # run workflow
    if not args.dry:
        if args.slurm:
            wf.run(
                plugin='SLURM',
                plugin_args={
                    'sbatch_args': f'--account={args.slurm} --time=00:30:00 --nodes=1 --ntasks-per-node=1 --mem=5gb'
                }
            )
        if args.pbs:
            wf.run(
                plugin='PBSGraph',
                plugin_args={
                    'qsub_args': f'-A {args.pbs} -N QSMxT -l walltime=00:30:00 -l select=1:ncpus=1:mem=5gb'
                }
            )
        else:
            plugin_args = { 'n_procs' : args.n_procs }
            if os.environ.get("PBS_JOBID"):
                jobid = os.environ.get("PBS_JOBID").split(".")[0]
                plugin_args['memory_gb'] = float(sys_cmd(f"qstat -f {jobid} | grep Resource_List.mem", print_output=False, print_command=False).split(" = ")[1].split("gb")[0])
                print(plugin_args)
            wf.run(
                plugin='MultiProc',
                plugin_args=plugin_args
            )

    show_warning_summary(logger)
    logger.log(LogLevel.INFO.value, 'Finished')

