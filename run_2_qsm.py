#!/usr/bin/env python3

import os.path
import os
import glob
import psutil

from nipype.interfaces.fsl import BET, ImageMaths, ImageStats, MultiImageMaths, CopyGeom, Merge, UnaryMaths
from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import DataSink, DataGrabber
from nipype.pipeline.engine import Workflow, Node, MapNode

from interfaces import nipype_interface_selectfiles as sf
from interfaces import nipype_interface_tgv_qsm as tgv
from interfaces import nipype_interface_phaseweights as phaseweights
from interfaces import nipype_interface_bestlinreg as bestlinreg
from interfaces import nipype_interface_makehomogeneous as makehomogeneous
from interfaces import nipype_interface_nonzeroaverage as nonzeroaverage
from interfaces import nipype_interface_composite as composite

import argparse


def init_workflow():
    subjects = [
        os.path.split(path)[1]
        for path in glob.glob(os.path.join(args.bids_dir, args.subject_pattern))
        if not args.subjects or os.path.split(path)[1] in args.subjects
    ]
    if not subjects:
        print(f"No subjects found in: {os.path.join(args.bids_dir, args.session_pattern)}")
        exit()
    wf = Workflow("workflow_qsm", base_dir=args.work_dir)
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
    if not sessions:
        print(f"No sessions found in: {os.path.join(args.bids_dir, subject, args.session_pattern)}")
        exit()
    wf = Workflow(subject, base_dir=os.path.join(args.work_dir, "workflow_qsm"))
    wf.add_nodes([
        init_session_workflow(subject, session)
        for session in sessions
    ])
    return wf

