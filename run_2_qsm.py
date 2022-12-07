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
from scripts.qsmxt_functions import get_qsmxt_version, get_qsmxt_dir
from scripts.logger import LogLevel, make_logger, show_warning_summary, get_logger

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
    n_datasink = Node(
        interface=DataSink(base_directory=run_args.output_dir),
        name='nipype_datasink'
    )

    # create json header for this run
    n_json = Node(
        interface=json.JsonInterface(
            in_dict={
                "QSMxT version" : get_qsmxt_version(),
                "Run command" : str.join(" ", sys.argv),
                "Python interpreter" : sys.executable,
                "Inhomogeneity correction" : run_args.inhomogeneity_correction,
                "QSM algorithm" : f"{run_args.qsm_algorithm}" + (f" with two-pass algorithm" if run_args.two_pass else ""),
                "Masking algorithm" : (f"{run_args.masking_algorithm}" + (f" plus BET" if run_args.add_bet else "")) if not mask_files else ("Predefined (one mask)" if len(mask_files) == 1 else "Predefined (multi-echo mask)")
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
            fields=['phase_files', 'magnitude_files', 'params_files', 'mask_files']
        ),
        name='nipype_getfiles'
    )
    n_getfiles.inputs.phase_files = phase_files
    n_getfiles.inputs.magnitude_files = magnitude_files
    n_getfiles.inputs.params_files = params_files
    if len(mask_files) == 1: mask_files = [mask_files[0] for _ in phase_files]
    n_getfiles.inputs.mask_files = mask_files

    # read echotime and field strengths from json files
    def read_json_me(in_file):
        import json
        json_file = open(in_file, 'rt')
        data = json.load(json_file)
        te = data['EchoTime']
        json_file.close()
        return te
    def read_json_se(in_files):
        import json
        json_file = open(in_files[0], 'rt')
        data = json.load(json_file)
        b0 = data['MagneticFieldStrength']
        json_file.close()
        return b0
    mn_json_params = MapNode(
        interface=Function(
            input_names=['in_file'],
            output_names=['EchoTime'],
            function=read_json_me
        ),
        iterfield=['in_file'],
        name='func_read-json-me'
    )
    wf.connect([
        (n_getfiles, mn_json_params, [('params_files', 'in_file')])
    ])
    n_json_params = Node(
        interface=Function(
            input_names=['in_files'],
            output_names=['MagneticFieldStrength'],
            function=read_json_se
        ),
        iterfield=['in_files'],
        name='func_read-json-se'
    )
    wf.connect([
        (n_getfiles, n_json_params, [('params_files', 'in_files')])
    ])

    # read voxel size 'vsz' from nifti file
    def read_nii(in_file):
        import nibabel as nib
        if isinstance(in_file, list): in_file = in_file[0]
        nii = nib.load(in_file)
        return str(nii.header.get_zooms()).replace(" ", "")
    n_nii_params = Node(
        interface=Function(
            input_names=['in_file'],
            output_names=['vsz'],
            function=read_nii
        ),
        name='nibabel_read-nii'
    )
    wf.connect([
        (n_getfiles, n_nii_params, [('phase_files', 'in_file')])
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

    # reorient to canonical
    def as_closest_canonical(phase, magnitude=None, mask=None):
        import os
        import nibabel as nib
        out_phase = os.path.abspath(f"{os.path.split(phase)[-1].split('.')[0]}_canonical.nii")
        out_mag = os.path.abspath(f"{os.path.split(magnitude)[-1].split('.')[0]}_canonical.nii") if magnitude else None
        out_mask = os.path.abspath(f"{os.path.split(mask)[-1].split('.')[0]}_canonical.nii") if mask else None
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
        (mn_phase_scaled, mn_inputs_canonical, [('out_file', 'phase')])
    ])
    if magnitude_files:
        wf.connect([
            (n_getfiles, mn_inputs_canonical, [('magnitude_files', 'magnitude')]),
        ])
    if mask_files:
        wf.connect([
            (n_getfiles, mn_inputs_canonical, [('mask_files', 'mask')]),
        ])
    
    # resample to axial
    if magnitude_files:
        mn_resample_inputs = MapNode(
            interface=sampling.AxialSamplingInterface(
                obliquity_threshold=10
            ),
            iterfield=['in_mag', 'in_pha', 'in_mask'] if mask_files else ['in_mag', 'in_pha'],
            name='nibabel_numpy_nilearn_axial-resampling'
        )
        wf.connect([
            (mn_inputs_canonical, mn_resample_inputs, [('magnitude', 'in_mag')]),
            (mn_inputs_canonical, mn_resample_inputs, [('phase', 'in_pha')])
        ])
        if mask_files:
            wf.connect([
                (mn_inputs_canonical, mn_resample_inputs, [('mask', 'in_mask')])
            ])

    # run homogeneity filter if necessary
    if run_args.inhomogeneity_correction:
        mn_inhomogeneity_correction = MapNode(
            interface=makehomogeneous.MakeHomogeneousInterface(),
            iterfield=['in_file'],
            name='mriresearchtools_correct-inhomogeneity'
            # output : out_file
        )
        wf.connect([
            (mn_resample_inputs, mn_inhomogeneity_correction, [('out_mag', 'in_file')])
        ])

    # collect inputs for masking
    mn_masking_inputs = MapNode(
        interface=IdentityInterface(
            fields=['phase_files', 'magnitude_files', 'mask_files']
        ),
        iterfield=['phase_files', 'magnitude_files', 'mask_files'],
        name='func_repeat-inputs'
    )
    if magnitude_files:
        wf.connect([
            (mn_resample_inputs, mn_masking_inputs, [('out_pha', 'phase_files')])
        ])
        if mask_files:
            wf.connect([
                (mn_resample_inputs, mn_masking_inputs, [('out_mask', 'mask_files')])
            ])
        if run_args.inhomogeneity_correction:
            wf.connect([
                (mn_inhomogeneity_correction, mn_masking_inputs, [('out_file', 'magnitude_files')])
            ])
        else:
            wf.connect([
                (mn_resample_inputs, mn_masking_inputs, [('out_mag', 'magnitude_files')])
            ])
    else:
        wf.connect([
            (mn_inputs_canonical, mn_masking_inputs, [('phase', 'phase_files')])
        ])
        if mask_files:
            wf.connect([
                (mn_inputs_canonical, mn_masking_inputs, [('mask', 'mask_files')])
            ])
    
    # masking steps
    wf_masking = masking_workflow(run_args, mn_masking_inputs, mask_files, len(magnitude_files) > 0, fill_masks=True, add_bet=run_args.add_bet, name="mask")
    wf.connect([
        (wf_masking, n_datasink, [('masking_outputs.masks', 'masks')])
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
            (wf_masking, n_addtojson, [('masking_outputs.threshold', 'in_num_value')])
        ])
        n_json = n_addtojson
    wf.connect([
        (n_json, n_datasink, [('out_file', 'qsm_headers')])
    ])

    # qsm steps
    mn_qsm_inputs = MapNode(
        interface=IdentityInterface(
            fields=['phase', 'magnitude', 'mask', 'TE', 'B0_str', 'B0_dir', 'vsz']
        ),
        iterfield=['phase', 'magnitude', 'mask', 'TE'],
        name='qsm_inputs'
    )
    wf.connect([
        (mn_masking_inputs, mn_qsm_inputs, [('phase_files', 'phase')]),
        (mn_masking_inputs, mn_qsm_inputs, [('magnitude_files', 'magnitude')]),
        (wf_masking, mn_qsm_inputs, [('masking_outputs.masks', 'mask')]),
        (mn_json_params, mn_qsm_inputs, [('EchoTime', 'TE')]),
        (n_json_params, mn_qsm_inputs, [('MagneticFieldStrength', 'B0_str')]),
        (n_nii_params, mn_qsm_inputs, [('vsz', 'vsz')])
    ])
    mn_qsm_inputs.inputs.B0_dir = "(0,0,1)"
    wf_qsm = qsm_workflow(run_args, mn_qsm_inputs, "qsm")
    n_qsm_average = Node(
        interface=nonzeroaverage.NonzeroAverageInterface(),
        name="nibabel_numpy_qsm-average"
    )
    wf.connect([
        (wf_qsm, n_qsm_average, [('qsm_outputs.qsm', 'in_files')]),
    ])
    wf.connect([
        (n_qsm_average, n_datasink, [('out_file', 'qsm_final' if not run_args.two_pass else 'qsm_filled')])
    ])

    # two-pass algorithm
    if run_args.two_pass:
        wf_masking_intermediate = masking_workflow(run_args, mn_masking_inputs, mask_files, len(magnitude_files) > 0, fill_masks=False, add_bet=False, name="mask-intermediate")
        mn_qsm_inputs_intermediate = MapNode(
            interface=IdentityInterface(
                fields=['phase', 'magnitude', 'mask', 'TE', 'B0_str', 'B0_dir', 'vsz']
            ),
            iterfield=['phase', 'magnitude', 'mask', 'TE'],
            name='qsm_inputs-intermediate'
        )
        wf.connect([
            (mn_masking_inputs, mn_qsm_inputs_intermediate, [('phase_files', 'phase')]),
            (mn_masking_inputs, mn_qsm_inputs_intermediate, [('magnitude_files', 'magnitude')]),
            (wf_masking_intermediate, mn_qsm_inputs_intermediate, [('masking_outputs.masks', 'mask')]),
            (mn_json_params, mn_qsm_inputs_intermediate, [('EchoTime', 'TE')]),
            (n_json_params, mn_qsm_inputs_intermediate, [('MagneticFieldStrength', 'B0_str')]),
            (n_nii_params, mn_qsm_inputs_intermediate, [('vsz', 'vsz')])
        ])
        mn_qsm_inputs_intermediate.inputs.B0_dir = "(0,0,1)"
        wf_qsm_intermediate = qsm_workflow(run_args, mn_qsm_inputs_intermediate, "qsm-intermediate")
        
        # two-pass combination
        mn_qsm_twopass = MapNode(
            interface=twopass.TwopassNiftiInterface(),
            name='numpy_nibabel_twopass',
            iterfield=['in_file1', 'in_file2']
        )
        wf.connect([
            (wf_qsm_intermediate, mn_qsm_twopass, [('qsm_outputs.qsm', 'in_file1')]),
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
            (n_qsm_twopass_average, n_datasink, [('out_file', 'qsm_final')])
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
        choices=['romeo', 'laplacian'],
        help="Phase unwrapping algorithm. ROMEO is based on doi:10.1002/mrm.28563 from Eckstein et al. "+
             "Laplacian is based on doi:10.1364/OL.28.001194 and doi:10.1002/nbm.3064 from Schofield MA. "+
             "et al. and Zhou D. et al., respectively. ROMEO is the default when --qsm_algorithm is set to "+
             "rts or nextqsm, and no unwrapping is applied by default when --qsm_algorithm is set to tgv."
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
        default=None,
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
        default=1.25,
        type=float,
        help='Factor to multiply the algorithmically-determined threshold by. Larger factors will create '+
             'smaller masks.'
    )

    parser.add_argument(
        '--mask_erosions',
        type=int,
        default=1,
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
        action='store_true',
        help='Runs the QSM reconstruction in a two-stage fashion to reduce artefacts; combines '+
             'the results from two QSM images reconstructed using masks that separate more reliable '+
             'and less reliable phase regions. Note that this option requires threshold-based masking, '+
             'doubles reconstruction time, and in some cases can deteriorate QSM contrast in some '+
             'regions. Applications where two-pass QSM may improve results include body imaging, '+
             'lesion imaging, and imaging of other strong susceptibility sources. This method is '+
             'based on doi:10.1002/mrm.29048 from Stewart et al.'
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
        help='Number of processes to run concurrently for MultiProc. By default, the number of available '+
             'CPUs is used.'
    )

    parser.add_argument(
        '--debug',
        action='store_true',
        help='Enables some nipype settings for debugging.'
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
    
    # add_bet option only works with non-bet masking methods
    args.add_bet &= 'bet' not in args.masking_algorithm

    # two-pass option does not work with 'bet' masking
    args.two_pass &= 'bet' not in args.masking_algorithm
    args.single_pass = not args.two_pass

    # force masking input to magnitude if bet is the masking method
    args.masking_input = 'magnitude' if 'bet' in args.masking_algorithm else args.masking_input

    # decide on inhomogeneity correction
    args.inhomogeneity_correction &= (args.add_bet or args.masking_input == 'magnitude')
    
    # set number of concurrent processes to run depending on available resources
    if not args.n_procs:
        args.n_procs = int(os.environ["NCPUS"] if "NCPUS" in os.environ else os.cpu_count())

    # set number of concurrent threads for specific programs
    args.process_threads = args.n_procs if not args.qsub_account_string else 6

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

    #os.environ["JULIA_NUM_THREADS"] = str(args.process_threads)

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
        f.write("\n\n - Stewart AW, Robinson SD, O'Brien K, et al. QSMxT: Robust masking and artifact reduction for quantitative susceptibility mapping. Magnetic Resonance in Medicine. 2022;87(3):1289-1300. doi:10.1002/mrm.29048")
        f.write("\n\n - Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT")
        f.write("\n\n - Gorgolewski K, Burns C, Madison C, et al. Nipype: A Flexible, Lightweight and Extensible Neuroimaging Data Processing Framework in Python. Frontiers in Neuroinformatics. 2011;5. Accessed April 20, 2022. doi:10.3389/fninf.2011.00013")
        
        # nextqsm
        # qsm.jl laplacian
        # qsm.jl rts
        # qsm.jl v-sharp
        if any_string_matches_any_node(['tgv']):
            f.write("\n\n - Langkammer C, Bredies K, Poser BA, et al. Fast quantitative susceptibility mapping using 3D EPI and total generalized variation. NeuroImage. 2015;111:622-630. doi:10.1016/j.neuroimage.2015.02.041")
        if any_string_matches_any_node(['threshold-masking']) and args.threshold_value is None:
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
    
    # process args and make any necessary corrections
    args = process_args(args)
    
    # set environment variables
    set_env_variables(args)
    
    # build workflow
    wf = init_workflow(args)
    
    # write references to file
    write_references(wf)

    # run workflow
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

