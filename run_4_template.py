#!/usr/bin/env python3
# Adapted from NiPype tutorial example for QSMxT

import os
import glob
import argparse
import psutil

import nipype.interfaces.utility as util
import nipype.interfaces.ants as ants
import nipype.interfaces.io as io
import nipype.pipeline.engine as pe

from niflow.nipype1.workflows.smri.ants import ANTSTemplateBuildSingleIterationWF

def init_workflow(magnitude_images, qsm_images):

    # workflow
    wf = pe.Workflow("workflow_template", base_dir=args.work_dir)

    # datasource
    datasource = pe.Node(
        interface=util.IdentityInterface(
            fields=['imageList', 'passiveImagesDictionariesList']
        ),
        run_without_submitting=True,
        name='InputImages'
    )
    datasource.inputs.imageList = magnitude_images
    datasource.inputs.passiveImagesDictionariesList = qsm_images
    datasource.inputs.sort_filelist = True

    # initial average
    initAvg = pe.Node(
        interface=ants.AverageImages(),
        name='initAvg'
    )
    initAvg.inputs.dimension = 3
    initAvg.inputs.normalize = True
    wf.connect([
        (datasource, initAvg, [('imageList', 'images')])
    ])

    # first iteration
    buildTemplateIteration1 = ANTSTemplateBuildSingleIterationWF('iteration01')
    wf.connect([
        (initAvg, buildTemplateIteration1, [('output_average_image', 'inputspec.fixed_image')]),
        (datasource, buildTemplateIteration1, [('imageList', 'inputspec.images')]),
        (datasource, buildTemplateIteration1, [('passiveImagesDictionariesList', 'inputspec.ListOfPassiveImagesDictionaries')]),
    ])
    BeginANTS1 = buildTemplateIteration1.get_node("BeginANTS")
    BeginANTS1.plugin_args = {
        'qsub_args': f'-A {args.qsub_account_string} -l walltime=04:00:00 -l select=1:ncpus=10:mem=8gb',
        'overwrite': True
    }

    # second iteration
    buildTemplateIteration2 = ANTSTemplateBuildSingleIterationWF('iteration02')
    wf.connect([
        (buildTemplateIteration1, buildTemplateIteration2, [('outputspec.template', 'inputspec.fixed_image')]),
        (datasource, buildTemplateIteration2, [('imageList', 'inputspec.images')]),
        (datasource, buildTemplateIteration2, [('passiveImagesDictionariesList', 'inputspec.ListOfPassiveImagesDictionaries')])
    ])
    BeginANTS2 = buildTemplateIteration2.get_node("BeginANTS")
    BeginANTS2.plugin_args = {
        'qsub_args': f'-A {args.qsub_account_string} -l walltime=04:00:00 -l select=1:ncpus=10:mem=8gb',
        'overwrite': True
    }

    # datasink
    datasink = pe.Node(
        io.DataSink(),
        name="datasink"
    )
    datasink.inputs.base_directory = os.path.join('out/test', "results")
    wf.connect([
        (buildTemplateIteration2, datasink, [('outputspec.template', 'PrimaryTemplate')]),
        (buildTemplateIteration2, datasink, [('outputspec.passive_deformed_templates', 'PassiveTemplate')]),
        (initAvg, datasink, [('output_average_image', 'PreRegisterAverage')]),
    ])

    return wf

if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="QSMxT Template: Magnitude and QSM template and group space generator",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        'bids_dir',
        help='Input data folder that can be created using run_1_dicomToBids.py; can also use a ' +
             'custom folder containing subject folders and NIFTI files or a BIDS folder with a ' +
             'different structure, as long as --subject_pattern, --session_pattern, ' +
             '--magnitude_pattern and --phase_pattern are also specified.'
    )

    parser.add_argument(
        'qsm_dir',
        type=str,
        help="the qsm output directory produced by ./run_2_qsm.py"
    )

    parser.add_argument(
        'out_dir',
        help='Output folder; will be created if it does not exist.'
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
        '--magnitude_pattern',
        default='{subject}/{session}/anat/*qsm*{run}*magnitude*nii*',
        help='Pattern to match magnitude files within the BIDS directory. ' +
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
        '--pbs',
        default=None,
        dest='qsub_account_string',
        help='Run the pipeline via PBS and use the argument as the QSUB account string.'
    )

    parser.add_argument(
        '--n_procs',
        type=int,
        default=None,
        help='Number of processes to run concurrently. By default, we use the number of CPUs, ' +
             'provided there are 8 GBs of RAM available for each.'
    )

    parser.add_argument(
        '--debug',
        action='store_true',
        help='Enables some nipype settings for debugging.'
    )
    
    args = parser.parse_args()
    
    # ensure directories are complete and absolute
    if not args.work_dir: args.work_dir = args.out_dir
    args.bids_dir = os.path.abspath(args.bids_dir)
    args.qsm_dir = os.path.abspath(args.qsm_dir)
    args.work_dir = os.path.abspath(args.work_dir)
    args.out_dir = os.path.abspath(args.out_dir)

    # set number of concurrent processes to run depending on
    # available CPUs and RAM (max 1 per 3 GB of available RAM)
    n_cpus = int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count())
    if not args.n_procs:
        available_ram_gb = psutil.virtual_memory().available / 1e9
        args.n_procs = min(int(available_ram_gb / 3), n_cpus)

    # find input images
    magnitude_pattern = os.path.join(args.bids_dir, args.magnitude_pattern.format(subject=args.subject_pattern, session=args.session_pattern, run='*'))
    qsm_pattern = os.path.join(args.qsm_dir, "qsm_final", "*", "*.nii*")
    magnitude_images = sorted(glob.glob(magnitude_pattern))
    qsm_images = sorted(glob.glob(qsm_pattern))

    
    if len(magnitude_images) != len(qsm_images):
        print(f"QSMxT: Error: Number of QSM images ({len(qsm_images)}) and magnitude images ({len(input_images)}) do not match.")
        exit()

    # convert qsm_images to dictionary
    qsm_images = [{'QSM' : x} for x in qsm_images]

    wf = init_workflow(magnitude_images, qsm_images)

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

