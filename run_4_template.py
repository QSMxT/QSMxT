#!/usr/bin/env python3
# Adapted from NiPype tutorial example for QSMxT

import os
import glob
import argparse
import psutil
import sys
import datetime
import datetime

import nipype.interfaces.utility as util
import nipype.interfaces.ants as ants
import nipype.interfaces.io as io
import nipype.pipeline.engine as pe

from scripts.antsBuildTemplate import ANTSTemplateBuildSingleIterationWF
from scripts.qsmxt_functions import get_qsmxt_version, get_diff
from scripts.logger import LogLevel, make_logger, show_warning_summary

def init_workflow(magnitude_images, qsm_images):

    # workflow
    wf = pe.Workflow("workflow_template", base_dir=args.work_dir)

    # datasource
    datasource = pe.Node(
        interface=util.IdentityInterface(
            fields=['magnitude_images', 'qsm_images', 'qsm_dict']
        ),
        run_without_submitting=True,
        name='nipype_InputImages'
    )
    datasource.inputs.magnitude_images = magnitude_images
    datasource.inputs.qsm_images = qsm_images
    datasource.inputs.qsm_dict = [{'QSM' : x} for x in qsm_images]
    datasource.inputs.sort_filelist = True

    # initial average
    initAvg = pe.Node(
        interface=ants.AverageImages(),
        name='ants_average-images'
    )
    initAvg.inputs.dimension = 3
    initAvg.inputs.normalize = True
    wf.connect([
        (datasource, initAvg, [('magnitude_images', 'images')])
    ])

    # first iteration
    buildTemplateIteration1 = ANTSTemplateBuildSingleIterationWF('iteration01')
    wf.connect([
        (initAvg, buildTemplateIteration1, [('output_average_image', 'inputspec.fixed_image')]),
        (datasource, buildTemplateIteration1, [('magnitude_images', 'inputspec.images')]),
        (datasource, buildTemplateIteration1, [('qsm_dict', 'inputspec.ListOfPassiveImagesDictionaries')]),
    ])
    BeginANTS1 = buildTemplateIteration1.get_node("BeginANTS")
    BeginANTS1.plugin_args = {
        'qsub_args': f'-A {args.pbs} -l walltime=04:00:00 -l select=1:ncpus=10:mem=8gb',
        'overwrite': True
    }

    # second iteration
    buildTemplateIteration2 = ANTSTemplateBuildSingleIterationWF('iteration02')
    wf.connect([
        (buildTemplateIteration1, buildTemplateIteration2, [('outputspec.template', 'inputspec.fixed_image')]),
        (datasource, buildTemplateIteration2, [('magnitude_images', 'inputspec.images')]),
        (datasource, buildTemplateIteration2, [('qsm_dict', 'inputspec.ListOfPassiveImagesDictionaries')])
    ])
    BeginANTS2 = buildTemplateIteration2.get_node("BeginANTS")
    BeginANTS2.plugin_args = {
        'qsub_args': f'-A {args.pbs} -l walltime=04:00:00 -l select=1:ncpus=10:mem=8gb',
        'overwrite': True
    }

    # datasink
    datasink = pe.Node(
        io.DataSink(base_directory=args.output_dir),
        name='nipype_datasink'
    )
    wf.connect([
        (initAvg, datasink, [('output_average_image', 'initial_average')]),
        (buildTemplateIteration2, datasink, [('outputspec.template', 'magnitude_template')]),
        (buildTemplateIteration2, datasink, [('outputspec.passive_deformed_templates', 'qsm_template')]),
        (buildTemplateIteration2, datasink, [('outputspec.flattened_transforms', 'transforms')]),
        (buildTemplateIteration2, datasink, [('outputspec.wimtPassivedeformed', 'qsms_transformed')])
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
        'output_dir',
        help='Output folder; will be created if it does not exist.'
    )

    parser.add_argument(
        '--work_dir',
        default=None,
        help='NiPype working directory; defaults to \'work\' within \'output_dir\'.'
    )

    parser.add_argument(
        '--qsm_pattern',
        default=os.path.join('qsm_final', '*', '*'),
        help='Pattern used to match QSM images in qsm_dir'
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
        '--pbs',
        default=None,
        dest='pbs',
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
    args.bids_dir = os.path.abspath(args.bids_dir)
    args.qsm_dir = os.path.abspath(args.qsm_dir)
    args.output_dir = os.path.abspath(args.output_dir)
    args.work_dir = os.path.abspath(args.work_dir) if args.work_dir else os.path.abspath(args.output_dir)

    os.makedirs(args.work_dir, exist_ok=True)
    os.makedirs(args.output_dir, exist_ok=True)

    # setup logger
    logger = make_logger(
        logpath=os.path.join(args.output_dir, f"log_{str(datetime.datetime.now()).replace(':', '-').replace(' ', '_').replace('.', '')}.txt"),
        printlevel=LogLevel.INFO,
        writelevel=LogLevel.INFO,
        warnlevel=LogLevel.WARNING,
        errorlevel=LogLevel.ERROR
    )

    logger.log(LogLevel.INFO.value, f"Running QSMxT {get_qsmxt_version()}")
    logger.log(LogLevel.INFO.value, f"Command: {str.join(' ', sys.argv)}")
    logger.log(LogLevel.INFO.value, f"Python interpreter: {sys.executable}")

    diff = get_diff()
    if diff:
        logger.log(LogLevel.WARNING.value, f"Working directory not clean! Writing diff to {os.path.join(args.output_dir, 'diff.txt')}...")
        diff_file = open(os.path.join(args.output_dir, "diff.txt"), "w")
        diff_file.write(diff)
        diff_file.close()

    # environment variables for multi-threading
    os.environ["OMP_NUM_THREADS"] = "6"
    os.environ["ITK_GLOBAL_DEFAULT_NUMBER_OF_THREADS"] = "6"

    # set number of concurrent processes to run depending on
    # available CPUs and RAM (max 1 per 3 GB of available RAM)
    n_cpus = int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count())
    if not args.n_procs:
        available_ram_gb = psutil.virtual_memory().available / 1e9
        args.n_procs = max(1, min(int(available_ram_gb / 3), n_cpus))
        if available_ram_gb < 3:
            logger.log(LogLevel.WARNING.value, f"Less than 3 GB of memory available ({available_ram_gb} GB). At least 3 GB is recommended. You may need to close background programs.")
        logger.log(LogLevel.INFO.value, f"Running with {args.n_procs} processors.")

    # find input images
    magnitude_pattern = os.path.join(args.bids_dir, args.magnitude_pattern.format(subject=args.subject_pattern, session=args.session_pattern, run='*'))
    qsm_pattern = os.path.join(args.qsm_dir, args.qsm_pattern)
    magnitude_images = sorted(glob.glob(magnitude_pattern))
    magnitude_images = [x for x in magnitude_images if 'echo-1' in x or '_T2starw' in x]
    qsm_images = sorted(glob.glob(qsm_pattern))

    if len(magnitude_images) != len(qsm_images):
        print(f"QSMxT: Error: Number of QSM images ({len(qsm_images)}) and magnitude images ({len(magnitude_images)}) do not match.")
        print(f"Final QSM pattern: {qsm_pattern}")
        print(f"Final magnitude pattern: {magnitude_pattern}")
        exit()

    wf = init_workflow(magnitude_images, qsm_images)

    # write "details_and_citations.txt" with the command used to invoke the script and any necessary citations
    with open(os.path.join(args.output_dir, "details_and_citations.txt"), 'w', encoding='utf-8') as f:
        # output QSMxT version, run command, and python interpreter
        f.write(f"QSMxT: {get_qsmxt_version()}")
        f.write(f"\nRun command: {str.join(' ', sys.argv)}")
        f.write(f"\nPython interpreter: {sys.executable}")

        f.write("\n\n == References ==")

        # qsmxt, nipype, ants
        f.write("\n\n - Stewart AW, Robinson SD, O'Brien K, et al. QSMxT: Robust masking and artifact reduction for quantitative susceptibility mapping. Magnetic Resonance in Medicine. 2022;87(3):1289-1300. doi:10.1002/mrm.29048")
        f.write("\n\n - Stewart AW, Bollman S, et al. QSMxT/QSMxT. GitHub; 2022. https://github.com/QSMxT/QSMxT")
        f.write("\n\n - Gorgolewski K, Burns C, Madison C, et al. Nipype: A Flexible, Lightweight and Extensible Neuroimaging Data Processing Framework in Python. Frontiers in Neuroinformatics. 2011;5. Accessed April 20, 2022. doi:10.3389/fninf.2011.00013")
        f.write("\n\n - Avants BB, Tustison NJ, Johnson HJ. Advanced Normalization Tools. GitHub; 2022. https://github.com/ANTsX/ANTs")
        f.write("\n\n")

    if args.pbs:
        wf.run(
            plugin='PBSGraph',
            plugin_args={
                'qsub_args': f'-A {args.pbs} -l walltime=00:30:00 -l select=1:ncpus=1:mem=5gb'
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

