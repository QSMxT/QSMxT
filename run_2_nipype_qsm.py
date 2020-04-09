#!/usr/bin/env python3

import os.path
import os
from nipype.interfaces.fsl import BET, ImageMaths, ImageStats, MultiImageMaths, CopyGeom, Merge
from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import SelectFiles, DataSink
from nipype.pipeline.engine import Workflow, Node, MapNode
import nipype_interface_tgv_qsm as tgv
import nipype_interface_romeo as romeo
import argparse

def create_qsm_workflow(
            subject_list,
            bids_templates={
                'mag'    : '{subject_id_p}/anat/*magnitude*.nii.gz',
                'phs'    : '{subject_id_p}/anat/*phase*.nii.gz',
                'params' : '{subject_id_p}/anat/*phase*.json'
            },
            bids_dir='bids',
            work_dir='nipype-qsm-work',
            out_dir='nipype-qsm-out',
            workflow_name='qsm',
            masking='bet'
        ):

    # absolute paths to directories
    this_dir = os.path.dirname(os.path.abspath(__file__))
    bids_dir = os.path.join(this_dir, bids_dir)
    work_dir = os.path.join(this_dir, work_dir)
    out_dir  = os.path.join(this_dir, out_dir)

    # create initial workflow
    wf = Workflow(name=workflow_name)
    wf.base_dir = work_dir

    # use infosource to iterate workflow across subject list
    n_infosource = Node(
        interface=IdentityInterface(fields=['subject_id']), # input and output: subject_id
        name="infosource"
        # output: 'subject_id'
    )
    n_infosource.iterables = ('subject_id', subject_list) # runs the node with subject_id = each element in subject_list

    # select matching files from bids_dir
    n_selectfiles = Node(
        interface=SelectFiles(bids_templates, base_directory=bids_dir),
        name='selectfiles'
        # output: ['mag', 'phs', 'params']
    )
    wf.connect([
        (n_infosource, n_selectfiles, [('subject_id', 'subject_id_p')])
    ])


    # scale phase data
    mn_stats = MapNode(
        interface=ImageStats(op_string='-R'), # -R : <min intensity> <max intensity>
        iterfield=['in_file'],
        name='stats_node',
        # output: 'out_stat'
    )
    mn_phs_range = MapNode(
        interface=ImageMaths(),
        name='phs_range_node',
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

    # brain extraction
    if masking == 'bet':
        mn_bet = MapNode(
            interface=BET(frac=0.4, mask=True, robust=True),
            iterfield=['in_file'],
            name='mask_node'
            # output: 'mask_file'
        )
        wf.connect([
            (n_selectfiles, mn_bet, [('mag', 'in_file')])
        ])
    elif masking == 'romeo':
        # ROMEO only operates on stacked .nii files
        n_stacked_magnitude = Node(
            interface=Merge(
                dimension='t',
                output_type='NIFTI'
            ),
            name="stack_magnitude",
            iterfield=['in_files']
            # output: 'merged_file'
        )
        wf.connect([
            (n_selectfiles, n_stacked_magnitude, [('mag', 'in_files')])
        ])
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
            (mn_phs_range, n_stacked_phase, [('out_file', 'in_files')])
        ])

        mn_bet = Node(
            interface=romeo.RomeoInterface(),
            iterfield=['mag_file', 'phase_file', 'echo_times'],
            name='mask_node'
            # output: 'mask_file'
        )
        wf.connect([
            (n_stacked_magnitude, mn_bet, [('merged_file', 'mag_file')]),
            (n_stacked_phase, mn_bet, [('merged_file', 'phase_file')]),
            (mn_params, mn_bet, [('EchoTime', 'echo_times')])
        ])
        # TODO: add MapNode to repeat the mask file for QSM

    # qsm processing
    mn_qsm = MapNode(
        interface=tgv.QSMappingInterface(
            iterations=1000, 
            alpha=[0.0015, 0.0005], 
            num_threads=8,
        ),
        iterfield=['phase_file', 'mask_file', 'TE', 'b0'],
        name='qsm_node'
        # output: 'out_file'
    )
    mn_qsm.plugin_args = {
        'qsub_args' : '-l nodes=1:ppn=16,mem=20gb,vmem=20gb, walltime=03:00:00',
        'overwrite' : True
    }

    wf.connect([
        (mn_params, mn_qsm, [('EchoTime', 'TE')]),
        (mn_params, mn_qsm, [('MagneticFieldStrength', 'b0')]),
        (mn_bet, mn_qsm, [('mask_files', 'mask_file')]),
        (mn_phs_range, mn_qsm, [('out_file', 'phase_file')])
    ])

    # DELETE ME
    # datasink
    n_datasink = Node(
        interface=DataSink(base_directory=bids_dir, container=out_dir),
        name='datasink'
    )
    wf.connect([(mn_qsm, n_datasink, [('out_file', 'qsm')])])
    return wf
    # DELETE ME

    # mask processing
    def generate_multiimagemaths_lists(in_files):
        in_file = in_files[0]
        operand_files = in_files[1:]
        op_string = '-add %s '
        op_string = len(operand_files) * op_string
        return in_file, operand_files, op_string
    
    n_generate_add_masks_lists = Node(
        interface=Function(
            input_names=['in_files'],
            output_names=['list_in_file', 'list_operand_files', 'list_op_string'],
            function=generate_multiimagemaths_lists
        ),
        name='generate_add_masks_lists_node'
    )

    n_add_masks = Node(
        interface=MultiImageMaths(), 
        name="add_masks_node"
        # output: 'out_file'
    )

    wf.connect([(mn_bet, n_generate_add_masks_lists, [('mask_file', 'in_files')])])
    wf.connect([(n_generate_add_masks_lists, n_add_masks, [('list_in_file', 'in_file')])])
    wf.connect([(n_generate_add_masks_lists, n_add_masks, [('list_operand_files', 'operand_files')])])
    wf.connect([(n_generate_add_masks_lists, n_add_masks, [('list_op_string', 'op_string')])])

    # qsm post-processing
    n_generate_add_qsms_lists = Node(
        interface=Function(
            input_names=['in_files'],
            output_names=['list_in_file', 'list_operand_files', 'list_op_string'],
            function=generate_multiimagemaths_lists
        ),
        name='generate_add_qsms_lists_node'
        # output: 'out_file'
    )

    n_add_qsms = Node(
        interface=MultiImageMaths(),
        name="add_qsms_node"
        # output: 'out_file'
    )

    wf.connect([(mn_qsm, n_generate_add_qsms_lists, [('out_file', 'in_files')])])
    wf.connect([(n_generate_add_qsms_lists, n_add_qsms, [('list_in_file', 'in_file')])])
    wf.connect([(n_generate_add_qsms_lists, n_add_qsms, [('list_operand_files', 'operand_files')])])
    wf.connect([(n_generate_add_qsms_lists, n_add_qsms, [('list_op_string', 'op_string')])])

    # divide qsm by mask
    n_final_qsm = Node(
        interface=ImageMaths(op_string='-div'),
        name="divide_added_qsm_by_added_masks"
        # output: 'out_file'
    )

    wf.connect([(n_add_qsms, n_final_qsm, [('out_file', 'in_file')])])
    wf.connect([(n_add_masks, n_final_qsm, [('out_file', 'in_file2')])])

    # datasink
    n_datasink = Node(
        interface=DataSink(base_directory=bids_dir, container=out_dir),
        name='datasink'
    )

    wf.connect([(n_add_masks, n_datasink, [('out_file', 'mask_sum')])])
    wf.connect([(n_add_qsms, n_datasink, [('out_file', 'qsm_sum')])])
    wf.connect([(n_final_qsm, n_datasink, [('out_file', 'qsm_final_default')])])
    wf.connect([(mn_qsm, n_datasink, [('out_file', 'qsm_singleEchoes')])])
    wf.connect([(mn_bet, n_datasink, [('mask_file', 'mask_singleEchoes')])])

    return wf
    
