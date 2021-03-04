#!/usr/bin/env python3
from nipype.pipeline.engine import Workflow, Node, MapNode
from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import SelectFiles, DataSink, DataGrabber
from nipype.interfaces.freesurfer.preprocess import ReconAll, MRIConvert

from interfaces import nipype_interface_bestlinreg as bestlinreg
from interfaces import nipype_interface_applyxfm as applyxfm

import fnmatch
import glob
import os
import os.path
import argparse
import re


def create_segmentation_workflow(
    session_dirs,
    bids_dir,
    work_dir,
    out_dir,
    reconall_cpus,
    templates,
    t1_is_mprage,
    qsub_account_string
):

    wf = Workflow(name='workflow_segmentation', base_dir=work_dir)

    # use infosource to iterate workflow across subject list
    n_infosource = Node(
        interface=IdentityInterface(
            fields=['session_dir']
        ),
        name="infosource"
        # input: 'session_dir'
        # output: 'session_dir'
    )
    # runs the node with session_id = each element in subject_list
    n_infosource.iterables = ('session_dir', session_dirs)

    # select matching files from bids_dir
    n_selectfiles = Node(
        interface=SelectFiles(
            templates=templates,
            base_directory=bids_dir,
            sort_filelist=True
        ),
        name='selectfiles'
        # output: ['T1', 'mag']
    )
    wf.connect([
        (n_infosource, n_selectfiles, [('session_dir', 'session_dir_p')])
    ])

    def get_first(in_f):
        if isinstance(in_f, list):
            return in_f[0]
        return in_f

    n_getfirst_t1 = Node(
        interface=Function(
            input_names=['in_f'],
            output_names=['out_f'],
            function=get_first
        ),
        iterfield=['in_f'],
        name='get_first_t1'
    )
    wf.connect([
        (n_selectfiles, n_getfirst_t1, [('T1', 'in_f')])
    ])

    # segment t1
    n_reconall = Node(
        interface=ReconAll(
            parallel=True,
            openmp=1,
            mprage=t1_is_mprage
            #hires=True,
        ),
        name='recon_all'
    )
    n_reconall.plugin_args = {
        'qsub_args': f'-A {qsub_account_string} -q Short -l nodes=1:ppn={reconall_cpus},mem=20gb,vmem=20gb,walltime=12:00:00',
        'overwrite': True
    }
    wf.connect([
        (n_getfirst_t1, n_reconall, [('out_f', 'T1_files')]),
        (n_infosource, n_reconall, [('session_dir', 'subject_id')])
    ])

    # convert segmentation to nii
    n_reconall_aseg_nii = Node(
        interface=MRIConvert(
            out_type='niigz',
        ),
        name='reconall_aseg_nii'
    )
    wf.connect([
        (n_reconall, n_reconall_aseg_nii, [('aseg', 'in_file')]),
    ])

    # convert original t1 to nii
    n_reconall_orig_nii = Node(
        interface=MRIConvert(
            out_type='niigz'
        ),
        name='reconall_orig_nii'
    )
    wf.connect([
        (n_reconall, n_reconall_orig_nii, [('orig', 'in_file')])
    ])

    # estimate transform for t1 to qsm
    n_calc_t1_to_gre = Node(
        interface=bestlinreg.NiiBestLinRegInterface(),
        name='calculate_reg'
    )
    wf.connect([
        (n_selectfiles, n_calc_t1_to_gre, [('mag', 'in_fixed')]),
        (n_reconall_orig_nii, n_calc_t1_to_gre, [('out_file', 'in_moving')])
    ])

    # apply transform to segmentation
    n_register_t1_to_gre = Node(
        interface=applyxfm.NiiApplyMincXfmInterface(),
        name='register_segmentations'
    )
    wf.connect([
        (n_reconall_aseg_nii, n_register_t1_to_gre, [('out_file', 'in_file')]),
        (n_selectfiles, n_register_t1_to_gre, [('mag', 'in_like')]),
        (n_calc_t1_to_gre, n_register_t1_to_gre, [('out_transform', 'in_transform')])
    ])

    # datasink
    n_datasink = Node(
        interface=DataSink(
            base_directory=out_dir
            #container=out_dir
        ),
        name='datasink'
    )
    wf.connect([
        (n_reconall_aseg_nii, n_datasink, [('out_file', 't1_mni_segmentation')]),
        (n_reconall_orig_nii, n_datasink, [('out_file', 't1_mni')]),
        (n_register_t1_to_gre, n_datasink, [('out_file', 'qsm_segmentation')]),
        (n_calc_t1_to_gre, n_datasink, [('out_transform', 't1_mni_to_qsm_transforms')])
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
             'different structure, as long as --subject_folder_pattern, --session_folder_pattern, ' +
             '--input_t1_pattern and --input_magnitude_pattern are also specified.'
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
        '--subject_folder_pattern',
        default='sub*',
        help='Pattern to match subject folders in bids_dir.'
    )

    parser.add_argument(
        '--session_folder_pattern',
        default='ses*',
        help='Pattern to match session folders in subject folders.'
    )

    parser.add_argument(
        '--input_t1_pattern',
        default='anat/*T1w*nii*',
        help='Pattern to match input t1 files for segmentation within subject folders.'
    )

    parser.add_argument(
        '--input_magnitude_pattern',
        default='anat/*qsm*E01*magnitude*nii*',
        help='Pattern to match input magnitude files (in the qsm space) within subject folders.'
    )
    
    parser.add_argument(
        '--subjects', '-s',
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

    # environment variables
    os.environ["FSLOUTPUTTYPE"] = "NIFTI_GZ"
    os.environ["SUBJECTS_DIR"] = "."

    if args.debug:
        from nipype import config
        config.enable_debug_mode()
        config.set('execution', 'stop_on_first_crash', 'true')
        config.set('execution', 'remove_unnecessary_outputs', 'false')
        config.set('execution', 'keep_inputs', 'true')
        config.set('logging', 'workflow_level', 'DEBUG')
        config.set('logging', 'interface_level', 'DEBUG')
        config.set('logging', 'utils_level', 'DEBUG')

    # determine subject/session folders
    session_dirs = glob.glob(os.path.join(args.bids_dir, args.subject_folder_pattern, args.session_folder_pattern))
    if args.subjects:
        session_dirs = [x for x in session_dirs if any(s in x for s in args.subjects)]
    if args.sessions:
        session_dirs = [x for x in session_dirs if any(s in x for s in args.sessions)]
    session_dirs = [x.replace(os.path.relpath(args.bids_dir) + os.path.sep, '') for x in session_dirs]

    if not args.work_dir: args.work_dir = args.out_dir
    os.environ["PATH"] += os.pathsep + os.path.join(os.path.dirname(os.path.abspath(__file__)), "scripts")

    # subject_folder_pattern, input_magnitude_pattern
    num_echoes = len(glob.glob(os.path.join(args.bids_dir, session_dirs[0], args.input_magnitude_pattern)))
    if num_echoes == 0: args.input_magnitude_pattern = args.input_magnitude_pattern.replace("E01", "")

    if not glob.glob(os.path.join(args.bids_dir, session_dirs[0], args.input_t1_pattern)):
        print(f"Error: No T1-weighted images found in {args.bids_dir} matching pattern {args.subject_folder_pattern}/{args.input_t1_pattern}")
        exit()
    if not glob.glob(os.path.join(args.bids_dir, session_dirs[0], args.input_magnitude_pattern)):
        print(f"Error: No magnitude images found in {args.bids_dir} matching pattern {args.subject_folder_pattern}/{args.input_magnitude_pattern}")
        exit()

    templates={
        'T1': os.path.join('{session_dir_p}', args.input_t1_pattern),
        'mag': os.path.join('{session_dir_p}', args.input_magnitude_pattern)
    }

    wf = create_segmentation_workflow(
        session_dirs=session_dirs,
        bids_dir=os.path.abspath(args.bids_dir),
        work_dir=os.path.abspath(args.work_dir),
        out_dir=os.path.abspath(args.out_dir),
        reconall_cpus=1 if args.qsub_account_string is None else int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count()),
        templates=templates,
        t1_is_mprage=args.t1_is_mprage,
        qsub_account_string=args.qsub_account_string
    )

    os.makedirs(os.path.abspath(args.work_dir), exist_ok=True)
    os.makedirs(os.path.abspath(args.out_dir), exist_ok=True)

    # run workflow
    if args.qsub_account_string:
        wf.run(
            plugin='PBSGraph',
            plugin_args={
                'qsub_args': f'-A {args.qsub_account_string} -q Short -l nodes=1:ppn=1,mem=5GB,vmem=5GB,walltime=00:50:00'
            }
        )
    else:
        wf.run(
            plugin='MultiProc',
            plugin_args={
                'n_procs': int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count())
            }
        )