def init_session_workflow(subject, session):
    wf = Workflow(session, base_dir=os.path.join(args.work_dir, "workflow_qsm", subject, session))

    # datasink
    n_datasink = Node(
        interface=DataSink(base_directory=args.out_dir),
        name='datasink'
    )

    # exit if no runs found
    phase_pattern = os.path.join(args.bids_dir, args.phase_pattern.replace("{run}", "").format(subject=subject, session=session))
    phase_files = glob.glob(phase_pattern)
    if not phase_files:
        print(f"No phase files found matching pattern: {phase_pattern}")
        exit()
    for phase_file in phase_files:
        if 'run-' not in phase_file:
            print(f"No 'run-' identifier found in file: {phase_file}")
            exit()

    # identify all runs
    runs = sorted(list(set([
        f"run-{os.path.split(path)[1][os.path.split(path)[1].find('run-') + 4: os.path.split(path)[1].find('_', os.path.split(path)[1].find('run-') + 4)]}"
        for path in phase_files
    ])))

    # iterate across each run
    n_runPatterns = Node(
        interface=IdentityInterface(
            fields=['run'],
        ),
        name="iterate_runs"
    )
    n_runPatterns.iterables = ('run', runs)

    # get relevant files from this run
    n_selectFiles = Node(
        interface=sf.SelectFiles(
            templates={
                'mag': args.magnitude_pattern.replace("{run}", "{{run}}").format(subject=subject, session=session),
                'phs': args.phase_pattern.replace("{run}", "{{run}}").format(subject=subject, session=session),
                'params': args.phase_pattern.replace("{run}", "{{run}}").replace("nii.gz", "nii").replace("nii", "json").format(subject=subject, session=session)
            },
            base_directory=os.path.abspath(args.bids_dir),
            sort_filelist=True,
            error_if_empty=False,
            num_files=args.num_echoes_to_process
        ),
        name='select_files'
    )
    wf.connect([
        (n_runPatterns, n_selectFiles, [('run', 'run')])
    ])

    # scale phase data
    mn_stats = MapNode(
        # -R : <min intensity> <max intensity>
        interface=ImageStats(op_string='-R'),
        iterfield=['in_file'],
        name='get_stats',
        # output: 'out_stat'
    )
    wf.connect([
        (n_selectFiles, mn_stats, [('phs', 'in_file')])
    ])
    def scale_to_pi(min_and_max):
        from math import pi

        min_value = min_and_max[0][0]
        max_value = min_and_max[0][1]
        fsl_cmd = ""

        # set range to [0, max-min]
        fsl_cmd += "-sub %.10f " % min_value
        max_value -= min_value
        min_value -= min_value

        # set range to [0, 2pi]
        fsl_cmd += "-div %.10f " % (max_value / (2*pi))

        # set range to [-pi, pi]
        fsl_cmd += "-sub %.10f" % pi
        return fsl_cmd
    mn_phase_scaled = MapNode(
        interface=ImageMaths(suffix="_scaled"),
        name='phase_scaled',
        iterfield=['in_file']
        # inputs: 'in_file', 'op_string'
        # output: 'out_file'
    )
    wf.connect([
        (n_selectFiles, mn_phase_scaled, [('phs', 'in_file')]),
        (mn_stats, mn_phase_scaled, [(('out_stat', scale_to_pi), 'op_string')])
    ])

    # read echotime and field strengths from json files
    def read_json(in_file):
        import os
        te = 0.001
        b0 = 7
        if os.path.exists(in_file):
            import json
            with open(in_file, 'rt') as fp:
                data = json.load(fp)
                te = data['EchoTime']
                b0 = data['MagneticFieldStrength']
        return te, b0
    mn_params = MapNode(
        interface=Function(
            input_names=['in_file'],
            output_names=['EchoTime', 'MagneticFieldStrength'],
            function=read_json
        ),
        iterfield=['in_file'],
        name='read_json'
    )
    wf.connect([
        (n_selectFiles, mn_params, [('params', 'in_file')])
    ])

    # homogeneity filter
    if args.inhomogeneity_correction:
        mn_inhomogeneity_correction = MapNode(
            interface=makehomogeneous.MakeHomogeneousInterface(),
            iterfield=['in_file'],
            name='correct_inhomogeneity'
            # output : out_file
        )
        wf.connect([
            (n_selectFiles, mn_inhomogeneity_correction, [('mag', 'in_file')])
        ])

    # brain extraction
    def repeat(in_file):
        return in_file
    mn_mask = MapNode(
        interface=Function(
            input_names=['in_file'],
            output_names=['mask_file'],
            function=repeat
        ),
        iterfield=['in_file'],
        name='repeat_mask'
    )
    if args.masking == 'bet' or args.add_bet:
        mn_bet = MapNode(
            interface=BET(frac=args.bet_fractional_intensity, mask=True, robust=True),
            iterfield=['in_file'],
            name='fsl_bet'
            # output: 'mask_file'
        )
        if args.inhomogeneity_correction:
            wf.connect([
                (mn_inhomogeneity_correction, mn_bet, [('out_file', 'in_file')])
            ])
        else:
            wf.connect([
                (n_selectFiles, mn_bet, [('mag', 'in_file')])
            ])

        if not args.add_bet:
            wf.connect([
                (mn_bet, mn_mask, [('mask_file', 'in_file')])
            ])
    if args.masking == 'phase-based':
        mn_phaseweights = MapNode(
            interface=phaseweights.PhaseWeightsInterface(),
            iterfield=['in_file'],
            name='phase_weights'
            # output: 'out_file'
        )
        wf.connect([
            (mn_phase_scaled, mn_phaseweights, [('out_file', 'in_file')]),
        ])

        mn_phasemask = MapNode(
            interface=ImageMaths(
                suffix='_mask',
                op_string=f'-thrp {args.threshold} -bin -ero -dilM'
            ),
            iterfield=['in_file'],
            name='phase_mask'
            # input  : 'in_file'
            # output : 'out_file'
        )
        wf.connect([
            (mn_phaseweights, mn_phasemask, [('out_file', 'in_file')])
        ])
        
        wf.connect([
            (mn_phasemask, mn_mask, [('out_file', 'in_file')])
        ])
    elif args.masking == 'magnitude-based':
        mn_magmask = MapNode(
            interface=ImageMaths(
                suffix="_mask",
                op_string=f"-thrp {args.threshold} -bin -ero -dilM"
            ),
            iterfield=['in_file'],
            name='magnitude_mask'
            # output: 'out_file'
        )

        if args.inhomogeneity_correction:
            wf.connect([
                (mn_inhomogeneity_correction, mn_magmask, [('out_file', 'in_file')])
            ])
        else:
            wf.connect([
                (n_selectFiles, mn_magmask, [('mag', 'in_file')])
            ])

        wf.connect([
            (mn_magmask, mn_mask, [('out_file', 'in_file')])
        ])
    
    # QSM reconstruction
    if args.two_pass or args.masking == 'bet':
        mn_qsm = MapNode(
            interface=tgv.QSMappingInterface(
                iterations=args.qsm_iterations,
                alpha=[0.0015, 0.0005],
                erosions=0 if args.masking in ['phase-based', 'magnitude-based'] else 5,
                num_threads=args.qsm_threads,
                out_suffix='_qsm',
                extra_arguments='--ignore-orientation --no-resampling' if args.no_resampling else ''
            ),
            iterfield=['phase_file', 'TE', 'b0', 'mask_file'],
            name='qsm'
            # output: 'out_file'
        )

        # args for PBS
        mn_qsm.plugin_args = {
            'qsub_args': f'-A {args.qsub_account_string} -l walltime=03:00:00 -l select=1:ncpus={args.qsm_threads}:mem=20gb:vmem=20gb',
            'overwrite': True
        }

        wf.connect([
            (mn_params, mn_qsm, [('EchoTime', 'TE')]),
            (mn_params, mn_qsm, [('MagneticFieldStrength', 'b0')]),
            (mn_mask, mn_qsm, [('mask_file', 'mask_file')]),
            (mn_phase_scaled, mn_qsm, [('out_file', 'phase_file')])
        ])

        # qsm averaging
        n_qsm_average = Node(
            interface=nonzeroaverage.NonzeroAverageInterface(),
            name='qsm_average'
            # input : in_files
            # output : out_file
        )
        wf.connect([
            (mn_qsm, n_qsm_average, [('out_file', 'in_files')])
        ])

        wf.connect([
            (n_qsm_average, n_datasink, [('out_file', 'qsm_average' if args.masking != 'bet' else 'qsm_final')]),
            (mn_qsm, n_datasink, [('out_file', 'qsms')]),
            (mn_mask, n_datasink, [('mask_file', 'masks')])
        ])
    if args.masking in ['phase-based', 'magnitude-based']:
        mn_mask_filled = MapNode(
            interface=ImageMaths(
                suffix='_fillh',
                op_string="-fillh" if not args.extra_fill_strength else " ".join(
                    ["-dilM" for f in range(args.extra_fill_strength)] 
                    + ["-fillh"] 
                    + ["-ero" for f in range(args.extra_fill_strength)]
                )
            ),
            iterfield=['in_file'],
            name='mask_filled'
        )

        if args.add_bet:
            mn_bet_erode = MapNode(
                interface=ImageMaths(
                    suffix='_ero',
                    op_string=f'-ero -ero'
                ),
                iterfield=['in_file'],
                name='fsl_bet_erode'
            )
            wf.connect([
                (mn_bet, mn_bet_erode, [('mask_file', 'in_file')])
            ])
            mn_mask_plus_bet = MapNode(
                interface=composite.CompositeNiftiInterface(),
                name='mask_plus_bet',
                iterfield=['in_file1', 'in_file2'],
            )
            wf.connect([
                (mn_mask, mn_mask_plus_bet, [('mask_file', 'in_file1')]),
                (mn_bet_erode, mn_mask_plus_bet, [('out_file', 'in_file2')])
            ])
            wf.connect([
                (mn_mask_plus_bet, mn_mask_filled, [('out_file', 'in_file')])
            ])
        else:
            wf.connect([
                (mn_mask, mn_mask_filled, [('mask_file', 'in_file')])
            ])

        wf.connect([
            (mn_mask_filled, n_datasink, [('out_file', 'masks_filled')])
        ])

        mn_qsm_filled = MapNode(
            interface=tgv.QSMappingInterface(
                iterations=args.qsm_iterations,
                alpha=[0.0015, 0.0005],
                erosions=0,
                num_threads=args.qsm_threads,
                out_suffix='_qsm-filled',
                extra_arguments='--ignore-orientation --no-resampling' if args.no_resampling else ''
            ),
            iterfield=['phase_file', 'TE', 'b0', 'mask_file'],
            name='qsm_filledmask'
            # inputs: 'phase_file', 'TE', 'b0', 'mask_file'
            # output: 'out_file'
        )

        # args for PBS
        mn_qsm_filled.plugin_args = {
            'qsub_args': f'-A {args.qsub_account_string} -l walltime=03:00:00 -l select=1:ncpus={args.qsm_threads}:mem=20gb:vmem=20gb',
            'overwrite': True
        }

        wf.connect([
            (mn_params, mn_qsm_filled, [('EchoTime', 'TE')]),
            (mn_params, mn_qsm_filled, [('MagneticFieldStrength', 'b0')]),
            (mn_mask_filled, mn_qsm_filled, [('out_file', 'mask_file')]),
            (mn_phase_scaled, mn_qsm_filled, [('out_file', 'phase_file')]),
        ])
        wf.connect([
            (mn_qsm_filled, n_datasink, [('out_file', 'qsms_filled')]),
        ])

        # qsm averaging
        n_qsm_filled_average = Node(
            interface=nonzeroaverage.NonzeroAverageInterface(),
            name='qsm_filledmask_average'
            # input : in_files
            # output : out_file
        )
        wf.connect([
            (mn_qsm_filled, n_qsm_filled_average, [('out_file', 'in_files')])
        ])
        wf.connect([
            (n_qsm_filled_average, n_datasink, [('out_file', 'qsm_filled_average' if args.two_pass else 'qsm_final')])
        ])

        # composite qsm
        if args.two_pass:
            mn_qsm_composite = MapNode(
                interface=composite.CompositeNiftiInterface(),
                name='qsm_composite',
                iterfield=['in_file1', 'in_file2', 'in_maskFile'],
            )
            wf.connect([
                (mn_qsm, mn_qsm_composite, [('out_file', 'in_file1')]),
                (mn_qsm_filled, mn_qsm_composite, [('out_file', 'in_file2')]),
                (mn_mask, mn_qsm_composite, [('mask_file', 'in_maskFile')])
            ])

            n_qsm_composite_average = Node(
                interface=nonzeroaverage.NonzeroAverageInterface(),
                name='qsm_composite_average'
                # input : in_files
                # output: out_file
            )
            wf.connect([
                (mn_qsm_composite, n_qsm_composite_average, [('out_file', 'in_files')])
            ])

            wf.connect([
                (mn_qsm_composite, n_datasink, [('out_file', 'qsms_composite')]),
                (n_qsm_composite_average, n_datasink, [('out_file', 'qsm_final')]),
            ])

    return wf


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="QSMxT qsm: QSM Reconstruction Pipeline",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        'bids_dir',
        help='Input data folder generated using run_1_dicomConvert.py; can also use a ' +
             'previously existing BIDS folder. Ensure that the --subject_pattern, '+
             '--session_pattern, --magnitude_pattern and --phase_pattern are correct.'
    )

    parser.add_argument(
        'out_dir',
        help='Output QSM folder; will be created if it does not exist.'
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
        default='{subject}/{session}/anat/*{run}*magnitude*nii*',
        help='Pattern to match magnitude files within the BIDS directory. ' +
             'The {subject}, {session} and {run} placeholders must be present.'
    )

    parser.add_argument(
        '--phase_pattern',
        default='{subject}/{session}/anat/*{run}*phase*nii*',
        help='Pattern to match phase files for qsm within session folders. ' +
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
        '--num_echoes',
        dest='num_echoes_to_process',
        default=None,
        type=int,
        help='The number of echoes to process; by default all echoes are processed.'
    )

    parser.add_argument(
        '--masking', '-m',
        default='magnitude-based',
        choices=['magnitude-based', 'phase-based', 'bet'],
        help='Masking strategy. Magnitude-based and phase-based masking generates a mask by ' +
             'thresholding a lower percentage of the histogram of the signal (adjust using the '+
             '--threshold parameter). For phase-based masking, the spatial phase coherence is '+
             'thresholded and the magnitude is not required. Using BET automatically disables '+
             'the two-pass inversion strategy for artefact mitigation.'
    )

    parser.add_argument(
        '--single_pass',
        action='store_true',
        help='Runs a single QSM inversion per echo, rather than the novel two-pass QSM inversion that '+
             'separates reliable and less reliable phase regions for artefact reduction. '+
             'Use this option to disable the novel inversion and approximately halve the runtime.'
    )

    parser.add_argument(
        '--qsm_iterations',
        type=int,
        default=1000,
        help='Number of iterations used for QSM reconstruction in tgv_qsm.'
    )

    parser.add_argument(
        '--inhomogeneity_correction',
        action='store_true',
        help='Applies an inomogeneity correction to the magnitude prior to masking'
    )

    parser.add_argument(
        '--threshold',
        type=int,
        default=30,
        help='Threshold percentage; anything less than the threshold will be excluded from the mask'
    )

    parser.add_argument(
        '--bet_fractional_intensity',
        type=float,
        default=0.5,
        help='Fractional intensity for BET masking operations.'
    )

    def positive_int(value):
        ivalue = int(value)
        if ivalue <= 0:
            raise argparse.ArgumentTypeError("%s is an invalid positive int value" % value)
        return ivalue

    parser.add_argument(
        '--extra_fill_strength',
        type=positive_int,
        default=0,
        help='Adds strength to hole-filling for phase-based and magnitude-based masking; ' +
             'each integer increment adds to the masking procedure one further dilation step ' +
             'prior to hole-filling, followed by an equal number of erosion steps.'
    )

    parser.add_argument(
        '--add_bet',
        action='store_true',
        help='When using magnitude or phase-based masking, this option adds a BET mask to the filled, '+
             'threshold-based mask. This is useful if areas of the sample are missing due to a failure '+
             'of the hole-filling algorithm.'
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
        help='Number of processes to run concurrently for MultiProc. By default, we use the number of CPUs, ' +
             'provided there are 6 GBs of RAM available for each.'
    )

    parser.add_argument(
        '--debug',
        action='store_true',
        help='Enables some nipype settings for debugging.'
    )
    
    parser.add_argument(
        '--no_resampling',
        action='store_true',
        help='Deactivate resampling inside TGV_QSM. Useful when resampling fails with error: ' +
             '\'Incompatible size of mask and data images\'. Check results carefully.'
    )

    args = parser.parse_args()
    
    # ensure directories are complete and absolute
    args.work_dir = args.out_dir
    args.bids_dir = os.path.abspath(args.bids_dir)
    args.work_dir = os.path.abspath(args.work_dir)
    args.out_dir = os.path.abspath(args.out_dir)

    # this script's directory
    this_dir = os.path.dirname(os.path.abspath(__file__))

    # misc environment variables
    os.environ["FSLOUTPUTTYPE"] = "NIFTI_GZ"

    # path environment variable
    os.environ["PATH"] += os.pathsep + os.path.join(this_dir, "scripts")

    # add this_dir and cwd to pythonpath (not sure if this_dir needed...)
    if "PYTHONPATH" in os.environ: os.environ["PYTHONPATH"] += os.pathsep + this_dir
    else:                          os.environ["PYTHONPATH"]  = this_dir

    # debug options
    if args.debug:
        from nipype import config
        config.enable_debug_mode()
        config.set('execution', 'stop_on_first_crash', 'true')
        config.set('execution', 'remove_unnecessary_outputs', 'false')
        config.set('execution', 'keep_inputs', 'true')
        config.set('logging', 'workflow_level', 'DEBUG')
        config.set('logging', 'interface_level', 'DEBUG')
        config.set('logging', 'utils_level', 'DEBUG')

    # add_bet option only works with non-bet masking methods
    args.add_bet = args.add_bet and args.masking != 'bet'
    args.two_pass = args.masking != 'bet' and not args.single_pass

    # decide on inhomogeneity correction
    args.inhomogeneity_correction = args.inhomogeneity_correction and (args.add_bet or 'phase-based' not in args.masking)

    # set number of QSM threads
    n_cpus = int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count())
    
    # set number of concurrent processes to run depending on
    # available CPUs and RAM (max 1 per 6 GB of available RAM)
    if not args.n_procs:
        available_ram_gb = psutil.virtual_memory().available / 1e9
        args.n_procs = min(int(available_ram_gb / 6), n_cpus)
        if not args.n_procs:
            print(f"Insufficient memory to run QSMxT ({available_ram_gb} GB available; 6 GB needed)")
        print("Running with", args.n_procs, "procesors")

    #qsm_threads should be set to adjusted n_procs (either computed earlier or given via cli)
    #args.qsm_threads = args.n_procs if not args.qsub_account_string else 1
    args.qsm_threads = 1#args.n_procs if not args.qsub_account_string else 1

    os.makedirs(os.path.abspath(args.work_dir), exist_ok=True)
    os.makedirs(os.path.abspath(args.out_dir), exist_ok=True)

    # make sure tgv_qsm is compiled on the target system before we start the pipeline:
    # process = subprocess.run(['tgv_qsm'])

    # run workflow
    #wf.write_graph(graph2use='flat', format='png', simple_form=False)
    wf = init_workflow()
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