if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="QSM processing pipeline"
    )

    parser.add_argument(
        '--debug',
        dest='debug',
        action='store_true',
        help='debug mode'
    )

    parser.add_argument(
        '--masking',
        default='bet',
        const='bet',
        nargs='?',
        choices=['bet', 'romeo'],
        help='Masking strategy'
    )

    args = parser.parse_args()

    # environment variables
    os.environ["FSLOUTPUTTYPE"] = "NIFTI_GZ" # output type

    if args.debug:
        from nipype import config
        config.enable_debug_mode()
        config.set('execution', 'stop_on_first_crash', 'true')
        config.set('execution', 'remove_unnecessary_outputs', 'false')
        config.set('execution', 'keep_inputs', 'true')
        config.set('logging', 'workflow_level', 'DEBUG')
        config.set('logging', 'interface_level', 'DEBUG')
        config.set('logging', 'utils_level', 'DEBUG')

    # create qsm workflow
    wf = create_qsm_workflow(
        subject_list=[
            'sub-0001'
        ],
        masking=args.masking
    )

    # run workflow
    #wf.write_graph(graph2use='flat', format='png', simple_form=False)
    #wf.run('MultiProc', plugin_args={'n_procs': int(os.cpu_count())})
    #wf.run('MultiProc', plugin_args={'n_procs': 24})
    #wf.run(plugin='PBS', plugin_args={'-A UQ-CAI -l nodes=1:ppn=1,mem=5gb,vmem=5gb, walltime=01:00:00'})
    wf.run(plugin='PBSGraph', plugin_args=dict(qsub_args='-A UQ-CAI -l nodes=1:ppn=1,mem=5GB,vmem=5GB,walltime=00:30:00'))
