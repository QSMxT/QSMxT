#!/usr/bin/env python3
from nipype.pipeline.engine import Workflow, Node, MapNode
from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import SelectFiles, DataSink, DataGrabber

from nipype.interfaces.freesurfer.preprocess import ReconAll, MRIConvert

import glob
import os
import os.path
import argparse


def create_segmentation_workflow(
    subject_list,
    bids_dir,
    work_dir,
    out_dir,
    reconall_cpus,
    templates
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
        'qsub_args': f'-A UQ-CAI -q Short -l nodes=1:ppn={reconall_cpus},mem=20gb,vmem=20gb,walltime=12:00:00',
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

    # registration to magnitude


    # datasink
    n_datasink = Node(
        interface=DataSink(
            base_directory=bids_dir
            #container=out_dir
        ),
        name='datasink'
    )
    wf.connect([
        (mn_reconall_aseg_nii, n_datasink, [('out_file', 'segmentation')]),
        (mn_reconall_orig_nii, n_datasink, [('out_file', 't1_orig')])
    ])

    return wf


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="QSM segmentation pipeline",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        'bids_dir',
        help='bids directory'
    )

    parser.add_argument(
        'out_dir',
        help='output directory'
    )

    parser.add_argument(
        '--work_dir',
        default=None,
        const=None,
        help='work directory'
    )

    parser.add_argument(
        '--subjects',
        default=None,
        const=None,
        nargs='*',
        help='list of subjects as seen in bids_dir'
    )

    parser.add_argument(
        '--pbs',
        action='store_true',
        help='use PBS graph'
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
        subject_list = [subj for subj in os.listdir(args.bids_dir) if 'sub' in subj]
    else:
        subject_list = args.subjects

    if not args.work_dir: args.work_dir = args.out_dir

    num_echoes = len(sorted(glob.glob(os.path.join(glob.glob(os.path.join(args.bids_dir, "sub") + "*")[0], 'anat/') + "*qsm*magnitude*.nii*")))
    multi_echo = num_echoes > 1

    templates={
        'T1': '{subject_id_p}/anat/*t1*.nii*',
        'mag': '{subject_id_p}/anat/' + ('*qsm*magnitude*.nii*' if not multi_echo else '*qsm*E01*magnitude*.nii*')
    }

    wf = create_segmentation_workflow(
        subject_list=subject_list,
        bids_dir=os.path.abspath(args.bids_dir),
        work_dir=os.path.abspath(args.work_dir),
        out_dir=os.path.abspath(args.out_dir),
        reconall_cpus=16,
        templates=templates
    )

    os.makedirs(os.path.abspath(args.work_dir), exist_ok=True)
    os.makedirs(os.path.abspath(args.out_dir), exist_ok=True)

    # run workflow
    if args.pbs:
        wf.run(
            plugin='PBSGraph',
            plugin_args={
                'qsub_args': '-A UQ-CAI -q Short -l nodes=1:ppn=1,mem=5GB,vmem=5GB,walltime=00:50:00'
            }
        )
    else:
        wf.run(
            plugin='MultiProc',
            plugin_args={
                'n_procs': int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count())
            }
        )
