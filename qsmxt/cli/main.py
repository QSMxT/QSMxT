#!/usr/bin/env python3

import sys
import os
import psutil
import glob
import copy
import argparse
import json
import re
import datetime

from nipype import config, logging
from nipype.pipeline.engine import Workflow, Node
from nipype.interfaces.utility import Merge

from qsmxt.scripts.qsmxt_functions import get_qsmxt_version, get_qsmxt_dir, get_diff, print_qsm_premades, gen_plugin_args
from qsmxt.scripts.sys_cmd import sys_cmd
from qsmxt.scripts.logger import LogLevel, make_logger, show_warning_summary
from qsmxt.scripts.user_input import get_option, get_string, get_num, get_nums

from qsmxt.workflows.qsm import init_qsm_workflow
from qsmxt.workflows.template import init_template_workflow

def init_workflow(args):
    logger = make_logger('main')
    subject_paths = [
        path for path in sorted(glob.glob(os.path.join(args.bids_dir, "sub*")))
    ]

    if not subject_paths:
        logger.log(LogLevel.ERROR.value, f"No subjects found in {os.path.join(args.bids_dir, 'sub*')}!")
        script_exit(1, logger=logger)
    
    subject_ids = [os.path.split(subject_path)[1] for subject_path in subject_paths if not args.subjects or os.path.split(subject_path)[1] in args.subjects]

    if not subject_ids:
        logger.log(LogLevel.ERROR.value, f"Requested subjects {args.subjects} not found in {os.path.join(args.bids_dir, 'sub*')}!")
        script_exit(1, logger=logger)

    wf = Workflow(f'qsmxt-workflow', base_dir=args.workflow_dir)
    wf.add_nodes([
        node for node in
        [init_subject_workflow(args, subject) for subject in subject_ids]
        if node
    ])

    if args.do_qsm and args.do_template:
        qsm_output_nodes = []
        for node in wf._get_all_nodes():
            if 'qsmxt_outputs' in node._name:
                qsm_output_nodes.append(node)
        
        # A node to merge all the qsm files
        n_merge_qsm = Node(Merge(len(qsm_output_nodes)), name="merge_qsm")

        # Connect all the qsm_output_nodes to the merge_node
        for i, node in enumerate(qsm_output_nodes):
            wf.connect(node, 'qsm', n_merge_qsm, f'in{i+1}')

        template_wf = init_template_workflow(args)
        wf.connect([
            (n_merge_qsm, template_wf, [('out', 'template_inputs.qsm')]),
        ])

    return wf

def init_subject_workflow(args, subject):
    logger = make_logger('main')
    subject_path = os.path.join(args.bids_dir, subject)

    session_paths = [
        path for path in sorted(glob.glob(os.path.join(subject_path, "ses*")))
    ]

    session_ids = [os.path.split(path)[1] for path in session_paths if not args.sessions or os.path.split(path)[1] in args.sessions]

    if not session_ids and not glob.glob(os.path.join(subject_path, "anat", "*.*")):
        if args.sessions:
            logger.log(LogLevel.WARNING.value, f"No imaging data or sessions matching {args.sessions} found in {subject_path}")
        else:
            logger.log(LogLevel.WARNING.value, f"No imaging data found in {subject_path}")
        return None

    wf = Workflow(name=subject, base_dir=os.path.join(args.workflow_dir))

    for session in session_ids or [None]:
        session_wf = init_session_workflow(args, subject, session)
        if session_wf:
            wf.add_nodes([session_wf])

    return wf

