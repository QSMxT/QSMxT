#!/usr/bin/env python3
from nipype.pipeline.engine import Workflow, Node
from nipype.interfaces.io import DataSink
from nipype.interfaces.ants.registration import RegistrationSynQuick
from nipype.interfaces.ants.resampling import ApplyTransforms

from interfaces import nipype_interface_fastsurfer as fastsurfer
from interfaces import nipype_interface_mgz2nii as mgz2nii

import sys
import time
import glob
import os
import argparse
import psutil

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

    # register t1 to magnitude
    n_registration = Node(
        interface=RegistrationSynQuick(
            #num_threads=1,
            fixed_image=mag_file,
            moving_image=t1_file
        ),
        # relevant outputs: out_matrix
        name='ants_register-t1-to-qsm'
    )
    
    # segment t1
    n_fastsurfer = Node(
        interface=fastsurfer.FastSurferInterface(
            in_file=t1_file,
            num_threads=args.num_threads
        ),
        name='fastsurfer_segment-t1'
    )
    n_fastsurfer.plugin_args = {
        'qsub_args': f'-A {args.qsub_account_string} -l walltime=03:00:00 -l select=1:ncpus={args.num_threads}:mem=20gb:vmem=20gb',
        'overwrite': True
    }

    # convert segmentation to nii
    n_fastsurfer_aseg_nii = Node(
        interface=mgz2nii.Mgz2NiiInterface(),
        name='nibabel_mgz2nii',
    )
    wf.connect([
        (n_fastsurfer, n_fastsurfer_aseg_nii, [('out_file', 'in_file')])
    ])

    # apply transforms to segmentation
    n_transform_segmentation = Node(
        interface=ApplyTransforms(
            dimension=3,
            #output_image=,
            reference_image=mag_file,
            interpolation="NearestNeighbor"
        ),
        name='ants_transform-segmentation-to-qsm'
    )
    wf.connect([
        (n_fastsurfer_aseg_nii, n_transform_segmentation, [('out_file', 'input_image')]),
        (n_registration, n_transform_segmentation, [('out_matrix', 'transforms')])
    ])

    n_datasink = Node(
        interface=DataSink(
            base_directory=args.out_dir
            #container=out_dir
        ),
        name='nipype_datasink'
    )
    wf.connect([
        (n_fastsurfer_aseg_nii, n_datasink, [('out_file', 't1_segmentations')]),
        (n_transform_segmentation, n_datasink, [('output_image', 'qsm_segmentations')])
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
        default='{subject}/{session}/anat/*{run}*T1w*nii*',
        help='Pattern to match t1 files within the BIDS directory.'
    )

    parser.add_argument(
        '--magnitude_pattern',
        default='{subject}/{session}/anat/*{run}*magnitude*nii*',
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
        '--num_threads',
        type=int,
        default=1,
        help='The number of threads (MultiProc) or CPUs (PBS) used for each running instance ' +
             'of FastSurfer'
    )

    parser.add_argument(
        '--n_procs',
        type=int,
        default=None,
        help='Number of processes to run concurrently. By default, we use the number of ' +
             'available CPUs provided there are 4 GBs of memory available for each.'
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

    # ensure directories are complete and absolute
    args.out_dir = os.path.abspath(args.out_dir)
    args.bids_dir = os.path.abspath(args.bids_dir)
    args.work_dir = os.path.abspath(args.work_dir) if args.work_dir else os.path.abspath(args.out_dir)

    # this script's directory
    this_dir = os.path.dirname(os.path.abspath(__file__))

    # misc environment variables
    os.environ["SUBJECTS_DIR"] = "." # needed for reconall
    os.environ["FSLOUTPUTTYPE"] = "NIFTI_GZ"
    os.environ["FASTSURFER_HOME"] = "/opt/FastSurfer"

    # PATH environment variable
    os.environ["PATH"] += os.pathsep + os.path.join(this_dir, "scripts")
    os.environ["PATH"] += os.pathsep + os.path.abspath("/opt/FastSurfer/")

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

    os.makedirs(args.work_dir, exist_ok=True)
    os.makedirs(args.out_dir, exist_ok=True)

    # get number of CPUs
    n_cpus = int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count())
    
    # set number of concurrent processes to run depending on
    # available CPUs and RAM (max 1 per 11 GB of available RAM)
    if not args.n_procs:
        available_ram_gb = psutil.virtual_memory().available / 1e9
        args.n_procs = min(int(available_ram_gb / 11), n_cpus)

    # write "details_and_citations.txt" with the command used to invoke the script and any necessary citations
    with open(os.path.join(args.out_dir, "details_and_citations.txt"), 'w') as f:
        # output command used to invoke script
        f.write(str.join(" ", sys.argv))

        # qsmxt, nipype, fastsurfer, ants, nibabel
        f.write("\n\n - Stewart AW, Robinson SD, O'Brien K, et al. QSMxT: Robust masking and artifact reduction for quantitative susceptibility mapping. Magnetic Resonance in Medicine. 2022;87(3):1289-1300. doi:10.1002/mrm.29048")
        f.write("\n\n - Gorgolewski K, Burns C, Madison C, et al. Nipype: A Flexible, Lightweight and Extensible Neuroimaging Data Processing Framework in Python. Frontiers in Neuroinformatics. 2011;5. Accessed April 20, 2022. doi:10.3389/fninf.2011.00013")
        f.write("\n\n - Henschel L, Conjeti S, Estrada S, Diers K, Fischl B, Reuter M. FastSurfer - A fast and accurate deep learning based neuroimaging pipeline. NeuroImage. 2020;219:117012. doi:10.1016/j.neuroimage.2020.117012")
        f.write("\n\n - Avants BB, Tustison NJ, Johnson HJ. Advanced Normalization Tools. GitHub; 2022. https://github.com/ANTsX/ANTs")
        f.write("\n\n - Brett M, Markiewicz CJ, Hanke M, et al. nipy/nibabel. GitHub; 2019. https://github.com/nipy/nibabel")
        f.write("\n\n")

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
                'n_procs': args.n_procs
            }
        )

