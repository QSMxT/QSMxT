#!/usr/bin/env python3

import os.path
import os
import glob
from nipype.interfaces.fsl import BET, ImageMaths, ImageStats, MultiImageMaths, CopyGeom, Merge, UnaryMaths
from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import SelectFiles, DataSink
from nipype.pipeline.engine import Workflow, Node, MapNode

import nipype_interface_tgv_qsm as tgv
import nipype_interface_phaseweights as phaseweights
import nipype_interface_bestlinreg as bestlinreg
import nipype_interface_makehomogeneous as makehomogeneous
import nipype_interface_nonzeroaverage as nonzeroaverage
import nipype_interface_composite as composite

import argparse


def create_qsm_workflow(
    subject_list,
    bids_dir,
    work_dir,
    out_dir,
    bids_templates,
    masking='bet-multiecho',
    homogeneity_filter=True,
    qsm_threads=1
):

    # create initial workflow
    wf = Workflow(name='qsm', base_dir=work_dir)

    # use infosource to iterate workflow across subject list
    n_infosource = Node(
        interface=IdentityInterface(
            fields=['subject_id']
        ),
        name="subject_source"
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
        name='get_subject_data'
        # output: ['mag', 'phs', 'params']
    )
    wf.connect([
        (n_infosource, n_selectfiles, [('subject_id', 'subject_id_p')])
    ])

    # count the number of echoes
    def get_length(in_):
        return len(in_)
    n_num_echoes = Node(
        interface=Function(
            input_names=['in_'],
            output_names=['num_echoes'],
            function=get_length
        ),
        iterfield=['in_'],
        name='get_num_echoes'
    )
    wf.connect([
        (n_selectfiles, n_num_echoes, [('phs', 'in_')])
    ])

    # scale phase data
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

    mn_stats = MapNode(
        # -R : <min intensity> <max intensity>
        interface=ImageStats(op_string='-R'),
        iterfield=['in_file'],
        name='get_stats',
        # output: 'out_stat'
    )
    wf.connect([
        (n_selectfiles, mn_stats, [('phs', 'in_file')])
    ])

    mn_phase_scaled = MapNode(
        interface=ImageMaths(suffix="_scaled"),
        name='phase_scaled',
        iterfield=['in_file']
        # inputs: 'in_file', 'op_string'
        # output: 'out_file'
    )
    wf.connect([
        (n_selectfiles, mn_phase_scaled, [('phs', 'in_file')]),
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
        (n_selectfiles, mn_params, [('params', 'in_file')])
    ])
    n_params_e01 = Node(
        interface=Function(
            input_names=['in_file'],
            output_names=['EchoTime', 'MagneticFieldStrength'],
            function=read_json
        ),
        name='read_json1'
    )
    wf.connect([
        (n_selectfiles, n_params_e01, [('params1', 'in_file')])
    ])

    def repeat(in_file):
        return in_file


    # brain extraction
    mn_mask = MapNode(
        interface=Function(
            input_names=['in_file'],
            output_names=['mask_file'],
            function=repeat
        ),
        iterfield=['in_file'],
        name='repeat_mask'
    )

    if 'bet' in masking:
        # homogeneity filter
        n_mag = MapNode(
            interface=Function(
                input_names=['in_file'],
                output_names=['out_file'],
                function=repeat
            ),
            iterfield=['in_file'],
            name='repeat_magnitude'
        )
        if homogeneity_filter:
            mn_homogeneity_filter = MapNode(
                interface=makehomogeneous.MakeHomogeneousInterface(),
                iterfield=['in_file'],
                name='make_homogeneous'
                # output : out_file
            )
            wf.connect([
                (n_selectfiles, mn_homogeneity_filter, [('mag', 'in_file')]),
                (mn_homogeneity_filter, n_mag, [('out_file', 'in_file')])
            ])
        else:
            wf.connect([
                (n_selectfiles, n_mag, [('mag', 'in_file')])
            ])

        bet = MapNode(
            interface=BET(frac=0.4, mask=True, robust=True),
            iterfield=['in_file'],
            name='fsl_bet'
            # output: 'mask_file'
        )
        wf.connect([
            (n_mag, bet, [('out_file', 'in_file')])
        ])

        wf.connect([
            (bet, mn_mask, [('mask_file', 'in_file')])
        ])
    elif masking == 'phase-based':
        # per-echo phase-based masks
        mn_phaseweights = MapNode(
            interface=phaseweights.PhaseWeightsInterface(),
            iterfield=['in_file'],
            name='phase_weights'
            # output: 'out_file'
        )
        wf.connect([
            (n_selectfiles, mn_phaseweights, [('phs', 'in_file')]),
        ])

        mn_phasemask = MapNode(
            interface=ImageMaths(
                suffix='_mask',
                op_string='-thrp 40 -bin -ero -dilM'
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
    elif masking == 'magnitude-based':
        # per-echo magnitude-based masks
        mn_magmask = MapNode(
            interface=ImageMaths(
                suffix="_mask",
                op_string="-thrp 40 -bin"
            ),
            iterfield=['in_file'],
            name='magnitude_mask'
            # output: 'out_file'
        )
        wf.connect([
            (n_selectfiles, mn_magmask, [('mag', 'in_file')])
        ])

        wf.connect([
            (mn_magmask, mn_mask, [('out_file', 'in_file')])
        ])

    # qsm processing
    mn_qsm_iterfield = ['phase_file', 'TE', 'b0']
    
    # if using a multi-echo masking method, add mask_file to iterfield
    if masking not in ['bet-firstecho', 'bet-lastecho']: mn_qsm_iterfield.append('mask_file')

    mn_qsm = MapNode(
        interface=tgv.QSMappingInterface(
            iterations=1000,
            alpha=[0.0015, 0.0005],
            erosions=0 if masking in ['phase-based', 'magnitude-based'] else 5,
            num_threads=qsm_threads,
            out_suffix='_qsm'
        ),
        iterfield=mn_qsm_iterfield,
        name='qsm'
        # output: 'out_file'
    )

    # args for PBS
    mn_qsm.plugin_args = {
        'qsub_args': f'-A UQ-CAI -q Short -l nodes=1:ppn={qsm_threads},mem=20gb,vmem=20gb,walltime=03:00:00',
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

    # datasink
    n_datasink = Node(
        interface=DataSink(base_directory=bids_dir, container=out_dir),
        name='datasink'
    )

    wf.connect([
        (n_qsm_average, n_datasink, [('out_file', 'qsm_average')]),
        (mn_qsm, n_datasink, [('out_file', 'qsm')]),
        (mn_mask, n_datasink, [('mask_file', 'mask')])
    ])

    if masking in ['phase-based', 'magnitude-based']:
        mn_mask_filled = MapNode(
            interface=ImageMaths(
                suffix='_fillh',
                op_string='-fillh'
            ),
            iterfield=['in_file'],
            name='mask_filled'
        )
        wf.connect([
            (mn_mask, mn_mask_filled, [('mask_file', 'in_file')])
        ])

        mn_qsm_filled = MapNode(
            interface=tgv.QSMappingInterface(
                iterations=1000,
                alpha=[0.0015, 0.0005],
                erosions=0,
                num_threads=qsm_threads,
                out_suffix='_qsm-filled'
            ),
            iterfield=['phase_file', 'TE', 'b0', 'mask_file'],
            name='qsm_filledmask'
            # inputs: 'phase_file', 'TE', 'b0', 'mask_file'
            # output: 'out_file'
        )

        # args for PBS
        mn_qsm_filled.plugin_args = {
            'qsub_args': f'-A UQ-CAI -q Short -l nodes=1:ppn={qsm_threads},mem=20gb,vmem=20gb,walltime=03:00:00',
            'overwrite': True
        }

        wf.connect([
            (mn_params, mn_qsm_filled, [('EchoTime', 'TE')]),
            (mn_params, mn_qsm_filled, [('MagneticFieldStrength', 'b0')]),
            (mn_mask_filled, mn_qsm_filled, [('out_file', 'mask_file')]),
            (mn_phase_scaled, mn_qsm_filled, [('out_file', 'phase_file')])
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

        # composite qsm
        n_composite = Node(
            interface=composite.CompositeNiftiInterface(),
            name='qsm_composite'
        )
        wf.connect([
            (n_qsm_average, n_composite, [('out_file', 'in_file1')]),
            (n_qsm_filled_average, n_composite, [('out_file', 'in_file2')])
        ])

        wf.connect([
            (n_qsm_filled_average, n_datasink, [('out_file', 'qsm_average_filled-mask')]),
            (n_composite, n_datasink, [('out_file', 'qsm_composite')]),
            (mn_qsm_filled, n_datasink, [('out_file', 'qsm_filled-mask')])
        ])


    return wf


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="QSM processing pipeline",
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

    parser.add_argument(
        '--masking',
        default='bet-multiecho',
        const='bet-multiecho',
        nargs='?',
        choices=['bet-multiecho', 'bet-firstecho', 'bet-lastecho', 'phase-based', 'magnitude-based'],
        help='masking strategy'
    )

    parser.add_argument(
        "--hf",
        dest='homogeneity_filter',
        action='store_true',
        help='disables magnitude homogeneity filter for bet; enables homogeneity filter for other masking strategies'
    )

    parser.add_argument(
        "--threshold",
        type=int,
        default=30,
        help='robust threshold used for magnitude-based and phase-based masking'
    )

    parser.add_argument(
        '--pbs',
        dest='pbs',
        action='store_true',
        help='use PBS graph'
    )

    args = parser.parse_args()

    # environment variables
    os.environ["FSLOUTPUTTYPE"] = "NIFTI_GZ"
    os.environ["PATH"] += os.pathsep + os.path.join(os.path.dirname(os.path.abspath(__file__)), "scripts")

    this_dir = os.path.dirname(os.path.abspath(__file__))
    if "PYTHONPATH" in os.environ: os.environ["PYTHONPATH"] += os.pathsep + this_dir
    else:                          os.environ["PYTHONPATH"]  = this_dir

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

    # default homogeneity filter setting: on for BET, off for everything else
    homogeneity_filter = 'bet' in args.masking

    bids_templates = {
        'mag': '{subject_id_p}/anat/*gre*magnitude*.nii.gz',
        'phs': '{subject_id_p}/anat/*gre*phase*.nii.gz',
        'phs1': '{subject_id_p}/anat/*gre*E01*phase.nii.gz',
        'mag1': '{subject_id_p}/anat/*gre*E01*magnitude.nii.gz',
        'params': '{subject_id_p}/anat/*gre*phase*.json',
        'params1': '{subject_id_p}/anat/*gre*E01*phase*.json'
    }
    num_echoes = len(sorted(glob.glob(os.path.join(glob.glob(os.path.join(args.bids_dir, "sub") + "*")[0], 'anat/') + "*gre*magnitude*.nii.gz")))
    if 'bet-firstecho' in args.masking:
        bids_templates['mag'] = bids_templates['mag'].replace('gre*', 'gre*E01*')
    if 'bet-lastecho' in args.masking:
        bids_templates['mag'] = bids_templates['mag'].replace('gre*', f'gre*E{num_echoes:02}*')
    if 'phase-based' in args.masking:
        del bids_templates['mag']
    if num_echoes == 1:
        bids_templates['mag1'] = bids_templates['mag']
        bids_templates['phs1'] = bids_templates['phs']
        bids_templates['params1'] = bids_templates['params']

    wf = create_qsm_workflow(
        subject_list=subject_list,
        bids_dir=os.path.abspath(args.bids_dir),
        work_dir=os.path.abspath(args.work_dir),
        out_dir=os.path.abspath(args.out_dir),
        masking=args.masking,
        bids_templates=bids_templates,
        homogeneity_filter=homogeneity_filter != args.homogeneity_filter,
        qsm_threads=16 if args.pbs else 1
    )

    os.makedirs(os.path.abspath(args.work_dir), exist_ok=True)
    os.makedirs(os.path.abspath(args.out_dir), exist_ok=True)

    # run workflow
    wf.write_graph(graph2use='flat', format='png', simple_form=False)
    if args.pbs:
        wf.run(
            plugin='PBSGraph',
            plugin_args={
                'qsub_args': '-A UQ-CAI -q Short -l nodes=1:ppn=1,mem=5GB,vmem=5GB,walltime=00:30:00'
            }
        )
    else:
        wf.run(
            plugin='MultiProc',
            plugin_args={
                'n_procs': int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count())
            }
        )

    #wf.run(plugin='PBS', plugin_args={'-A UQ-CAI -l nodes=1:ppn=16,mem=5gb,vmem=5gb, walltime=30:00:00'})
