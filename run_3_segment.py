#!/usr/bin/env python3
from nipype.pipeline.engine import Workflow, Node, MapNode
from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import DataSink, DataGrabber
from nipype.interfaces.freesurfer.preprocess import ReconAll, MRIConvert

from interfaces import nipype_interface_niiremoveheader as niiremoveheader
from interfaces import nipype_interface_bestlinreg as bestlinreg
from interfaces import nipype_interface_applyxfm as applyxfm

import time
import fnmatch
import glob
import os
import os.path
import argparse
import re

def init_workflow():
    subjects = [
        os.path.split(path)[1]
        for path in glob.glob(os.path.join(args.bids_dir, args.subject_pattern))
        if not args.subjects or os.path.split(path)[1] in args.subjects
    ]
    wf = Workflow("workflow_segmentation", base_dir=args.work_dir)
    wf.add_nodes([
        init_subject_workflow(subject)
        for subject in subjects
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
    wf = Workflow(subject, base_dir=os.path.join(args.work_dir, "workflow_segmentation"))
    wf.add_nodes([
        init_session_workflow(subject, session)
        for session in sessions
    ])
    return wf

def init_session_workflow(subject, session):

    # identify all runs - ensure that we only look at runs where both T1 and magnitude exist
    magnitude_runs = sorted(list(set([
        os.path.split(path)[1][os.path.split(path)[1].find('run-') + 4: os.path.split(path)[1].find('_', os.path.split(path)[1].find('run-') + 4)]
        for path in glob.glob(os.path.join(args.bids_dir, args.magnitude_pattern.replace("{run}", "").format(subject=subject, session=session)))
    ])))
    t1w_runs = sorted(list(set([
        os.path.split(path)[1][os.path.split(path)[1].find('run-') + 4: os.path.split(path)[1].find('_', os.path.split(path)[1].find('run-') + 4)]
        for path in glob.glob(os.path.join(args.bids_dir, args.t1_pattern.replace("{run}", "").format(subject=subject, session=session)))
    ])))
    if len(t1w_runs) != len(magnitude_runs):
        print(f"QSMxT: WARNING: Number of T1w and magnitude runs do not match for {subject}/{session}");
        time.sleep(3)
    runs = [f'run-{x}' for x in t1w_runs]

    wf = Workflow(session, base_dir=os.path.join(args.work_dir, "workflow_segmentation", subject, session))
    wf.add_nodes([
        init_run_workflow(subject, session, run)
        for run in runs
    ])
    return wf

def init_run_workflow(subject, session, run):

    wf = Workflow(run, base_dir=os.path.join(args.work_dir, "workflow_segmentation", subject, session, run))

    # get relevant files from this run
    t1_pattern = os.path.join(args.bids_dir, args.t1_pattern.format(subject=subject, session=session, run=run))
    mag_pattern = os.path.join(args.bids_dir, args.magnitude_pattern.format(subject=subject, session=session, run=run))
    t1_files = sorted(glob.glob(t1_pattern))
    mag_files = sorted(glob.glob(mag_pattern))
    if not t1_files:
        print(f"No T1w files matching pattern: {t1_pattern}")
        exit()
    if not mag_files:
        print(f"No magnitude files matching pattern: {mag_files}")
        exit()
    if len(t1_files) > 1:
        print(f"QSMxT: Warning: Multiple T1w files matching pattern {t1_pattern}")
    t1_file = t1_files[0]
    mag_file = mag_files[0]
    
    # segment t1
    n_reconall = Node(
        interface=ReconAll(
            parallel=True,
            openmp=1,
            mprage=args.t1_is_mprage,
            directive='all',
            T1_files=[t1_file]
            #hires=True,
        ),
        name='recon_all'
    )
    n_reconall.plugin_args = {
        'qsub_args': f'-A {args.qsub_account_string} -l walltime=12:00:00 -l select=1:ncpus={g_args.reconall_cpus}:mem=20gb',
        'overwrite': True
    }

    # convert segmentation to nii
    n_reconall_aseg_nii = Node(
        interface=MRIConvert(
            out_type='niigz',
            out_file=f'{subject}_{session}_{run}_t1w-aseg.nii.gz'
        ),
        name='reconall_aseg_nii'
    )
    wf.connect([
        (n_reconall, n_reconall_aseg_nii, [('aseg', 'in_file')])
    ])

    # remove header from magnitude file
    n_removeheader_magnitude = Node(
        interface=niiremoveheader.NiiRemoveHeaderInterface(
            in_file=mag_file
        ),
        name='remove_header_magnitude',
    )

    # remove header from t1w file
    n_removeheader_t1 = Node(
        interface=niiremoveheader.NiiRemoveHeaderInterface(
            in_file=t1_file
        ),
        name='remove_header_t1',
    )

    # remove header from t1w file
    n_removeheader_aseg = Node(
        interface=niiremoveheader.NiiRemoveHeaderInterface(),
        name='remove_header_aseg',
    )
    wf.connect([
        (n_reconall_aseg_nii, n_removeheader_aseg, [('out_file', 'in_file')])
    ])

    # estimate transform for t1 to qsm
    n_calc_t1_to_qsm = Node(
        interface=bestlinreg.NiiBestLinRegInterface(),
        name='calculate_reg'
    )
    wf.connect([
        (n_removeheader_magnitude, n_calc_t1_to_qsm, [('out_file', 'in_fixed')]),
        (n_removeheader_t1, n_calc_t1_to_qsm, [('out_file', 'in_moving')])
    ])

    # apply transform to segmentation
    n_register_t1_to_qsm = Node(
        interface=applyxfm.NiiApplyMincXfmInterface(),
        name='register_segmentations'
    )
    wf.connect([
        (n_removeheader_magnitude, n_register_t1_to_qsm, [('out_file', 'in_like')]),
        (n_removeheader_aseg, n_register_t1_to_qsm, [('out_file', 'in_file')]),
        (n_calc_t1_to_qsm, n_register_t1_to_qsm, [('out_transform', 'in_transform')])
    ])

    # datasink
    n_datasink = Node(
        interface=DataSink(
            base_directory=args.out_dir
            #container=out_dir
        ),
        name='datasink'
    )
    wf.connect([
        (n_removeheader_aseg, n_datasink, [('out_file', 't1_mni_segmentation')]),
        (n_removeheader_t1, n_datasink, [('out_file', 't1_mni')]),
        (n_register_t1_to_qsm, n_datasink, [('out_file', 'qsm_segmentation')]),
        (n_calc_t1_to_qsm, n_datasink, [('out_transform', 't1_mni_to_qsm_transforms')])
    ])

    return wf


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="QSMxT segment: QSM and T1 segmentation pipeline. Segments T1-weighted images and registers " +
                    "these to the QSM space to produce segmentations for both T1 and QSM.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        'bids_dir',
        help='Input data folder that can be created using run_1_dicomToBids.py. Can also use a ' +
             'custom folder containing subject folders and NIFTI files or a BIDS folder with a ' +
             'different structure, as long as --subject_pattern, --session_pattern, ' +
             '--t1_pattern and --magnitude_pattern are also specified.'
    )

    parser.add_argument(
        'out_dir',
        help='Output segmentation directory; will be created if it does not exist.'
    )

    parser.add_argument(
        '--work_dir',
        default=None,
        help='NiPype working directory; defaults to \'work\' within \'out_dir\'.'
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
        '--t1_pattern',
        default='{subject}/{session}/anat/*T1w*{run}*nii*',
        help='Pattern to match t1 files within the BIDS directory.'
    )

    parser.add_argument(
        '--magnitude_pattern',
        default='{subject}/{session}/anat/*qsm*{run}*magnitude*nii*',
        help='Pattern to match magnitude files within the BIDS directory.'
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
        '--t1_is_mprage',
        action='store_true',
        help='Use if t1w images are MPRAGE; improves segmentation.'
    )

    parser.add_argument(
        '--pbs',
        default=None,
        dest='qsub_account_string',
        help='Run the pipeline via PBS and use the argument as the QSUB account string.'
    )

    parser.add_argument(
        '--debug',
        action='store_true',
        help='Enables some NiPype settings for debugging.'
    )

    args = parser.parse_args()

    # supplementary arguments
    g_args = lambda:None
    g_args.reconall_cpus = 1# if args.qsub_account_string is None else int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count())

    # ensure directories are complete and absolute
    if not args.work_dir: args.work_dir = args.out_dir
    args.bids_dir = os.path.abspath(args.bids_dir)
    args.work_dir = os.path.abspath(args.work_dir)
    args.out_dir = os.path.abspath(args.out_dir)

    # this script's directory
    this_dir = os.path.dirname(os.path.abspath(__file__))

    # misc environment variables
    os.environ["SUBJECTS_DIR"] = "." # needed for reconall
    os.environ["FSLOUTPUTTYPE"] = "NIFTI_GZ"

    # PATH environment variable
    os.environ["PATH"] += os.pathsep + os.path.join(this_dir, "scripts")

    # PYTHONPATH environment variable
    if "PYTHONPATH" in os.environ: os.environ["PYTHONPATH"] += os.pathsep + this_dir
    else:                          os.environ["PYTHONPATH"]  = this_dir

    # don't remove outputs
    from nipype import config
    config.set('execution', 'remove_unnecessary_outputs', 'false')

    # debugging options
    if args.debug:
        config.enable_debug_mode()
        config.set('execution', 'stop_on_first_crash', 'true')
        config.set('execution', 'keep_inputs', 'true')
        config.set('logging', 'workflow_level', 'DEBUG')
        config.set('logging', 'interface_level', 'DEBUG')
        config.set('logging', 'utils_level', 'DEBUG')

    wf = init_workflow()

    os.makedirs(os.path.abspath(args.work_dir), exist_ok=True)
    os.makedirs(os.path.abspath(args.out_dir), exist_ok=True)

    # run workflow
    if args.qsub_account_string:
        wf.run(
            plugin='PBSGraph',
            plugin_args={
                'qsub_args': f'-A {args.qsub_account_string} -l walltime=00:50:00 -l select=1:ncpus=1:mem=5gb'
            }
        )
    else:
        wf.run(
            plugin='MultiProc',
            plugin_args={
                'n_procs': int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count())
            }
        )
