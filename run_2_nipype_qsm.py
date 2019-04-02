#!/usr/bin/env python3
from os.path import join as opj
import os
from nipype.interfaces.fsl import BET, ImageMaths, ImageStats, MultiImageMaths
from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import SelectFiles, DataSink
from nipype.pipeline.engine import Workflow, Node, MapNode
import nipype_interface_tgv_qsm as tgv
# from .nipype_interface_tgv_qsm import QSMappingInterface as tgv
# <editor-fold desc="DEBUG MODE">
# from nipype import config
# config.enable_debug_mode()
#
# config.set('execution', 'stop_on_first_crash', 'true')
# config.set('execution', 'remove_unnecessary_outputs', 'false')
# config.set('execution', 'keep_inputs', 'true')
# config.set('logging', 'workflow_level', 'DEBUG')
# config.set('logging', 'interface_level', 'DEBUG')
# config.set('logging', 'utils_level', 'DEBUG')
# </editor-fold>

# <editor-fold desc="Parameters">
os.environ["FSLOUTPUTTYPE"] = "NIFTI_GZ"

# work on scratch space only
experiment_dir = '/gpfs1/scratch/30days/uqsbollm/CONCUSSION-Q0538/interim'
#this is where the scripts and the bids data are located

output_dir = '/gpfs1/scratch/30days/uqsbollm/CONCUSSION-Q0538/derivatives'
# this is where the final files will be stored

working_dir = '/gpfs1/scratch/30days/uqsbollm/temp/CONCUSSION-Q0538'
# this is a temporary directory that can be deleted when everything ran

subject_list = ['sub-S008LCBL', 'sub-S009MC3D']

# </editor-fold>

# <editor-fold desc="Create Workflow and link to subject list">
wf = Workflow(name='qsm')
wf.base_dir = opj(experiment_dir, working_dir)

# create infosource to iterate over subject list
infosource = Node(IdentityInterface(fields=['subject_id']), name="infosource")
infosource.iterables = [('subject_id', subject_list)]
# </editor-fold>

# <editor-fold desc="Select files">
templates = {'mag': '{subject_id}/anat/*gre_M_echo_*.nii.gz',
             'phs': '{subject_id}/anat/*gre_P_echo_*.nii.gz',
             'params': '{subject_id}/anat/*gre_P_echo_*.json'}
selectfiles = Node(SelectFiles(templates, base_directory=experiment_dir), name='selectfiles')

wf.connect([(infosource, selectfiles, [('subject_id', 'subject_id')])])
# </editor-fold>

# <editor-fold desc="Brain Extraction">
bet_n = MapNode(BET(frac=0.4, mask=True, robust=True),
                name='bet_node', iterfield=['in_file'])

wf.connect([(selectfiles, bet_n, [('mag', 'in_file')])])
# </editor-fold>

# <editor-fold desc="Scale phase data">
stats = MapNode(ImageStats(op_string='-R'),
                name='stats_node', iterfield=['in_file'])


def scale_to_pi(min_and_max):
    data_min = min_and_max[0][0]
    data_max = min_and_max[0][1]
    # this works for VB17 data that is between 0 and 4096:
    #return '-add %.10f -div %.10f -mul 6.28318530718 -sub 3.14159265359' % (data_min, data_max+data_min)
    # This works for VE11C data with -4096 to + 4096 range
    return '-div %.10f -mul 3.14159265359' % (data_max)

phs_range_n = MapNode(ImageMaths(),
                      name='phs_range_node', iterfield=['in_file'])

wf.connect([(selectfiles, stats, [('phs', 'in_file')]),
            (selectfiles, phs_range_n, [('phs', 'in_file')]),
            (stats, phs_range_n, [(('out_stat', scale_to_pi), 'op_string')])
            ])
# </editor-fold>

# <editor-fold desc="Read echotime and fieldstrenghts from json files ">


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


params_n = MapNode(interface=Function(input_names=['in_file'],
                                      output_names=['EchoTime', 'MagneticFieldStrength'],
                                      function=read_json),
                   name='read_json', iterfield=['in_file'])

wf.connect([(selectfiles, params_n, [('params', 'in_file')])])
# </editor-fold>

# <editor-fold desc="QSM Processing">
# Run QSM processing
qsm_n = MapNode(tgv.QSMappingInterface(iterations=1000, alpha=[0.0015, 0.0005], num_threads=8),
                name='qsm_node', iterfield=['file_phase', 'file_mask', 'TE', 'b0'])
