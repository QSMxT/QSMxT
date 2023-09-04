import os
import glob
import re

from nipype.interfaces.io import DataSink
from nipype.pipeline.engine import Workflow, Node
from nipype.interfaces.utility import IdentityInterface, Function
import nipype.interfaces.ants as ants

from qsmxt.scripts.qsmxt_functions import gen_plugin_args
from qsmxt.scripts.antsBuildTemplate import ANTSTemplateBuildSingleIterationWF
from qsmxt.scripts.logger import LogLevel, make_logger

def get_matching_files(bids_dir, subject, dtype="anat", suffixes=[], ext="nii*", session=None, run=None, part=None, acq=None):
    pattern = os.path.join(bids_dir, subject)
    if session:
        pattern = os.path.join(pattern, session)
    pattern = os.path.join(pattern, dtype) + os.path.sep
    if acq:
        pattern += f"*acq-{acq}_*"
    if run:
        pattern += f"*run-{run}_*"
    if part:
        pattern += f"*part-{part}_*"
    dir, fname = os.path.split(pattern)
    if suffixes:
        if fname:
            matching_files = [glob.glob(f"{pattern}_{suffix}.{ext}") for suffix in suffixes]
        else:
            matching_files = [glob.glob(os.path.join(dir, f"*{suffix}.{ext}")) for suffix in suffixes]
    else:
        matching_files = [glob.glob(f"{pattern}.{ext}")]
    return sorted([item for sublist in matching_files for item in sublist])

