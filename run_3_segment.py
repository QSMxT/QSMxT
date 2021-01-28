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
    subject_list,
    bids_dir,
    work_dir,
    out_dir,
    reconall_cpus,
    templates,
    qsub_account_string
):

    wf = Workflow(name='workflow_segmentation', base_dir=work_dir)

    # use infosource to iterate workflow across subject list
    n_infosource = Node(
        interface=IdentityInterface(
            fields=['subject_id']
        ),
        name="infosource"
        # input: 'subject_id'
        # output: 'subject_id'
    )
    # runs the node with subject_id = each element in subject_list
    n_infosource.iterables = ('subject_id', subject_list)

    # select matching files from bids_dir
    n_selectfiles = Node(
        interface=SelectFiles(
            templates=templates,
            base_directory=bids_dir
        ),
        name='selectfiles'
        # output: ['t1', 'mag']
    )
    wf.connect([
        (n_infosource, n_selectfiles, [('subject_id', 'subject_id_p')])
    ])

    # segment t1
    mn_reconall = MapNode(
        interface=ReconAll(
            parallel=True,
            openmp=reconall_cpus
            #hires=True,
            #mprage=True
        ),
        name='recon_all',
        iterfield=['T1_files', 'subject_id']
    )
    mn_reconall.plugin_args = {
        'qsub_args': f'-A {qsub_account_string} -q Short -l nodes=1:ppn={reconall_cpus},mem=20gb,vmem=20gb,walltime=12:00:00',
        'overwrite': True
    }
    wf.connect([
        (n_selectfiles, mn_reconall, [('T1', 'T1_files')]),
        (n_infosource, mn_reconall, [('subject_id', 'subject_id')])
    ])

    # convert segmentation to nii
    mn_reconall_aseg_nii = MapNode(
        interface=MRIConvert(
            out_type='niigz',
        ),
        name='reconall_aseg_nii',
        iterfield=['in_file']
    )
    wf.connect([
        (mn_reconall, mn_reconall_aseg_nii, [('aseg', 'in_file')]),
    ])

    # convert original t1 to nii
    mn_reconall_orig_nii = MapNode(
        interface=MRIConvert(
            out_type='niigz'
        ),
        name='reconall_orig_nii',
        iterfield=['in_file']
    )
    wf.connect([
        (mn_reconall, mn_reconall_orig_nii, [('orig', 'in_file')])
    ])

    # estimate transform for t1 to qsm
    mn_calc_t1_to_gre = MapNode(
        interface=bestlinreg.NiiBestLinRegInterface(),
        name='calculate_reg',
        iterfield=['in_fixed', 'in_moving']
    )
    wf.connect([
        (n_selectfiles, mn_calc_t1_to_gre, [('mag', 'in_fixed')]),
        (mn_reconall_orig_nii, mn_calc_t1_to_gre, [('out_file', 'in_moving')])
    ])

    # apply transform to segmentation
    mn_register_t1_to_gre = MapNode(
        interface=applyxfm.NiiApplyMincXfmInterface(),
        name='register_segmentations',
        iterfield=['in_file', 'in_like', 'in_transform']
    )
    wf.connect([
        (mn_reconall_aseg_nii, mn_register_t1_to_gre, [('out_file', 'in_file')]),
        (n_selectfiles, mn_register_t1_to_gre, [('mag', 'in_like')]),
        (mn_calc_t1_to_gre, mn_register_t1_to_gre, [('out_transform', 'in_transform')])
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
        (mn_reconall_aseg_nii, n_datasink, [('out_file', 't1_mni_segmentation')]),
        (mn_reconall_orig_nii, n_datasink, [('out_file', 't1_mni')]),
        (mn_register_t1_to_gre, n_datasink, [('out_file', 'qsm_segmentation')]),
        (mn_calc_t1_to_gre, n_datasink, [('out_transform', 't1_mni_to_qsm_transforms')])
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
        help='input data folder that can be created using run_1_dicomToBids.py; can also use a ' +
             'custom folder containing subject folders and NIFTI files or a BIDS folder with a ' +
             'different structure, as long as --subject_folder_pattern, --input_t1_pattern ' +
             'and --input_magnitude_pattern are also specified'
    )

    parser.add_argument(
        'out_dir',
        help='output segmentation directory; will be created if it does not exist'
    )

    parser.add_argument(
        '--work_dir',
        default=None,
        help='nipype working directory; defaults to \'work\' within \'out_dir\''
    )

    parser.add_argument(
        '--subject_folder_pattern',
        default='sub*',
        help='pattern to match subject folders in bids_dir'
    )

    parser.add_argument(
        '--input_t1_pattern',
        default='anat/*T1w*nii*',
        help='pattern to match input t1 files for segmentation within subject folders'
    )

    parser.add_argument(
        '--input_magnitude_pattern',
        default='anat/*qsm*E01*magnitude*nii*',
        help='pattern to match input magnitude files (in the qsm space) within subject folders'
    )
    
    parser.add_argument(
        '--subjects', '-s',
        default=None,
        nargs='*',
        help='list of subject folders to process; by default all subjects are processed'
    )

    parser.add_argument(
        '--pbs',
        default=None,
        dest='qsub_account_string',
        help='run the pipeline via PBS and use the argument as the QSUB account string'
    )

    parser.add_argument(
        '--debug',
        action='store_true',
        help='enables some nipype settings for debugging'
    )

    args = parser.parse_args()

    # environment variables
    os.environ["FSLOUTPUTTYPE"] = "NIFTI_GZ"
    os.environ["SUBJECTS_DIR"] = "."


    # check if minc is on the path and remove it - otherwise it collides with the old minc libraries included in freesurfer
    test = os.environ['PATH']
    clean_path=''
    print('before removing minc from path: ', test)
    for path in test.split(':'):
        if 'minc' in path:
            print('removing ', path)
        else:
            clean_path=clean_path+path+':'
    print('after removing minc from path: ', clean_path)
    os.environ['PATH'] = clean_path

    if args.debug:
        from nipype import config
        config.enable_debug_mode()
        config.set('execution', 'stop_on_first_crash', 'true')
        config.set('execution', 'remove_unnecessary_outputs', 'false')
        config.set('execution', 'keep_inputs', 'true')
        config.set('logging', 'workflow_level', 'DEBUG')
        config.set('logging', 'interface_level', 'DEBUG')
        config.set('logging', 'utils_level', 'DEBUG')

    # subject folders
    if not args.subjects:
        subject_list = [subj for subj in os.listdir(args.bids_dir) if fnmatch.fnmatch(subj, args.subject_folder_pattern) and os.path.isdir(os.path.join(args.bids_dir, subj))]
    else:
        subject_list = args.subjects

    if not args.work_dir: args.work_dir = args.out_dir
    os.environ["PATH"] += os.pathsep + os.path.join(os.path.dirname(os.path.abspath(__file__)), "scripts")

    # subject_folder_pattern, input_magnitude_pattern
    num_echoes = len(glob.glob(os.path.join(args.bids_dir, subject_list[0], args.input_magnitude_pattern)))
    if num_echoes == 0: args.input_magnitude_pattern = args.input_magnitude_pattern.replace("E01", "")

    if not glob.glob(os.path.join(args.bids_dir, subject_list[0], args.input_t1_pattern)):
        print(f"Error: No T1-weighted images found in {args.bids_dir} matching pattern {args.subject_folder_pattern}/{args.input_t1_pattern}")
        exit()
    if not glob.glob(os.path.join(args.bids_dir, subject_list[0], args.input_magnitude_pattern)):
        print(f"Error: No magnitude images found in {args.bids_dir} matching pattern {args.subject_folder_pattern}/{args.input_magnitude_pattern}")
        exit()

    templates={
        'T1': os.path.join('{subject_id_p}', args.input_t1_pattern),
        'mag': os.path.join('{subject_id_p}', args.input_magnitude_pattern)
    }

    wf = create_segmentation_workflow(
        subject_list=subject_list,
        bids_dir=os.path.abspath(args.bids_dir),
        work_dir=os.path.abspath(args.work_dir),
        out_dir=os.path.abspath(args.out_dir),
        reconall_cpus=16,
        templates=templates,
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
