#!/usr/bin/env python3
from nipype.pipeline.engine import Workflow, Node, MapNode
from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import SelectFiles, DataSink, DataGrabber

from nipype.interfaces.freesurfer.preprocess import ReconAll

import os
import os.path
import argparse


def create_segmentation_workflow(
    subject_list,
    bids_dir,
    work_dir,
    out_dir,
    reconall_threads,
    bids_templates={
        'T1': '{subject_id_p}/anat/*t1*.nii.gz'
    },
):
    # absolute paths to directories
    this_dir = os.path.dirname(os.path.abspath(__file__))
    bids_dir = os.path.join(this_dir, bids_dir)
    work_dir = os.path.join(this_dir, work_dir)
    out_dir = os.path.join(this_dir, out_dir)

    wf = Workflow(name='segmentation', base_dir=work_dir)

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
            templates=bids_templates,
            base_directory=bids_dir
        ),
        name='selectfiles'
        # output: ['mag', 'phs', 'params']
    )
    wf.connect([
        (n_infosource, n_selectfiles, [('subject_id', 'subject_id_p')])
    ])

    recon_all = MapNode(
        interface=ReconAll(
            parallel=True,
            openmp=reconall_threads
        ),
        name='recon_all',
        iterfield=['T1_files', 'subject_id']
    )
    wf.connect([
        (n_selectfiles, recon_all, [('T1', 'T1_files')]),
        (n_infosource, recon_all, [('subject_id', 'subject_id')])
    ])

    # TODO: Add register to atlas
    # ....

    # datasink
    n_datasink = Node(
        interface=DataSink(
            base_directory=bids_dir,
            container=out_dir
        ),
        name='datasink'
    )
    wf.connect([(recon_all, n_datasink, [('aseg', 'aseg')])])

    return wf


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="QSM segmentation pipeline",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        '--bids_dir',
        required=True,
        help='bids directory'
    )

    parser.add_argument(
        '--subjects',
        default=None,
        const=None,
        nargs='*',
        help='list of subjects as seen in bids_dir'
    )

    parser.add_argument(
        '--work_dir',
        required=True,
        help='work directory'
    )

    parser.add_argument(
        '--out_dir',
        required=True,
        help='output directory'
    )

    parser.add_argument(
        '--debug',
        dest='debug',
        action='store_true',
        help='debug mode'
    )

    args = parser.parse_args()

    # environment variables
    os.environ["FSLOUTPUTTYPE"] = "NIFTI_GZ"

    if args.debug:
        from nipype import config
        config.enable_debug_mode()
        config.set('execution', 'stop_on_first_crash', 'true')
        config.set('execution', 'remove_unnecessary_outputs', 'false')
        config.set('execution', 'keep_inputs', 'true')
        config.set('logging', 'workflow_level', 'DEBUG')
        config.set('logging', 'interface_level', 'DEBUG')
        config.set('logging', 'utils_level', 'DEBUG')

    if not args.subjects:
        subject_list = [subj for subj in os.listdir(
            args.bids_dir) if 'sub' in subj]
    else:
        subject_list = args.subjects

    ncpus = int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count())
    
    wf = create_segmentation_workflow(
        subject_list=subject_list,
        bids_dir=args.bids_dir,
        work_dir=args.work_dir,
        out_dir=args.out_dir,
        reconall_threads=ncpus
    )

    os.makedirs(os.path.abspath(args.work_dir), exist_ok=True)
    os.makedirs(os.path.abspath(args.out_dir), exist_ok=True)

    # run workflow
    wf.run('MultiProc', plugin_args={'n_procs': ncpus})
