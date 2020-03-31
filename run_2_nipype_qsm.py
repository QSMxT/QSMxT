#!/usr/bin/env python3

import os.path
import os
from nipype.interfaces.fsl import BET, ImageMaths, ImageStats, MultiImageMaths
from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import SelectFiles, DataSink
from nipype.pipeline.engine import Workflow, Node, MapNode
import nipype_interface_tgv_qsm as tgv

if __name__ == "__main__":
    ## DEBUG ##
    # from .nipype_interface_tgv_qsm import QSMappingInterface as tgv
    # from nipype import config
    # config.enable_debug_mode()
    # config.set('execution', 'stop_on_first_crash', 'true')
    # config.set('execution', 'remove_unnecessary_outputs', 'false')
    # config.set('execution', 'keep_inputs', 'true')
    # config.set('logging', 'workflow_level', 'DEBUG')
    # config.set('logging', 'interface_level', 'DEBUG')
    # config.set('logging', 'utils_level', 'DEBUG')

    # environment variables
    os.environ["FSLOUTPUTTYPE"] = "NIFTI_GZ" # output type

    # directories
    this_path   = os.path.dirname(os.path.abspath(__file__))
    data_dir    = this_path + '/bids/'            # bids data directory
    output_dir  = this_path + '/nipype-qsm-out/'  # final output directory
    working_dir = this_path + '/nipype-qsm-work/' # temp working directory

    # subjects in the data directory to process
    subject_list = ['sub-0001']

    # files to match
    templates = {
        'mag'    : '{subject_id_p}/anat/*magnitude*.nii.gz',
        'phs'    : '{subject_id_p}/anat/*phase*.nii.gz',
        'params' : '{subject_id_p}/anat/*phase*.json'
    }

    # create workflow
    wf = Workflow(name='qsm')
    wf.base_dir = working_dir

    # use infosource to iterate workflow across subject list
    n_infosource = Node(
        interface=IdentityInterface(fields=['subject_id']), # input and output: subject_id
        name="infosource"
        # output: 'subject_id'
    )
    n_infosource.iterables = ('subject_id', subject_list) # runs the node with subject_id = each element in subject_list

    # select matching files from data_dir
    n_selectfiles = Node(
        interface=SelectFiles(templates, base_directory=data_dir),
        name='selectfiles'
        # output: ['mag', 'phs', 'params']
    )
    wf.connect([
        (n_infosource, n_selectfiles, [('subject_id', 'subject_id_p')])
    ])

    # brain extraction
    mn_bet = MapNode(
        interface=BET(frac=0.4, mask=True, robust=True),
        iterfield=['in_file'],
        name='bet_node'
        # output: 'mask_file'
    )
    wf.connect([
        (n_selectfiles, mn_bet, [('mag', 'in_file')])
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
        # this works for VB17 data that is between 0 and 4096:
        #data_min = min_and_max[0][0]
        #data_max = min_and_max[0][1]
        #return '-add %.10f -div %.10f -mul 6.28318530718 -sub 3.14159265359' % (data_min, data_max+data_min)

        # This works for VE11C data with -4096 to + 4096 range
        data_max = min_and_max[0][1]
        return '-div %.10f -mul 3.14159265359' % (data_max)

    wf.connect([
        (n_selectfiles, mn_stats, [('phs', 'in_file')]),
        (n_selectfiles, mn_phs_range, [('phs', 'in_file')]),
        (mn_stats, mn_phs_range, [(('out_stat', scale_to_pi), 'op_string')])
        # for vb17 use lambda min_max: '-add %.10f -div %.10f -mul 6.28318530718 -sub 3.14159265359' % (min_max[0][0], min_max[0][0]+min_max[0][1])
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

    # qsm processing
    mn_qsm = MapNode(
        tgv.QSMappingInterface(
            iterations=1000, 
            alpha=[0.0015, 0.0005], 
            num_threads=8
        ),
        iterfield=['file_phase', 'file_mask', 'TE', 'b0'],
        name='qsm_node'
    )
    mn_qsm.plugin_args = {
        'qsub_args' : '-l nodes=1:ppn=16,mem=20gb,vmem=20gb, walltime=03:00:00',
        'overwrite' : True
    }

    wf.connect([
        (mn_params, mn_qsm, [('EchoTime', 'TE')]),
        (mn_params, mn_qsm, [('MagneticFieldStrength', 'b0')]),
        (mn_bet, mn_qsm, [('mask_file', 'file_mask')]),
        (mn_phs_range, mn_qsm, [('out_file', 'file_phase')])
    ])

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
    )

    n_add_qsms = Node(
        interface=MultiImageMaths(),
        name="add_qsms_node"
    )

    wf.connect([(mn_qsm, n_generate_add_qsms_lists, [('out_file', 'in_files')])])
    wf.connect([(n_generate_add_qsms_lists, n_add_qsms, [('list_in_file', 'in_file')])])
    wf.connect([(n_generate_add_qsms_lists, n_add_qsms, [('list_operand_files', 'operand_files')])])
    wf.connect([(n_generate_add_qsms_lists, n_add_qsms, [('list_op_string', 'op_string')])])

    # divide qsm by mask
    n_final_qsm = Node(
        interface=ImageMaths(op_string='-div'),
        name="divide_added_qsm_by_added_masks"
    )

    wf.connect([(n_add_qsms, n_final_qsm, [('out_file', 'in_file')])])
    wf.connect([(n_add_masks, n_final_qsm, [('out_file', 'in_file2')])])

    # datasink
    n_datasink = Node(
        interface=DataSink(base_directory=data_dir, container=output_dir),
        name='datasink'
    )

    wf.connect([(n_add_masks, n_datasink, [('out_file', 'mask_sum')])])
    wf.connect([(n_add_qsms, n_datasink, [('out_file', 'qsm_sum')])])
    wf.connect([(n_final_qsm, n_datasink, [('out_file', 'qsm_final_default')])])
    wf.connect([(mn_qsm, n_datasink, [('out_file', 'qsm_singleEchoes')])])
    wf.connect([(mn_bet, n_datasink, [('mask_file', 'mask_singleEchoes')])])

    # run
    #wf.write_graph(graph2use='flat', format='png', simple_form=False)
    wf.run('MultiProc', plugin_args={'n_procs': int(os.cpu_count())})
    #wf.run('MultiProc', plugin_args={'n_procs': 24})
    #wf.run(plugin='PBS', plugin_args={'-A UQ-CAI -l nodes=1:ppn=1,mem=5gb,vmem=5gb, walltime=01:00:00'})
    #wf.run(plugin='PBSGraph', plugin_args=dict(
    #qsub_args='-A UQ-CAI -l nodes=1:ppn=1,mem=5GB,vmem=5GB,walltime=00:30:00'))