def init_session_workflow(args, subject, session=None):
    logger = make_logger('main')
    base = os.path.join(subject, session) if session else subject
    wf = Workflow(session or "default",
                  base_dir=os.path.join(args.workflow_dir, "workflow", base))

    file_pattern = os.path.join(args.bids_dir, base, "anat", f"sub-*_part-*.nii*")
    files = sorted(glob.glob(file_pattern))

    groups = {}
    for path in files:
        acq = re.search("_acq-([a-zA-Z0-9-]+)_", path).group(1) if '_acq-' in path else None
        rec = re.search("_rec-([a-zA-Z0-9-]+)_", path).group(1) if '_rec-' in path else None
        inv = re.search("_inv-([a-zA-Z0-9]+)_", path).group(1) if '_inv-' in path else None
        run = re.search("_run-([a-zA-Z0-9]+)_", path).group(1) if '_run-' in path else None

        suffix = os.path.splitext(os.path.split(path)[1])[0].split('_')[-1].split('.')[0]

        if args.recs:
            if not any(f"_{r}_" in os.path.split(path)[1] for r in args.recs):
                continue

        if args.invs:
            if not any(f"_{i}_" in os.path.split(path)[1] for i in args.invs):
                continue

        if args.acqs:
            if not any(f"_{a}_" in os.path.split(path)[1] for a in args.acqs):
                continue

        if args.runs:
            if not any(f"_{r}_" in os.path.split(path)[1] for r in args.runs):
                continue

        key = (acq, rec, inv, suffix)
        if key not in groups:
            groups[key] = set()
        if run:
            groups[key].add(run)

    run_details = {}
    for key, runs in groups.items():
        if any(r is not None for r in runs):
            run_details[key] = sorted(list(runs))
        else:
            run_details[key] = [None]

    if any([args.do_qsm, args.do_segmentation, args.do_t2starmap,
            args.do_r2starmap, args.do_swi, args.do_analysis]):
        workflows = [
            init_qsm_workflow(copy.deepcopy(args), subject, session, acq, rec, inv, suffix, run)
            for (acq, rec, inv, suffix), runs in run_details.items()
            for run in (runs if runs is not None else [None])
        ]
        wf.add_nodes([w for w in workflows if w])

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
        help='Input BIDS directory. Can be generated using dicom-convert or nifti-convert.'
    )

    parser.add_argument(
        'output_dir',
        nargs='?',
        default=None,
        type=os.path.abspath,
        help='Input output directory. By default, the output will be integrated into the BIDS directory as a BIDS derivative.'
    )

    parser.add_argument(
        '--workflow-dir',
        dest='workflow_dir',
        default=None,
        type=os.path.abspath,
        help='Directory for nipype workflow intermediate files. By default, this is set to output_dir/workflow or bids_dir/derivatives/qsmxt/.../workflow if no output_dir is specified.'
    )

    parser.add_argument(
        '--do_qsm',
        nargs='?',
        type=argparse_bool,
        const=True,
        default=None,
        help="Whether or not to run the QSM pipeline."
    )

    parser.add_argument(
        '--do_segmentation',
        nargs='?',
        type=argparse_bool,
        const=True,
        default=None,
        help="Whether or not to run the segmentation pipeline."
    )

    parser.add_argument(
        '--do_analysis',
        nargs='?',
        type=argparse_bool,
        const=True,
        default=None,
        help="Whether or not to run the template-building pipeline."
    )

    parser.add_argument(
        '--do_template',
        nargs='?',
        type=argparse_bool,
        const=True,
        default=None,
        help="Whether or not to run the template-building pipeline."
    )

    parser.add_argument(
        '--do_t2starmap',
        nargs='?',
        type=argparse_bool,
        const=True,
        default=None,
        help='Enables generation of T2* map.'
    )

    parser.add_argument(
        '--do_r2starmap',
        nargs='?',
        type=argparse_bool,
        const=True,
        default=None,
        help='Enables generation of R2* map.'
    )

    parser.add_argument(
        '--do_swi',
        nargs='?',
        type=argparse_bool,
        const=True,
        default=None,
        help='Enables generation SWI via CLEAR-SWI.'
    )

    parser.add_argument(
        '--labels_file',
        default=None,
        help='Optional labels CSV file to include named fields in analysis outputs. The CSV should contain '+
             'segmentation numbers in the first column and ROI names in the second. The aseg_labels.csv '+
             'file contains labels for the aseg atlas used in the segmentation pipeline.'
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
        help='List of BIDS runs to process (e.g. \'run-1\'); by default all runs are processed.'
    )

    parser.add_argument(
        '--recs',
        default=None,
        nargs='*',
        help='List of BIDS reconstructions to process (e.g. \'rec-1\'); by default all reconstructions are processed.'
    )

    parser.add_argument(
        '--invs',
        default=None,
        nargs='*',
        help='List of BIDS inversions to process (e.g. \'inv-1\'); by default all inversions are processed.'
    )

    parser.add_argument(
        '--acqs',
        default=None,
        nargs='*',
        help='List of BIDS acqs to process (e.g. \'acq-qsm\'); by default all runs are processed.'
    )

    parser.add_argument(
        '--num_echoes',
        dest='num_echoes',
        default=None,
        type=int,
        help='The number of echoes to process; by default all echoes are processed.'
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

    def is_valid_reference(value):
        """Custom validation for the referencing method."""
        if value.lower() in ["mean", "none"]:
            return value
        try:
            seg_id = int(value)
            if seg_id < 0:
                raise argparse.ArgumentTypeError(f"Invalid segmentation ID: {value}")
            return seg_id
        except ValueError:
            raise argparse.ArgumentTypeError(f"Invalid reference value: {value}. Expected 'mean', 'none', or a segmentation ID.")

    parser.add_argument(
        "--qsm_reference",
        type=is_valid_reference,
        default=None,
        nargs='+',
        help="Referencing method for QSM. Options are: 'mean', or a segmentation ID (integer). Default is no referencing."
    )

    parser.add_argument(
        '--tgv_iterations',
        type=int,
        default=None,
        help='Number of iterations used by tgv. Used only when --qsm_algorithm is set to tgv.'
    )

    parser.add_argument(
        '--tgv_alphas',
        type=float,
        default=None,
        nargs=2,
        help='Regularisation alphas used by tgv. Used only when --qsm_algorithm is set to tgv.'
    )

    parser.add_argument(
        '--tgv_erosions',
        type=int,
        default=None,
        help='Number of erosions applied by tgv. Used only when --qsm_algorithm is set to tgv.'
    )

    parser.add_argument(
        '--rts_tol',
        type=float,
        default=None,
        help='Stopping tolerance for RTS convergence (default: 1e-4). Lower values increase precision '+
            'but may slow convergence. Used only when --qsm_algorithm is set to rts.'
    )

    parser.add_argument(
        '--rts_delta',
        type=float,
        default=None,
        help='Threshold for ill-conditioned k-space region (default: 0.15). Controls which k-space '+
            'regions are considered ill-conditioned. Used only when --qsm_algorithm is set to rts.'
    )

    parser.add_argument(
        '--rts_mu',
        type=float,
        default=None,
        help='Mu regularization parameter for TV minimization (default: 1e5). Controls the strength '+
            'of total variation regularization. Used only when --qsm_algorithm is set to rts.'
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
        '--masking_algorithm',
        default=None,
        choices=['threshold', 'bet'],
        help='Masking algorithm. Threshold-based masking uses a simple binary threshold applied to the '+
            '--masking_input, followed by a hole-filling strategy determined by the --filling_algorithm. '+
            'BET masking generates a mask using the Brain Extraction Tool (BET) based on '+
            'doi:10.1002/hbm.10062 from Smith SM. The default algorithm is \'threshold\'.'
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
        '--use_existing_qsms',
        nargs='?',
        type=argparse_bool,
        const=True,
        default=None,
        help='Instead of generating new QSMs for each subject, this option will prioritize using existing '+
            'QSM images from the BIDS folder in the --existing_qsm_pipeline derivatives directory. When existing '+
            'QSMs cannot be found, the QSM will be generated using the selected settings. '+
            'Valid paths fit '+
            'BIDS_DIR/derivatives/EXISTING_QSM_PIPELINE/sub-<SUBJECT_ID>/[ses-<SESSION_ID>]/anat/sub-<SUBJECT_ID>[_ses-<SESSION_ID>]*_Chimap.nii'
    )

    parser.add_argument(
        '--existing_qsm_pipeline',
        default=None,
        help='A pattern matching the name of the software pipeline used to derive pre-existing QSM images.'
    )

    parser.add_argument(
        '--use_existing_segmentations',
        nargs='?',
        type=argparse_bool,
        const=True,
        default=None,
        help='Instead of generating new segmentations for each subject, this option will prioritize using existing '+
            'segmentations images from the BIDS folder in the --existing_segmentation_pipeline derivatives directory. When existing '+
            'segmentations cannot be found, the segmentations will be generated using FastSurfer. '+
            'Valid paths fit '+
            'BIDS_DIR/derivatives/existing_segmentation_pipeline/sub-<SUBJECT_ID>/[ses-<SESSION_ID>]/anat/sub-<SUBJECT_ID>[_ses-<SESSION_ID>]*_dseg.nii'
    )

    parser.add_argument(
        '--existing_segmentation_pipeline',
        default=None,
        help='A pattern matching the name of the software pipeline used to derive pre-existing segmentations in the QSM space.'
    )

    parser.add_argument(
        '--use_existing_masks',
        nargs='?',
        type=argparse_bool,
        const=True,
        default=None,
        help='Instead of generating new masks for each subject, this option will prioritize using existing '+
            'masks from the BIDS folder in the --existing_masks_pipeline derivatives directory. A single mask may be '+
            'present (and will be applied to all echoes), or a mask for each echo can be used. When existing '+
            'masks cannot be found, the --masking_algorithm will be used as a fallback. See '+
            'https://bids-specification.readthedocs.io/en/stable/05-derivatives/03-imaging.html#masks. '+
            'Valid paths fit '+
            'BIDS_DIR/derivatives/EXISTING_MASK_PIPELINE/sub-<SUBJECT_ID>/[ses-<SESSION_ID>]/anat/sub-<SUBJECT_ID>[_ses-<SESSION_ID>]*_mask.nii'
    )

    parser.add_argument(
        '--existing_masks_pipeline',
        default=None,
        help='A pattern matching the name of the software pipeline used to derive input masks to be used when '+
             '--use_existing_masks is enabled. Defaults to \'*\' to match any.'
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
        '--export_dicoms',
        nargs='?',
        type=argparse_bool,
        const=True,
        default=None,
        help='Exports outputs to DICOM format in addition to NIfTI.'
    )

    parser.add_argument(
        '--preserve_float',
        nargs='?',
        type=argparse_bool,
        const=True,
        default=None,
        help='Exported DICOMs will preserve quantitative float values instead of converting '
             'to int. Reduced compatibility with some DICOM viewers.'
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
        default=None,
        dest='slurm',
        help='Run the pipeline via SLURM and use the arguments as the account string and partition.'
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

    parser.add_argument(
        '--version', '-v',
        action='store_true',
        dest='version',
        default=None,
        help='Displays the QSMxT version'
    )

    logger = make_logger('pre')
    
    # parse explicit arguments ONLY
    args, unknown = parser.parse_known_args(args)

    # give error for unknown args
    if unknown:
        logger.log(LogLevel.ERROR.value, f"Unknown arguments: {unknown}")
        script_exit(1, logger=logger)

    # if listing premades or displaying the version, skip the rest
    if args.list_premades or args.version:
        if return_run_command:
            return args, str.join(' ', vars(args)), {}
        return args

    # bids and output are required
    if args.bids_dir is None:
        parser.error("bids_dir is required!")
    if args.output_dir is None:
        args.output_dir = os.path.join(args.bids_dir, 'derivatives', f"qsmxt-{datetime.datetime.now().strftime('%Y-%m-%d-%H%M%S')}")
        if args.workflow_dir is None:
            args.workflow_dir = os.path.join(args.bids_dir, 'derivatives', 'workflow')
    else:
        if args.workflow_dir is None:
            args.workflow_dir = os.path.join(args.output_dir, 'workflow')
    if not os.path.exists(args.workflow_dir):
        os.makedirs(args.workflow_dir, exist_ok=True)
    

    # Checking the combined qsm_reference values
    if args.qsm_reference is not None:
        if not ((len(args.qsm_reference) == 1 and args.qsm_reference[0] in ['mean', 'none']) or all(isinstance(x, int) for x in args.qsm_reference)):
            parser.error("--qsm_reference must be either 'mean', 'none', or a series of one or more integers")

    # get explicitly set arguments
    explicit_args = {}
    for k in args.__dict__:
        if args.__dict__[k] is not None:
            explicit_args[k] = args.__dict__[k]

    # load previous explicit arguments if they exist already
    using_json_settings = False
    if os.path.exists(os.path.join(args.output_dir, 'settings.json')):
        if len(explicit_args.keys()) == 1 and all(x in explicit_args.keys() for x in ['output_dir']):
            using_json_settings = True
        if len(explicit_args.keys()) == 2 and all(x in explicit_args.keys() for x in ['bids_dir', 'output_dir']):
            print(f"Previous QSMxT settings detected in {args.output_dir}!")
            using_json_settings = 'yes' == get_option(
                prompt=f"Load previous settings? [default: 'yes']: ",
                options=['yes', 'no'],
                default='yes'
            )
        if len(explicit_args.keys()) == 3 and all(x in explicit_args.keys() for x in ['bids_dir', 'output_dir', 'auto_yes']):
            using_json_settings = True
        if using_json_settings:
            logger.log(LogLevel.INFO.value, "Loading previous QSMxT settings...")
            with open(os.path.join(args.output_dir, 'settings.json'), 'r') as settings_file:
                json_settings = json.load(settings_file)['pipeline']
            keys = set(vars(args)) & set(json_settings)
            for key in keys:
                if key == 'auto_yes': continue
                explicit_args[key] = json_settings[key]

    # get implicit args based on usual defaults
    pipeline_file = f"{os.path.join(get_qsmxt_dir(), 'qsm_pipelines.json')}"
    with open(pipeline_file, "r") as json_file:
        premades = json.load(json_file)
    implicit_args = premades['default']
    implicit_args['premade'] = 'default'
    
    # get custom pipelines from the user
    if args.pipeline_file:
        with open(args.pipeline_file, "r") as json_file:
            user_premades = json.load(json_file)
        premades.update(user_premades)

    # update implicit arguments based on the explicitly selected premade
    if 'premade' in explicit_args.keys():
        if explicit_args['premade'] in premades:
            for key in list(premades[explicit_args['premade']].keys()):
                value = premades[explicit_args['premade']][key]
                if key not in explicit_args:
                    implicit_args[key] = value
                    if value is None:
                        del implicit_args[key]
        else:
            logger.log(LogLevel.ERROR.value, f"Chosen premade pipeline '{explicit_args['premade']}' not found!")
            if args.auto_yes: script_exit(1, logger=logger)
            del explicit_args['premade']
    elif 'premade' in implicit_args.keys():
        if implicit_args['premade'] in premades:
            for key in list(premades[implicit_args['premade']].keys()):
                value = premades[implicit_args['premade']][key]
                implicit_args[key] = value
                if value is None:
                    del implicit_args[key]
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

    # update args using final args
    keys = set(vars(args)) & set(final_args)
    for key in keys:
        vars(args)[key] = final_args[key]

    # process arguments to fix anything invalid
    args = process_args(args)
    final_args = vars(args).copy()

    # remove any unnecessary explicit args
    for key, value in implicit_args.items():
        if key in explicit_args and explicit_args[key] == value:
            del explicit_args[key]

    # create final args
    final_args = implicit_args.copy()
    for key, value in explicit_args.items():
        final_args[key] = value

    # update args using final args
    keys = set(vars(args)) & set(final_args)
    for key in keys:
        vars(args)[key] = final_args[key]

    # get adjustments from the user
    if not final_args['auto_yes']:
        final_args2, implicit_args = get_interactive_args(final_args.copy(), explicit_args, implicit_args, premades, using_json_settings)
        for key, val in final_args2.items():
            if key not in implicit_args or implicit_args[key] != val:
                explicit_args[key] = val # may be unnecessary!
            final_args[key] = val

    # remove any unnecessary explicit args
    for key, value in implicit_args.items():
        if key in explicit_args and explicit_args[key] == value:
            del explicit_args[key]

    # create final args
    final_args = implicit_args.copy()
    for key, value in explicit_args.items():
        final_args[key] = value

    # update args using final args
    keys = set(vars(args)) & set(final_args)
    for key in keys:
        vars(args)[key] = final_args[key]

    # compute the minimum run command to re-execute the built pipeline non-interactively
    if return_run_command:
        run_command = f"qsmxt {explicit_args['bids_dir']}"
        if 'premade' in explicit_args and explicit_args['premade'] != 'default':
            run_command += f" --premade '{explicit_args['premade']}'"
        for key, value in explicit_args.items():
            if key in ['bids_dir', 'output_dir', 'workflow_dir', 'auto_yes', 'premade', 'multiproc', 'mem_avail', 'n_procs']: continue
            if key == 'do_qsm' and value == True and all(x not in explicit_args.keys() for x in ['do_swi', 'do_r2starmap', 'do_t2starmap', 'do_segmentation', 'do_analysis']):
                continue
            if key == 'do_qsm' and value == False and any(x in explicit_args.keys() for x in ['do_swi', 'do_r2starmap', 'do_t2starmap', 'do_segmentation', 'do_analysis']):
                continue
            elif value == True: run_command += f' --{key}'
            elif value == False: run_command += f' --{key} off'
            elif isinstance(value, str): run_command += f" --{key} '{value}'"
            elif isinstance(value, (int, float)) and value != False: run_command += f" --{key} {value}"
            elif isinstance(value, list):
                run_command += f" --{key}"
                for val in value:
                    run_command += f" {val}"
        run_command += ' --auto_yes'
        return args, run_command, explicit_args
    return args, explicit_args

def short_path(path):
    rel_path = os.path.relpath(path)
    return rel_path if len(rel_path) < len(path) else path

def generate_run_command(all_args, implicit_args, explicit_args, short=True):
    # identify any added explicit arguments
    for key, val in all_args.items():
        if key not in implicit_args or implicit_args[key] != val:
            explicit_args[key] = val

    # remove unnecessary explicit args that are already implied by implicit args
    for key, value in implicit_args.items():
        if key in explicit_args and explicit_args[key] == value:
            del explicit_args[key]

    # remove unnecessary explicit args that are selected by args
    for key, value in all_args.items():
        if key in implicit_args and key in explicit_args and all_args[key] == implicit_args[key]:
            del explicit_args[key]
    
    # compute the minimum run command to re-execute the built pipeline non-interactively
    os.path.relpath(explicit_args['bids_dir'])
    run_command = f"qsmxt {short_path(explicit_args['bids_dir'])}"
    if 'premade' in explicit_args and explicit_args['premade'] != 'default':
        run_command += f" --premade '{explicit_args['premade']}'"
    for key, value in explicit_args.items():
        if key in ['bids_dir', 'output_dir', 'workflow_dir', 'auto_yes', 'premade', 'multiproc', 'mem_avail', 'n_procs']: continue
        if key == 'labels_file' and value == os.path.join(get_qsmxt_dir(), 'aseg_labels.csv'):
            continue
        if key == 'do_qsm' and value == True and all(x not in explicit_args.keys() for x in ['do_swi', 'do_r2starmap', 'do_t2starmap', 'do_segmentation', 'do_analysis']):
            continue
        if key == 'do_qsm' and value == False and any(x in explicit_args.keys() for x in ['do_swi', 'do_r2starmap', 'do_t2starmap', 'do_segmentation', 'do_analysis']):
            continue
        elif value == True and isinstance(value, bool): run_command += f' --{key}'
        elif value == False and isinstance(value, bool): run_command += f' --{key} off'
        elif isinstance(value, str): run_command += f" --{key} '{value}'"
        elif isinstance(value, (int, float)): run_command += f" --{key} {value}"
        elif isinstance(value, list):
            run_command += f" --{key}"
            for val in value:
                run_command += f" {val}"
    run_command += ' --auto_yes'

    return run_command

def get_compliance_message(args):

    if not args.do_qsm:
        return

    compliant = True
    message = ""

    # ✅ Phase-quality-based masking
    if args.masking_input != 'phase' or args.masking_algorithm == 'bet':
        compliant = False
        message += "\n - Phase-quality-based masking recommended"

    if args.use_existing_masks:
        compliant = False
        message += "\n - The existing masks may not be phase-quality-based"
    
    # ✅ Multi-echo images should be combined before background removal
    if not args.combine_phase:
        compliant = False
        message += "\n - B0 mapping recommended on phase images"
    
    # ✅ SHARP/PDF
    if not any(x in args.bf_algorithm for x in ['sharp', 'pdf']) or args.qsm_algorithm in ['nextqsm', 'tgv']:
        compliant = False
        message += "\n - SHARP/PDF based background field removal recommended"
    
    # ✅ Sparsity-based dipole inversion
    if not any(x in args.qsm_algorithm for x in ['rts']):
        compliant = False
        message += "\n - Sparsity-based dipole inversion recommended"
    
    # ✅ Susceptibility values should be referenced
    if args.qsm_reference is None:
        compliant = False
        message += "\n - Susceptibility values should be referenced"

    if not compliant:
        message = "WARNING: Pipeline is NOT guidelines compliant (see https://doi.org/10.1002/mrm.30006):" + message
    else:
        message = "Guidelines compliant! (see https://doi.org/10.1002/mrm.30006)"

    return message


def get_interactive_args(args, explicit_args, implicit_args, premades, using_json_settings):
    class dotdict(dict):
        """dot.notation access to dictionary attributes"""
        __getattr__ = dict.get
        __setattr__ = dict.__setitem__
        __delattr__ = dict.__delitem__
    args = dotdict(args)

    def update_desired_images():
        print("\n=== Desired outputs ===")
        print(" qsm: Quantitative Susceptibility Mapping (QSM)")
        print(" swi: Susceptibility Weighted Imaging (SWI)")
        print(" t2s: T2* maps")
        print(" r2s: R2* maps")
        print(" seg: Segmentations (requires qsm)")
        print(" analysis: QSM across segmented ROIs (requires qsm+seg)")
        print(" template: GRE group space + GRE/QSM templates (requires qsm)")
        print(" dicoms: Output DICOMs where possible (compatible image types include QSM, SWI and SWI-MIP)")

        while True:
            user_in = input("\nEnter desired images (space-separated) [default - qsm]: ").lower()
            while not all(x in ['qsm', 'swi', 't2s', 'r2s', 'seg', 'analysis', 'template', 'dicoms'] for x in user_in.split()):
                user_in = input("Enter desired images (space-separated) [default - qsm]: ").lower()

            if 'qsm' in user_in.split() or len(user_in.split()) == 0:
                args.do_qsm = True
                if len(user_in.split()) > 1:
                    explicit_args['do_qsm'] = True
            if 'qsm' not in user_in.split() and len(user_in.split()) > 0:
                args.do_qsm = False
                if 'do_qsm' in explicit_args: del explicit_args['do_qsm']
                if 'premade' in explicit_args: del explicit_args['premade']
                args.premade = 'default'
            if 'swi' in user_in.split():
                args.do_swi = True
                explicit_args['do_swi'] = True
            else:
                if 'do_swi' in explicit_args: del explicit_args['do_swi']
                args.do_swi = False
            if 't2s' in user_in.split():
                args.do_t2starmap = True
                explicit_args['do_t2starmap'] = True
            else:
                if 'do_t2starmap' in explicit_args: del explicit_args['do_t2starmap']
                args.do_t2starmap = False
            if 'r2s' in user_in.split():
                args.do_r2starmap = True
                explicit_args['do_r2starmap'] = True
            else:
                if 'do_r2starmap' in explicit_args: del explicit_args['do_r2starmap']
                args.do_r2starmap = False
            if 'seg' in user_in.split():
                args.do_segmentation = True
                explicit_args['do_segmentation'] = True
            else:
                if 'do_segmentation' in explicit_args: del explicit_args['do_segmentation']
                args.do_segmentation = False
            if 'analysis' in user_in.split():
                args.do_analysis = True
                explicit_args['do_analysis'] = True
            else:
                if 'do_analysis' in explicit_args: del explicit_args['do_analysis']
                args.do_analysis = False
            if 'template' in user_in.split():
                args.do_template = True
                explicit_args['do_template'] = True
            else:
                if 'do_template' in explicit_args: del explicit_args['do_template']
                args.do_template = False
            if 'dicoms' in user_in.split():
                if any([args.do_qsm, args.do_swi]):
                    args.export_dicoms = True
                    explicit_args['export_dicoms'] = True
                else:
                    print("dicoms requires one of either qsm or swi.")
                    continue
            else:
                if 'export_dicoms' in explicit_args: del explicit_args['export_dicoms']
                args.export_dicoms = False
            
            break
        
    # allow user to update the premade if none was chosen
    if not using_json_settings and 'premade' not in explicit_args.keys() and not any(x in explicit_args.keys() for x in ["do_qsm", "do_swi", "do_t2starmap", "do_r2starmap", "do_segmentations", "do_analysis"]):
        update_desired_images()

    def select_premade():
            print("\n=== Premade QSM pipelines ===")

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
            if args.premade == 'default':
                if 'premade' in explicit_args: del explicit_args['premade']
            else:
                explicit_args['premade'] = args.premade
            for key, value in premades[args.premade].items():
                if key not in explicit_args and key != 'premade':
                    args[key] = value
                implicit_args[key] = value
    if args.do_qsm and 'premade' not in explicit_args.keys() and not using_json_settings:
        select_premade()

    args = process_args(args)

    # pipeline customisation
    while True:
        print("\n=== QSMxT - Settings Menu ===")
        
        print("\n(1) Desired outputs:")
        print(f" - Quantitative Susceptibility Mapping (QSM): {'Yes' if args.do_qsm else 'No'}")
        print(f" - Susceptibility Weighted Imaging (SWI): {'Yes' if args.do_swi else 'No'}")
        print(f" - T2* mapping: {'Yes' if args.do_t2starmap else 'No'}")
        print(f" - R2* mapping: {'Yes' if args.do_r2starmap else 'No'}")
        print(f" - Segmentations: {'Yes' if args.do_segmentation else 'No'}")
        print(f" - Analysis CSVs: {'Yes' if args.do_analysis else 'No'}")
        print(f" - GRE/QSM template space: {'Yes' if args.do_template else 'No'}")
        print(f" - DICOM outputs: {'Yes' if args.export_dicoms else 'No'}")

        if args.do_qsm:
            print(f"\n(2) QSM pipeline: {args.premade}")
            print("\n(3) [ADVANCED] QSM masking:")
            print(f" - Use existing masks if available: {'Yes' if args.use_existing_masks else 'No'}" + (f" (using PIPELINE_NAME={args.existing_masks_pipeline})" if args.use_existing_masks else ""))
            if args.masking_algorithm == 'threshold':
                print(f" - Masking algorithm: threshold ({args.masking_input}-based{('; inhomogeneity-corrected)' if args.masking_input == 'magnitude' and args.inhomogeneity_correction else ')')}")
                print(f"   - Two-pass artefact reduction: {'Enabled' if args.two_pass else 'Disabled'}")
                if args.threshold_value:
                    if len(args.threshold_value) >= 2 and all(args.threshold_value) and args.two_pass:
                        if int(args.threshold_value[0]) == float(args.threshold_value[0]) and int(args.threshold_value[1]) == float(args.threshold_value[1]):
                            print(f"   - Threshold: {int(args.threshold_value[0])}, {int(args.threshold_value[1])} (hardcoded voxel intensities)")
                        else:
                            print(f"   - Threshold: {float(args.threshold_value[0])*100}%, {float(args.threshold_value[1])*100}% (hardcoded percentiles of the signal histogram)")
                    elif len(args.threshold_value) == 1 and all(args.threshold_value):
                        if int(args.threshold_value[0]) == float(args.threshold_value[0]):
                            print(f"   - Threshold: {int(args.threshold_value[0])} (hardcoded voxel intensity)")
                        else:
                            print(f"   - Threshold: {float(args.threshold_value[0])*100}% (hardcoded percentile of per-echo histogram)")
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
            
            print("\n(4) [ADVANCED] QSM phase processing:")
            print(f" - Axial resampling: " + (f"Enabled (obliquity threshold = {args.obliquity_threshold})" if args.obliquity_threshold != -1 else "Disabled"))
            print(f" - Multi-echo combination: " + ("B0 mapping (using ROMEO)" if args.combine_phase else "Susceptibility averaging"))
            if args.qsm_algorithm not in ['tgv']:
                print(f" - Phase unwrapping: {args.unwrapping_algorithm}")
                if args.qsm_algorithm not in ['nextqsm']:
                    print(f" - Background field removal: {args.bf_algorithm}")
            print(f" - Dipole inversion: {args.qsm_algorithm}")
            print(f" - Referencing: {args.qsm_reference}")

        if args.do_analysis:
            print("\n(5) Analysis")
            if args.do_qsm:
                if args.use_existing_qsms:
                    print(f" - QSM inputs: QSMxT-generated and pre-existing (from derived pipeline matching '{args.existing_qsm_pipeline}')")
                else:
                    print(f" - QSM inputs: QSMxT-generated")
            else:
                print(f" - QSM inputs: Pre-existing (from derived pipeline matching '{args.existing_qsm_pipeline}')")

            if args.do_segmentation:
                if args.use_existing_segmentations:
                    print(f" - Segmentation inputs: QSMxT-generated and pre-existing (from derived pipeline matching '{args.existing_segmentation_pipeline}')")
                else:
                    print(f" - Segmentation inputs: QSMxT-generated")
            else:
                print(f" - Segmentation inputs: Pre-existing (from derived pipeline matching '{args.existing_segmentation_pipeline}')")

        message = get_compliance_message(args=args)
        if message:
            print(f"\n{message}")

        print(f"\nRun command: {generate_run_command(all_args=args, implicit_args=implicit_args, explicit_args=explicit_args)}")
        
        user_in = get_option(
            prompt="\nEnter a number to customize; enter 'run' to run: ",
            options=['run', '1'] + (['2', '3', '4'] if args.do_qsm else []) + (['5'] if args.do_analysis else []),
            default=None
        )
        if user_in == 'run': break
        
        if user_in == '1':
            update_desired_images()
        if user_in == '2': # PREMADE
            select_premade()
        if user_in == '3': # MASKING
            print("=== MASKING ===")

            print("\n== Existing masks ==")
            args.use_existing_masks = 'yes' == get_option(
                prompt=f"Use existing masks if available [default: {'yes' if args.use_existing_masks else 'no'}]: ",
                options=['yes', 'no'],
                default='yes' if args.use_existing_masks else 'no'
            )
            if args.use_existing_masks:
                args.existing_masks_pipeline = get_string(
                    prompt=f"Enter pattern to match the software pipeline name from which masks were derived or '*' to match any [default: {args.existing_masks_pipeline}]: ",
                    default=args.existing_masks_pipeline
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
        if user_in == '4': # PHASE PROCESSING
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
            print("   - Compatible with two-pass artefact reduction algorithm")
            print("tgv: Total Generalized Variation")
            print("   - https://doi.org/10.1016/j.neuroimage.2015.02.041")
            print("   - Combined unwrapping, background field removal and dipole inversion")
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

            print("\n== QSM reference ==")
            print("Select a QSM reference:\n")
            print("mean: QSM will be relative to the subject-level mean of the non-zero QSM values")
            print("segmentation ID (enter int; requires segmentation pipeline): QSM will be relative to the subject-level mean of the QSM within the segmentation mask")
            print("none: No QSM referencing")

            user_in = None
            while True:
                user_in = input(f"\nSelect QSM reference [default - {args.qsm_reference}]: ")
                if user_in == "":
                    user_in = args.qsm_reference
                    break
                elif user_in in ['mean', 'none']:
                    break
                elif user_in.isnumeric() and args.do_segmentation:
                    user_in = [int(user_in)]
                    break
                elif user_in.isnumeric():
                    print("Segmentation pipeline must be enabled for that option.")
                elif user_in not in ['mean', 'none', ''] and not user_in.isnumeric():
                    print("Invalid input")
            args.qsm_reference = user_in
        if user_in == '5': # ANALYSIS
            
            if args.do_qsm:
                print("\n== QSM images for analysis ==")
                args.use_existing_qsms == 'yes' == get_option(
                    prompt=f"\nInclude pre-existing QSMs in analyses? [default - {'yes' if args.use_existing_qsms else 'no'}]: ",
                    options=['yes', 'no'],
                    default='yes' if args.use_existing_qsms else 'no'
                )
            
            derived_dirs = [item.split(os.path.sep)[-1] for item in glob.glob(os.path.join(args.bids_dir, "derivatives", "*")) if 'qsmxt-workflow' not in item and glob.glob(os.path.join(item, "sub*"))]
            if args.use_existing_qsms and derived_dirs:
                print("\n== QSM derived pipeline ==")
                print("\nDetected the following pipelines:")
                for i, derived_dir in enumerate(derived_dirs):
                    print(f"{i+1}. {derived_dir}")
                
                user_in = get_string(
                    prompt=f"Select one of the derived pipelines or enter a pattern [default - '{args.existing_qsm_pipeline}']: ",
                    default=args.existing_qsm_pipeline
                )

                try:
                    args.existing_qsm_pipeline = derived_dirs[int(user_in)-1]
                except:
                    args.existing_qsm_pipeline = user_in

            if args.do_segmentation:
                print("\n== Segmentations for analysis ==")
                args.use_existing_segmentations == 'yes' == get_option(
                    prompt=f"\nInclude pre-existing segmentations in analyses? [default - {'yes' if args.use_existing_segmentations else 'no'}]: ",
                    options=['yes', 'no'],
                    default='yes' if args.use_existing_segmentations else 'no'
                )
            
            derived_dirs = [item.split(os.path.sep)[-1] for item in glob.glob(os.path.join(args.bids_dir, "derivatives", "*")) if 'qsmxt-workflow' not in item and glob.glob(os.path.join(item, "sub*"))]
            if args.use_existing_qsms and derived_dirs:
                print("\n== Segmentations derived pipeline ==")
                print("\nDetected the following pipelines:")
                for i, derived_dir in enumerate(derived_dirs):
                    print(f"{i+1}. {derived_dir}")
                
                user_in = get_string(
                    prompt=f"Select one of the derived pipelines or enter a pattern [default - '{args.existing_segmentation_pipeline}']: ",
                    default=args.existing_segmentation_pipeline
                )

                try:
                    args.existing_segmentation_pipeline = derived_dirs[int(user_in)-1]
                except:
                    args.existing_segmentation_pipeline = user_in

    return args.copy(), implicit_args

def process_args(args):
    run_args = {}
    logger = make_logger('main')

    if not any([args.do_qsm, args.do_segmentation, args.do_swi, args.do_t2starmap, args.do_r2starmap, args.do_analysis]):
        args.do_qsm = True

    if args.do_analysis and not args.do_qsm:
        args.use_existing_qsms = True
    if args.do_analysis and not args.do_segmentation:
        args.use_existing_segmentations = True
    if args.do_template and not args.do_qsm:
        args.use_existing_qsms = True

    # default QSM algorithms
    if not args.qsm_algorithm:
        logger.log(LogLevel.WARNING.value, 'No QSM algorithm selected! Defaulting to RTS.')
        args.qsm_algorithm = 'rts'

    # default masking settings for QSM algorithms
    if not args.masking_algorithm:
        if args.qsm_algorithm == 'nextqsm':
            args.masking_algorithm = 'bet'
        else:
            args.masking_algorithm = 'threshold'
        logger.log(LogLevel.WARNING.value, f"No --masking_algorithm set! Defaulting to {args.masking_algorithm}.")
    
    # force masking input to magnitude if bet is the masking method
    if 'bet' in args.masking_algorithm and args.masking_input != 'magnitude':
        logger.log(LogLevel.WARNING.value, f"Switching --masking_input to 'magnitude' which is required for --masking_algorithm 'bet'.")
        args.masking_input = 'magnitude'

    # default threshold settings
    if args.masking_algorithm == 'threshold':
        if not (args.threshold_value or args.threshold_algorithm):
            logger.log(LogLevel.WARNING.value, f"--masking_algorithm set to 'threshold' but no --threshold_value or --threshold_algorithm set! Defaulting --threshold_algorithm to otsu.")
            args.threshold_algorithm = 'otsu'
        if not args.filling_algorithm:
            logger.log(LogLevel.WARNING.value, f"--masking_algorithm set to 'threshold' but no --filling_algorithm set! Defaulting to 'both'.")
            args.filling_algorithm = 'both'

    # default unwrapping settings for QSM algorithms
    if not args.unwrapping_algorithm and args.qsm_algorithm in ['nextqsm', 'rts', 'tv']:
        args.unwrapping_algorithm = 'romeo'
        logger.log(LogLevel.WARNING.value, f"Unwrapping is required for --qsm_algorithm {args.qsm_algorithm} but none is selected! Defaulting to --unwrapping_algorithm {args.unwrapping_algorithm}.")
    if args.combine_phase and args.unwrapping_algorithm != 'romeo':
        logger.log(LogLevel.WARNING.value, f"--combine_phase option requires --unwrapping_algorithm 'romeo'. Switching to --unwrapping_algorithm 'romeo'.")
        args.unwrapping_algorithm = 'romeo'

    if args.unwrapping_algorithm and args.qsm_algorithm in ['tgv']:
        logger.log(LogLevel.WARNING.value, f"--unwrapping_algorithm {args.unwrapping_algorithm} selected, but unwrapping is already handled by --qsm_algorithm 'tgv'. Disabling unwrapping.")
        args.unwrapping_algorithm = None

    # add_bet option only works with non-bet masking and filling methods
    if 'bet' in args.masking_algorithm and args.add_bet:
        logger.log(LogLevel.WARNING.value, f"--add_bet option does not work with --masking_algorithm bet. Disabling --add_bet.")
        args.add_bet = False
    if args.filling_algorithm == 'bet' and args.add_bet:
        logger.log(LogLevel.WARNING.value, f"--add_bet option does not work with --filling_algorithm bet. Disabling --add_bet.")
        args.add_bet = False

    # default two-pass settings for QSM algorithms
    if args.two_pass is None:
        if args.qsm_algorithm in ['rts', 'tgv', 'tv']:
            args.two_pass = True
            logger.log(LogLevel.WARNING.value, f"--two_pass setting not selected. Defaulting to 'on' for --qsm_algorithm {args.qsm_algorithm}.")
        else:
            args.two_pass = False
            logger.log(LogLevel.WARNING.value, f"--two_pass setting not selected. Defaulting to 'off' for --qsm_algorithm {args.qsm_algorithm}.")
    
    # two-pass does not work with bet masking, nextqsm, or vsharp
    if args.two_pass and 'bet' in args.masking_algorithm:
        logger.log(LogLevel.WARNING.value, f"--two_pass setting incompatible with --masking_algorithm bet. Disabling --two_pass.")
        args.two_pass = False
    
    if args.two_pass and args.qsm_algorithm == 'nextqsm':
        logger.log(LogLevel.WARNING.value, f"--two_pass setting incompatible with --qsm_algorithm nextqsm. Disabling --two_pass.")
        args.two_pass = False
    
    if args.two_pass and (args.bf_algorithm == 'vsharp' and args.qsm_algorithm in ['tv', 'rts', 'nextqsm']):
        logger.log(LogLevel.WARNING.value, f"--two_pass setting incompatible with --bf_algorithm vsharp. Disabling --two_pass.")
        args.two_pass = False

    # decide on inhomogeneity correction
    if args.inhomogeneity_correction and not (args.add_bet or args.masking_input == 'magnitude' or args.filling_algorithm == 'bet'):
        logger.log(LogLevel.WARNING.value, f"--inhomogeneity_correction requries either --add_bet, --masking_input 'magnitude', or --filling_algorithm 'bet'.")
        args.inhomogeneity_correction = False

    # decide on supplementary imaging
    if args.do_r2starmap is None: args.do_r2starmap = False
    if args.do_t2starmap is None: args.do_t2starmap = False
    if args.do_swi is None: args.do_swi = False
    
    # set number of concurrent processes to run depending on available resources
    if not args.n_procs:
        args.n_procs = int(os.environ["NCPUS"] if "NCPUS" in os.environ else os.cpu_count())
    
    # get rough estimate of 90% of the available memory
    args.mem_avail = psutil.virtual_memory().available / (1024 ** 3) * 0.90
    
    # determine whether multiproc will be used
    args.multiproc = not (args.pbs or any(args.slurm))

    # set default labels file if needed
    args.labels_file = args.labels_file or os.path.join(get_qsmxt_dir(), 'aseg_labels.csv')

    # debug options
    #config.set('execution', 'remove_node_directories', 'true')
    config.set('execution', 'try_hard_link_datasink', 'true')
    if args.debug:
        config.set('execution', 'stop_on_first_crash', 'true')
        config.set('monitoring', 'enabled', 'true')
        config.set('monitoring', 'summary_file', os.path.join(args.output_dir, 'resource_monitor.json'))
        #config.set('execution', 'remove_unnecessary_outputs', 'false')
        #config.set('execution', 'keep_inputs', 'true')
        #config.set('logging', 'workflow_level', 'DEBUG')
        #config.set('logging', 'interface_level', 'DEBUG')
        #config.set('logging', 'utils_level', 'DEBUG')

    return args

def set_env_variables(args):
    # misc environment variables
    os.environ["FSLOUTPUTTYPE"] = "NIFTI"

    # path environment variable
    os.environ["PATH"] += os.pathsep + os.path.join(get_qsmxt_dir(), "scripts")

    # add this_dir and cwd to pythonpath
    if "PYTHONPATH" in os.environ: os.environ["PYTHONPATH"] += os.pathsep + get_qsmxt_dir()
    else:                          os.environ["PYTHONPATH"]  = get_qsmxt_dir()


def visualize_resource_usage(json_file, wf):
    import pandas as pd
    import numpy as np
    from matplotlib import pyplot as plt

    logger = make_logger('main')
    json_dir = os.path.split(json_file)[0]

    # Load JSON data from file
    with open(json_file, 'r') as file:
        data = json.load(file)
    
    # Convert JSON data to DataFrame
    df = pd.DataFrame(data)
    
    # Convert Unix timestamps to datetime
    df['time'] = pd.to_datetime(df['time'], unit='s')
    
    # Set time as the index
    df.set_index('time', inplace=True)
    
    # Simplify name entries
    df['name'] = df['name'].apply(lambda x: x.split('.')[-1])

    # Create a dictionary from workflow with node names and requested memory
    mem_requested = {node.name: node.mem_gb for node in wf._get_all_nodes()}

    # Map the requested memory to the DataFrame
    df['mem_requested'] = df['name'].map(mem_requested)

    csv_file_path = os.path.join(json_dir, 'resource_usage.csv')
    df.to_csv(csv_file_path)
    logger.log(LogLevel.INFO.value, f"Resource usage data saved as CSV at: {csv_file_path}")

    # Plotting all resource usages
    plt.figure(figsize=(24, 8))
    for name in df['name'].unique():
        subset = df[df['name'] == name]
        plt.plot(subset.index.values.ravel(), subset['rss_GiB'].values.ravel(), label=f"{name} RSS used")
        plt.plot(subset.index.values.ravel(), subset['vms_GiB'].values.ravel(), label=f"{name} VMS used")
    plt.title('Memory Usage Over Time')
    plt.ylabel('Memory (GiB)')
    plt.xlabel('Time')
    plt.legend(loc='upper left', bbox_to_anchor=(1, 1))
    plt.grid(True)
    plt.tight_layout()
    plt.savefig(os.path.join(json_dir, "mem-usage.png"))

    plt.figure(figsize=(24, 8))
    for name in df['name'].unique():
        subset = df[df['name'] == name]
        plt.plot(subset.index.values.ravel(), subset['cpus'].values.ravel(), label=f"{name} CPU used")
    plt.title('CPU Usage Over Time')
    plt.ylabel('CPU Usage (%)')
    plt.xlabel('Time')
    plt.legend(loc='upper left', bbox_to_anchor=(1, 1))
    plt.grid(True)
    plt.tight_layout()
    plt.savefig(os.path.join(json_dir, "cpu-usage.png"))

    # Plotting resource usages
    # Group by 'name' and calculate the maximum rss_GiB, vms_GiB and get mem_requested (assuming it's the same for each group)
    grouped = df.groupby('name').agg({
        'rss_GiB': 'max',
        'mem_requested': 'max'
    }).reset_index()

    # Plotting
    fig, ax = plt.subplots(figsize=(10, 6))

    # Adjust bar positions
    bar_width = 0.25  # Smaller bar width
    positions = range(len(grouped['name']))  # Positions for the bars

    ax.bar([p - bar_width/2 for p in positions], grouped['rss_GiB'], width=bar_width, label='Max rss_GiB', color='b', align='center')
    ax.bar([p + bar_width/2 for p in positions], grouped['mem_requested'], width=bar_width, label='Memory Requested', color='g', align='center')

    # Labels and legend
    ax.set_xlabel('Process Name')
    ax.set_ylabel('Memory in GiB')
    ax.set_xticks(positions)
    ax.set_xticklabels(grouped['name'], rotation=90)

    ax.legend(loc='upper left')

    plt.title('Memory Usage Metrics by Process')
    plt.grid(True)
    plt.tight_layout()
    plt.savefig(os.path.join(json_dir, "max-mem-usage.png"))

def write_citations(wf, args):
    # get all node names
    node_names = [node._name.lower() for node in wf._get_all_nodes()]

    def any_string_matches_any_node(strings):
        return any(string in node_name for string in strings for node_name in node_names)

    # write "references.txt" with the command used to invoke the script and any necessary references
    with open(os.path.join(args.output_dir, "references.txt"), 'w', encoding='utf-8') as f:
        f.write("== References ==")
        f.write(f"\n\n - QSMxT{'' if not args.two_pass else ' and two-pass combination method'}: Stewart AW, Robinson SD, O'Brien K, et al. QSMxT: Robust masking and artifact reduction for quantitative susceptibility mapping. Magnetic Resonance in Medicine. 2022;87(3):1289-1300. doi:10.1002/mrm.29048")
        f.write("\n\n - QSMxT: Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT")
        f.write("\n\n - Python package - Nipype: Gorgolewski K, Burns C, Madison C, et al. Nipype: A Flexible, Lightweight and Extensible Neuroimaging Data Processing Framework in Python. Frontiers in Neuroinformatics. 2011;5. Accessed April 20, 2022. doi:10.3389/fninf.2011.00013")
        if args.do_qsm:
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
            if any_string_matches_any_node(['clearswi']):
                f.write("\n\n - SWI - CLEARSWI: Eckstein, K., Bachrata, B., Hangel, G., et al. Improved susceptibility weighted imaging at ultra-high field using bipolar multi-echo acquisition and optimized image processing: CLEAR-SWI. Neuroimage, 237, 118175. doi:10.1016/j.neuroimage.2021.118175")
            if any_string_matches_any_node(['mrt']):
                f.write("\n\n - Julia package - MriResearchTools: Eckstein K. korbinian90/MriResearchTools.jl. GitHub; 2022. https://github.com/korbinian90/MriResearchTools.jl")
            if any_string_matches_any_node(['nibabel']):
                f.write("\n\n - Python package - nibabel: Brett M, Markiewicz CJ, Hanke M, et al. nipy/nibabel. GitHub; 2019. https://github.com/nipy/nibabel")
            if any_string_matches_any_node(['scipy']):
                f.write("\n\n - Python package - scipy: Virtanen P, Gommers R, Oliphant TE, et al. SciPy 1.0: fundamental algorithms for scientific computing in Python. Nat Methods. 2020;17(3):261-272. doi:10.1038/s41592-019-0686-2")
            if any_string_matches_any_node(['numpy']):
                f.write("\n\n - Python package - numpy: Harris CR, Millman KJ, van der Walt SJ, et al. Array programming with NumPy. Nature. 2020;585(7825):357-362. doi:10.1038/s41586-020-2649-2")
        if args.do_segmentation:
            f.write("\n\n - FastSurfer: Henschel L, Conjeti S, Estrada S, Diers K, Fischl B, Reuter M. FastSurfer - A fast and accurate deep learning based neuroimaging pipeline. NeuroImage. 2020;219:117012. doi:10.1016/j.neuroimage.2020.117012")
        if args.do_segmentation or args.do_template:
            f.write("\n\n - ANTs: Avants BB, Tustison NJ, Johnson HJ. Advanced Normalization Tools. GitHub; 2022. https://github.com/ANTsX/ANTs")
        f.write("\n\n")

def script_exit(error_code=0, logger=None):
    if logger:
        show_warning_summary(logger)
        logger.log(LogLevel.INFO.value, 'Finished')
    if 'pytest' in sys.modules:
        if error_code == 0:
            return
        raise RuntimeError(f"Error code {error_code}")
    exit(error_code)

def main(argv=None):
    # get run arguments
    argv = argv or sys.argv[1:]

    # create initial logger
    logger = make_logger(name='pre', printlevel=LogLevel.DEBUG if '--debug' in argv else LogLevel.INFO, writelevel=LogLevel.DEBUG if '--debug' in argv else LogLevel.INFO)

    # display version and exit if needed
    if any(x in argv for x in ['-v', '--version']):
        logger.log(LogLevel.INFO.value, f"QSMxT v{get_qsmxt_version()}")
        script_exit(0)
    
    # parse explicit arguments
    args, run_command, explicit_args = parse_args(argv, return_run_command=True)

    # list premade pipelines and exit if needed
    if args.list_premades:
        print_qsm_premades(args.pipeline_file)
        script_exit(logger=logger)

    # create output directory if it doesn't exist
    os.makedirs(args.output_dir, exist_ok=True)
    
    # overwrite logger with one that logs to file
    logpath = os.path.join(args.output_dir, f"qsmxt.log")
    logger = make_logger(
        name='main',
        logpath=logpath,
        printlevel=LogLevel.DEBUG if args.debug else LogLevel.INFO,
        writelevel=LogLevel.DEBUG if args.debug else LogLevel.INFO
    )
    logger.log(LogLevel.INFO.value, f"QSMxT v{get_qsmxt_version()}")
    logger.log(LogLevel.INFO.value, f"Python interpreter: {sys.executable}")
    logger.log(LogLevel.INFO.value, f"Command: {run_command}")

    # write command to file
    with open(os.path.join(args.output_dir, 'command.txt'), 'w') as command_file:
        command_file.write(f"{run_command}\n")

    # print diff if needed
    diff = get_diff()
    if diff:
        logger.log(LogLevel.WARNING.value, f"QSMxT's working directory is not clean! Writing git diff to {os.path.join(args.output_dir, 'diff.txt')}...")
        with open(os.path.join(args.output_dir, "diff.txt"), "w") as diff_file:
            diff_file.write(diff)
    
    # process args and make any necessary corrections
    args = process_args(args)
    
    # display compliance message
    message = get_compliance_message(args)
    if message:
        if 'warning' in message.lower():
            logger.log(LogLevel.WARNING.value, message.replace('WARNING: ', '').replace('\n - ', '; '))
        else:
            logger.log(LogLevel.INFO.value, message)

    # display available memory
    if args.multiproc:
        logger.log(LogLevel.INFO.value, f"Available memory: {round(args.mem_avail, 3)} GB")

    # write settings to file
    with open(os.path.join(args.output_dir, 'settings.json'), 'w') as settings_file:
        json.dump({ "pipeline" : explicit_args }, settings_file)
    
    # set environment variables
    set_env_variables(args)
    
    # build workflow
    wf = init_workflow(args)

    # handle empty workflow
    if len(wf._get_all_nodes()) == 0:
        logger.log(LogLevel.ERROR.value, f"Workflow is empty! There is nothing to do.")
        script_exit(1, logger=logger)

    # write citations to file
    write_citations(wf, args)

    # set nipype logging options
    config.update_config({'logging': { 'log_directory': args.output_dir, 'log_to_file': True }})
    logging.update_logging(config)

    # run workflow
    if not args.dry:
        if args.slurm[0] is not None:
            logger.log(LogLevel.INFO.value, f"Running using SLURMGraph plugin with account={args.slurm[0]} and partition={args.slurm[1]}")
            slurm_args = gen_plugin_args(slurm_account=args.slurm[0], slurm_partition=args.slurm[1])
            slurm_args['dont_resubmit_completed_jobs'] = True
            wf.run(
                plugin='SLURMGraph',
                plugin_args=slurm_args
            )
        elif args.pbs:
            logger.log(LogLevel.INFO.value, f"Running using PBS Graph plugin with account={args.pbs}")
            wf.run(
                plugin='PBSGraph',
                plugin_args=gen_plugin_args(pbs_account=args.pbs)
            )
        else:
            logger.log(LogLevel.INFO.value, f"Running using MultiProc plugin with n_procs={args.n_procs}")
            plugin_args = { 'n_procs' : args.n_procs }
            if os.environ.get("PBS_JOBID"):
                logger.log(LogLevel.INFO.value, f"Detected PBS_JOBID! Identifying job memory limit...")
                jobid = os.environ.get("PBS_JOBID").split(".")[0]
                plugin_args['memory_gb'] = round(float(sys_cmd(f"qstat -f {jobid} | grep Resource_List.mem", print_output=False, print_command=False).split(" = ")[1].split("gb")[0]), 2)
                logger.log(LogLevel.INFO.value, f"Memory limit set to {plugin_args['memory_gb']} GB")
            wf.run(
                plugin='MultiProc',
                plugin_args=plugin_args
            )
            if args.debug:
                logger.log(LogLevel.DEBUG.value, f"Plotting resource monitor summaries...")
                visualize_resource_usage(os.path.join(args.output_dir, "resource_monitor.json"), wf)

    script_exit(logger=logger)
    return args

if __name__ == "__main__":
    main()