def init_template_workflow(args):
    logger = make_logger('main')
    logger.log(LogLevel.INFO.value, f"Creating QSM/GRE template workflow...")

    subjects = [
        os.path.split(path)[1]
        for path in glob.glob(os.path.join(args.bids_dir, "sub*"))
        if not args.subjects or os.path.split(path)[1] in args.subjects
    ]

    magnitude_files = []

    for subject in subjects:
        sessions = []
        if not glob.glob(os.path.join(args.bids_dir, subject, "ses*")):
            sessions = [None]
        else:
            sessions = [
                os.path.split(path)[1]
                for path in sorted(glob.glob(os.path.join(args.bids_dir, subject, "ses*")))
                if not args.sessions or os.path.split(path)[1] in args.sessions
            ]

        for session in sessions:
            phase_pattern = os.path.join(args.bids_dir, os.path.join(subject, session) if session else subject, "anat", f"sub-*_part-phase*.nii*")
            files = sorted(glob.glob(phase_pattern))

            if not files:
                logger.log(LogLevel.WARNING.value, f"No files found matching pattern: {phase_pattern}")
                continue

            run_details = {}

            # get all unique acquisition names
            acquisitions = sorted(list(set([
                re.search("_acq-([a-zA-Z0-9]+)_", path).group(1)
                for path in files
                if '_acq-' in path
            ])))

            # handle multiple runs associated with each acquisition name
            for acquisition in acquisitions:
                acquisition_runs = sorted(list(set([
                    re.search("_run-([a-zA-Z0-9]+)_", path).group(1)
                    for path in files
                    if acquisition in path
                    and '_run-' in path
                ])))
                run_details[acquisition] = acquisition_runs if len(acquisition_runs) else None

            # handle runs not associated with any acquisition name
            other_runs = sorted(list(set([
                re.search("_run-([a-zA-Z0-9]+)_", path).group(1)
                for path in files
                if '_acq-' not in path
                and '_run-' in path
            ])))
            if other_runs:
                run_details[None] = other_runs

            # handle the final case where there may be no run or acquisition identifier
            others = any(all(x not in path for x in ['_acq-', '_run-']) for path in files)
            if others:
                if None in run_details.keys(): run_details[None].append(None)
                run_details[None] = [None]

            if not run_details:
                continue

            for acq, runs in (run_details.items() if run_details else [(None, [None])]):
                for run in (runs if runs is not None else [None]):
                    run_magfiles = get_matching_files(args.bids_dir, subject=subject, session=session, acq=acq, run=run, dtype="anat", part="mag")
                    if run_magfiles:
                        magnitude_files.append(run_magfiles[0])

    if not magnitude_files:
        logger.log(LogLevel.ERROR.value, "No GRE magnitude images found! Template-building will not be possible.")
        return
    
    params_files = [path.replace('.nii.gz', '.nii').replace('.nii', '.json') for path in magnitude_files]

    wf = Workflow("template", base_dir=os.path.join(args.output_dir, "workflow", "template"))

    n_inputs = Node(
        IdentityInterface(
            fields=['magnitude', 'qsm', 'params']
        ),
        name='template_inputs'
    )
    n_inputs.inputs.magnitude = magnitude_files
    n_inputs.inputs.params = params_files
    n_inputs.inputs.sort_filelist = True

    n_outputs = Node(
        interface=IdentityInterface(
            fields=['initial_average', 'magnitude_template', 'qsm_template', 'transforms', 'qsms_transformed']
        ),
        name='template_outputs'
    )
    n_datasink = Node(
        interface=DataSink(base_directory=args.output_dir),
        name='template_datasink'
    )
    wf.connect([
        (n_outputs, n_datasink, [('initial_average', 'template.initial_average')]),
        (n_outputs, n_datasink, [('magnitude_template', 'template.magnitude_template')]),
        (n_outputs, n_datasink, [('qsm_template', 'template.qsm_template')]),
        (n_outputs, n_datasink, [('transforms', 'template.transforms')]),
        (n_outputs, n_datasink, [('qsms_transformed', 'template.qsms_transformed')])
    ])

    n_qsmdict = Node(
        interface=Function(
            input_names=['qsm'],
            output_names=['qsm_dict'],
            function=lambda qsm: [{'QSM': x} for x in qsm]
        ),
        name='func_create-qsm-dict'
    )
    wf.connect([
        (n_inputs, n_qsmdict, [('qsm', 'qsm')])
    ])

    # initial average
    initAvg = Node(
        interface=ants.AverageImages(),
        name='ants_average-images'
    )
    initAvg.inputs.dimension = 3
    initAvg.inputs.normalize = True
    wf.connect([
        (n_inputs, initAvg, [('magnitude', 'images')])
    ])

    # first iteration
    buildTemplateIteration1 = ANTSTemplateBuildSingleIterationWF('iteration01')
    wf.connect([
        (initAvg, buildTemplateIteration1, [('output_average_image', 'inputspec.fixed_image')]),
        (n_inputs, buildTemplateIteration1, [('magnitude', 'inputspec.images')]),
        (n_qsmdict, buildTemplateIteration1, [('qsm_dict', 'inputspec.ListOfPassiveImagesDictionaries')]),
    ])
    n_ants_threads = min(6, args.n_procs) if args.multiproc else 6
    n_ants_mem_gb = min(8, args.mem_avail) if args.multiproc else 8
    BeginANTS1 = buildTemplateIteration1.get_node("BeginANTS")
    BeginANTS1.plugin_args = gen_plugin_args(
        plugin_args={ 'overwrite': True },
        slurm_account=args.slurm[0],
        pbs_account=args.pbs,
        slurm_partition=args.slurm[1],
        name="ANTS",
        time="04:00:00",
        mem_gb=n_ants_mem_gb,
        num_cpus=n_ants_threads
    )
    BeginANTS1.environ = {
        'OMP_NUM_THREADS': str(n_ants_threads),
        'ITK_GLOBAL_DEFAULT_NUMBER_OF_THREADS': str(n_ants_threads)
    }

    # second iteration
    buildTemplateIteration2 = ANTSTemplateBuildSingleIterationWF('iteration02')
    wf.connect([
        (buildTemplateIteration1, buildTemplateIteration2, [('outputspec.template', 'inputspec.fixed_image')]),
        (n_inputs, buildTemplateIteration2, [('magnitude', 'inputspec.images')]),
        (n_qsmdict, buildTemplateIteration2, [('qsm_dict', 'inputspec.ListOfPassiveImagesDictionaries')])
    ])
    BeginANTS2 = buildTemplateIteration2.get_node("BeginANTS")
    BeginANTS2.plugin_args = gen_plugin_args(
        plugin_args={ 'overwrite': True },
        slurm_account=args.slurm[0],
        pbs_account=args.pbs,
        slurm_partition=args.slurm[1],
        name="ANTS",
        time="04:00:00",
        mem_gb=n_ants_mem_gb,
        num_cpus=n_ants_threads
    )
    BeginANTS2.environ = {
        'OMP_NUM_THREADS': str(n_ants_threads),
        'ITK_GLOBAL_DEFAULT_NUMBER_OF_THREADS': str(n_ants_threads)
    }

    # datasink
    wf.connect([
        (initAvg, n_outputs, [('output_average_image', 'initial_average')]),
        (buildTemplateIteration2, n_outputs, [('outputspec.template', 'magnitude_template')]),
        (buildTemplateIteration2, n_outputs, [('outputspec.passive_deformed_templates', 'qsm_template')]),
        (buildTemplateIteration2, n_outputs, [('outputspec.flattened_transforms', 'transforms')]),
        (buildTemplateIteration2, n_outputs, [('outputspec.wimtPassivedeformed', 'qsms_transformed')])
    ])

    return wf