#
qsm_n.plugin_args = {'qsub_args': '-l nodes=1:ppn=16,mem=20gb,vmem=20gb, walltime=03:00:00',
                     'overwrite': True}

wf.connect([
    (params_n, qsm_n, [('EchoTime', 'TE')]),
    (params_n, qsm_n, [('MagneticFieldStrength', 'b0')]),
    (bet_n, qsm_n, [('mask_file', 'file_mask')]),
    (phs_range_n, qsm_n, [('out_file', 'file_phase')])
])
# </editor-fold>


# <editor-fold desc="Define the function that calls MultiImageMaths">
def generate_multiimagemaths_lists(in_files):
    in_file = in_files[0]
    operand_files = in_files[1:]
    op_string = '-add %s '
    op_string = len(operand_files) * op_string
    return in_file, operand_files, op_string
# </editor-fold>


# <editor-fold desc="Mask processing">
generate_add_masks_lists_n = Node(Function(
    input_names=['in_files'],
    output_names=['list_in_file', 'list_operand_files', 'list_op_string'],
    function=generate_multiimagemaths_lists),
    name='generate_add_masks_lists_node')

add_masks_n = Node(MultiImageMaths(),
                   name="add_masks_node")

wf.connect([(bet_n, generate_add_masks_lists_n, [('mask_file', 'in_files')])])
wf.connect([(generate_add_masks_lists_n, add_masks_n, [('list_in_file', 'in_file')])])
wf.connect([(generate_add_masks_lists_n, add_masks_n, [('list_operand_files', 'operand_files')])])
wf.connect([(generate_add_masks_lists_n, add_masks_n, [('list_op_string', 'op_string')])])

# </editor-fold>

# # <editor-fold desc="QSM Post processing">
generate_add_qsms_lists_n = Node(Function(
    input_names=['in_files'],
    output_names=['list_in_file', 'list_operand_files', 'list_op_string'],
    function=generate_multiimagemaths_lists),
    name='generate_add_qsms_lists_node')

add_qsms_n = Node(MultiImageMaths(),
                  name="add_qsms_node")

wf.connect([(qsm_n, generate_add_qsms_lists_n, [('out_file', 'in_files')])])
wf.connect([(generate_add_qsms_lists_n, add_qsms_n, [('list_in_file', 'in_file')])])
wf.connect([(generate_add_qsms_lists_n, add_qsms_n, [('list_operand_files', 'operand_files')])])
wf.connect([(generate_add_qsms_lists_n, add_qsms_n, [('list_op_string', 'op_string')])])

# divide QSM by mask
final_qsm_n = Node(ImageMaths(op_string='-div'),
                   name="divide_added_qsm_by_added_masks")

wf.connect([(add_qsms_n, final_qsm_n, [('out_file', 'in_file')])])
wf.connect([(add_masks_n, final_qsm_n, [('out_file', 'in_file2')])])

# </editor-fold>

# <editor-fold desc="Datasink">
datasink = Node(DataSink(base_directory=experiment_dir, container=output_dir),
                name='datasink')

wf.connect([(add_masks_n, datasink, [('out_file', 'mask_sum')])])
wf.connect([(add_qsms_n, datasink, [('out_file', 'qsm_sum')])])
wf.connect([(final_qsm_n, datasink, [('out_file', 'qsm_final_default')])])
wf.connect([(qsm_n, datasink, [('out_file', 'qsm_singleEchoes')])])
wf.connect([(bet_n, datasink, [('mask_file', 'mask_singleEchoes')])])

# </editor-fold>

# <editor-fold desc="Run">
# # run as MultiProc
# # wf.write_graph(graph2use='flat', format='png', simple_form=False)


#wf.run('MultiProc', plugin_args={'n_procs': int(os.environ['NCPUS'])})

wf.run('MultiProc', plugin_args={'n_procs': 24})

# wf.run(plugin='PBS', plugin_args={'-A UQ-CAI -l nodes=1:ppn=1,mem=5gb,vmem=5gb, walltime=01:00:00'})

#wf.run(plugin='PBSGraph', plugin_args=dict(
#    qsub_args='-A UQ-CAI -l nodes=1:ppn=1,mem=5GB,vmem=5GB,walltime=00:30:00'))

# </editor-fold>
