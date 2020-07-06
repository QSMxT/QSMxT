#!/usr/bin/env python3

import os.path
import os
import glob
from nipype.interfaces.fsl import BET, ImageMaths, ImageStats, MultiImageMaths, CopyGeom, Merge, UnaryMaths
from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import SelectFiles, DataSink
from nipype.pipeline.engine import Workflow, Node, MapNode

import nipype_interface_tgv_qsm as tgv
import nipype_interface_romeo as romeo
import nipype_interface_bestlinreg as bestlinreg
import nipype_interface_applyxfm as applyxfm
import nipype_interface_makehomogeneous as makehomogeneous
import nipype_interface_nonzeroaverage as nonzeroaverage

import argparse


def create_qsm_workflow(
    subject_list,
    bids_dir,
    work_dir,
    out_dir,
    atlas_dir,
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
        (n_selectfiles, n_num_echoes, [('mag', 'in_')])
    ])

    # scale phase data
    mn_stats = MapNode(
        # -R : <min intensity> <max intensity>
        interface=ImageStats(op_string='-R'),
        iterfield=['in_file'],
        name='get_min_max',
        # output: 'out_stat'
    )
    mn_phs_range = MapNode(
        interface=ImageMaths(suffix="_scaled"),
        name='scale_phase',
        iterfield=['in_file']
        # inputs: 'in_file', 'op_string'
        # output: 'out_file'
    )

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

    wf.connect([
        (n_selectfiles, mn_stats, [('phs', 'in_file')]),
        (n_selectfiles, mn_phs_range, [('phs', 'in_file')]),
        (mn_stats, mn_phs_range, [(('out_stat', scale_to_pi), 'op_string')])
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

    def repeat(in_file):
        return in_file

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

    # brain extraction
    if 'bet' in masking:
        bet = MapNode(
            interface=BET(frac=0.4, mask=True, robust=True),
            iterfield=['in_file'],
            name='fsl_bet'
            # output: 'mask_file'
        )

        wf.connect([
            (n_mag, bet, [('out_file', 'in_file')])
        ])

        mn_mask = MapNode(
            interface=Function(
                input_names=['in_file'],
                output_names=['mask_file'],
                function=repeat
            ),
            iterfield=['in_file'],
            name='repeat_mask'
        )
        wf.connect([
            (bet, mn_mask, [('mask_file', 'in_file')])
        ])
    elif masking == 'romeo':
        # ROMEO only operates on stacked .nii files
        n_stacked_phase = Node(
            interface=Merge(
                dimension='t',
                output_type='NIFTI'
            ),
            name="stack_phase",
            iterfield=['in_files']
            # output: 'merged_file'
        )
        wf.connect([
            (n_selectfiles, n_stacked_phase, [('phs', 'in_files')])
        ])

        n_romeo = Node(
            interface=romeo.RomeoInterface(
                weights_threshold=200
            ),
            iterfield=['in_file', 'echo_times'],
            name='romeo_mask'
            # output: 'out_file'
        )
        wf.connect([
            (n_stacked_phase, n_romeo, [('merged_file', 'in_file')]),
            (mn_params, n_romeo, [('EchoTime', 'echo_times')])
        ])

        n_romeo_erode = Node(
            interface=ImageMaths(
                suffix='_ero',
                op_string='-ero'
            ),
            name='romeo_ero'
            # input  : 'in_file'
            # output : 'out_file'
        )
        wf.connect([
            (n_romeo, n_romeo_erode, [('out_file', 'in_file')])
        ])
        n_romeo_dilate = Node(
            interface=ImageMaths(
                suffix='_dil',
                op_string='-dilM'
            ),
            name='romeo_ero_dil'
            # input  : 'in_file'
            # output : 'out_file'
        )
        wf.connect([
            (n_romeo_erode, n_romeo_dilate, [('out_file', 'in_file')])
        ])
        n_romeo_fillh = Node(
            interface=ImageMaths(
                suffix='_fillh',
                op_string='-fillh'
            ),
            name='romeo_ero_dil_fillh'
            # input  : 'in_file'
            # output : 'out_file'
        )
        wf.connect([
            (n_romeo_dilate, n_romeo_fillh, [('out_file', 'in_file')])
        ])

        mn_mask = MapNode(
            interface=Function(
                input_names=['in_file'],
                output_names=['mask_file'],
                function=repeat
            ),
            iterfield=['in_file'],
            name='repeat_mask'
        )
        wf.connect([
            (n_romeo_fillh, mn_mask, [('out_file', 'in_file')])
        ])
    elif masking == 'atlas-based':
        n_selectatlas = Node(
            interface=SelectFiles(
                templates={
                    'template': '*template*',
                    'mask': '*mask*'
                },
                base_directory=atlas_dir
            ),
            name='get_atlas'
            # output: ['template', 'mask']
        )

        mn_bestlinreg = MapNode(
            interface=bestlinreg.NiiBestLinRegInterface(),
            iterfield=['in_fixed', 'in_moving'],
            name='get_atlas_registration'
            # output: out_transform
        )

        wf.connect([
            (n_mag, mn_bestlinreg, [('out_file', 'in_fixed')]),
            (n_selectatlas, mn_bestlinreg, [('template', 'in_moving')])
        ])

        mn_applyxfm = MapNode(
            interface=applyxfm.NiiApplyMincXfmInterface(),
            iterfield=['in_file', 'in_like', 'in_transform'],
            name='apply_atlas_registration'
            # output: out_file
        )

        wf.connect([
            (n_selectatlas, mn_applyxfm, [('mask', 'in_file')]),
            (n_mag, mn_applyxfm, [('out_file', 'in_like')]),
            (mn_bestlinreg, mn_applyxfm, [('out_transform', 'in_transform')])
        ])

        mn_mask = MapNode(
            interface=Function(
                input_names=['in_file'],
                output_names=['mask_file'],
                function=repeat
            ),
            iterfield=['in_file'],
            name='repeat_mask'
        )
        wf.connect([
            (mn_applyxfm, mn_mask, [('out_file', 'in_file')])
        ])
    elif masking == 'composite':
        raise NotImplementedError
    
    # qsm processing
    mn_qsm_iterfield = ['phase_file', 'TE', 'b0']
    
    # if using a multi-echo masking method, add mask_file to iterfield
    if masking not in ['bet-firstecho', 'bet-lastecho', 'romeo']: mn_qsm_iterfield.append('mask_file')
    if masking == 'romeo': mn_qsm_iterfield.append('erosions')

    mn_qsm = MapNode(
        interface=tgv.QSMappingInterface(
            iterations=1000,
            alpha=[0.0015, 0.0005],
            erosions=2 if masking == 'romeo' else 5,
            num_threads=qsm_threads
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
        (mn_phs_range, mn_qsm, [('out_file', 'phase_file')])
    ])

    if masking == 'romeo':
        def get_erosions(num_echoes):
            return [5+x for x in range(num_echoes)]

        n_erosions = Node(
            interface=Function(
                input_names=['num_echoes'],
                output_names=['num_erosions'],
                function=get_erosions
            ),
            name='num_erosions'
        )

        wf.connect([
            (n_num_echoes, n_erosions, [('num_echoes', 'num_echoes')]),
            (n_erosions, mn_qsm, [('num_erosions', 'erosions')])
        ])

    # qsm averaging
    n_final_qsm = Node(
        interface=nonzeroaverage.NonzeroAverageInterface(),
        name='average_qsm'
        # input : in_files
        # output : out_file
    )
    wf.connect([
        (mn_qsm, n_final_qsm, [('out_file', 'in_files')])
    ])

    # datasink
    n_datasink = Node(
        interface=DataSink(base_directory=bids_dir, container=out_dir),
        name='datasink'
    )

    wf.connect([
        (n_final_qsm, n_datasink, [('out_file', 'qsm_final')]),
        (mn_qsm, n_datasink, [('out_file', 'qsm_singleEchoes')]),
        (mn_mask, n_datasink, [('mask_file', 'mask_singleEchoes')])
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
        choices=['bet-multiecho', 'bet-firstecho', 'bet-lastecho', 'romeo', 'atlas-based', 'composite'],
        help='masking strategy'
    )

    parser.add_argument(
        "--hf",
        dest='homogeneity_filter',
        action='store_true',
        help='disables magnitude homogeneity filter for bet; enables homogeneity filter for other masking strategies'
    )

    parser.add_argument(
        '--atlas_dir',
        default=os.path.join(
            os.path.dirname(os.path.abspath(__file__)),
            "atlas"
        ),
        const=os.path.join(
            os.path.dirname(os.path.abspath(__file__)),
            "atlas"
        ),
        nargs='?',
        help='atlas directory',
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
        'params': '{subject_id_p}/anat/*gre*phase*.json'
    }
    if 'bet-firstecho' in args.masking:
        bids_templates['mag'] = bids_templates['mag'].replace('gre*', 'gre*E01*')
    if 'bet-lastecho' in args.masking:
        num_echoes = len(sorted(glob.glob(os.path.join(glob.glob(os.path.join(args.bids_dir, "sub") + "*")[0], 'anat/') + "*gre*magnitude*.nii.gz")))
        bids_templates['mag'] = bids_templates['mag'].replace('gre*', f'gre*E{num_echoes:02}*')

    wf = create_qsm_workflow(
        subject_list=subject_list,
        bids_dir=os.path.abspath(args.bids_dir),
        work_dir=os.path.abspath(args.work_dir),
        out_dir=os.path.abspath(args.out_dir),
        masking=args.masking,
        atlas_dir=os.path.abspath(args.atlas_dir),
        bids_templates=bids_templates,
        homogeneity_filter=homogeneity_filter != args.homogeneity_filter,
        qsm_threads=16 if args.pbs else 1
    )

    os.makedirs(os.path.abspath(args.work_dir), exist_ok=True)
    os.makedirs(os.path.abspath(args.out_dir), exist_ok=True)

    # run workflow
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

    #wf.write_graph(graph2use='flat', format='png', simple_form=False)
    #wf.run(plugin='PBS', plugin_args={'-A UQ-CAI -l nodes=1:ppn=16,mem=5gb,vmem=5gb, walltime=30:00:00'})
