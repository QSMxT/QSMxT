import os
import glob

from nipype.interfaces.io import DataSink
from nipype.pipeline.engine import Workflow, Node
from nipype.interfaces.utility import IdentityInterface, Function
import nipype.interfaces.ants as ants

from qsmxt.scripts.qsmxt_functions import gen_plugin_args
from qsmxt.scripts.antsBuildTemplate import ANTSTemplateBuildSingleIterationWF
from qsmxt.scripts.logger import LogLevel, make_logger

def get_matching_files(bids_dir, subject='*', session='*', suffixes=None, part=None, acq=None, run=None):
    pattern = f"{bids_dir}/{subject}/{session}/anat/{subject}_{session}*"
    if acq:
        pattern += f"acq-{acq}_*"
    if run:
        pattern += f"run-{run}_*"
    if part:
        pattern += f"part-{part}_*"
    if suffixes:
        matching_files = [glob.glob(f"{pattern}{suffix}.nii*") for suffix in suffixes]
    return sorted([item for sublist in matching_files for item in sublist])

def init_template_workflow(args):
    logger = make_logger('main')
    logger.log(LogLevel.INFO.value, f"Creating QSM/GRE template workflow...")

    subjects = [
        os.path.split(path)[1]
        for path in glob.glob(os.path.join(args.bids_dir, args.subject_pattern))
        if not args.subjects or os.path.split(path)[1] in args.subjects
    ]

    magnitude_files = []
    for subject in subjects:
        subject_magfiles = get_matching_files(args.bids_dir, subject=subject, session='*', suffixes=["T2starw", "MEGRE"], part="mag")
        subject_phasefiles = get_matching_files(args.bids_dir, subject=subject, session='*', suffixes=["T2starw", "MEGRE"], part="phase")
        if len(subject_magfiles) and len(subject_magfiles) != len(subject_phasefiles):
            logger.log(LogLevel.ERROR.value, f"Number of phase files does not match the number of magnitude files for {subject}! QSM template-building will not be possible.")
            return
        if len(subject_magfiles):
            magnitude_files.append(subject_magfiles[0])
    if not magnitude_files:
        logger.log(LogLevel.ERROR.value, "No GRE magnitude images found! Template-building will not be possible.")
        return
    
    params_files = [path.replace('.nii.gz', '.nii').replace('.nii', '.json') for path in subject_magfiles]

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

