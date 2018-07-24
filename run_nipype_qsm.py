#!/usr/bin/env python3
from os.path import join as opj
import os
from nipype.interfaces.fsl import BET, ImageMaths, ImageStats, Merge
from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import SelectFiles, DataSink
from nipype.pipeline.engine import Workflow, Node, MapNode
import nipype_interface_tgv_qsm as tgv

# <editor-fold desc="Parameters">
os.environ["FSLOUTPUTTYPE"] = "NIFTI_GZ"

experiment_dir = '/QRISdata/Q0538/17042_detection_of_concussion/interim'
output_dir = '/QRISdata/Q0538/17042_detection_of_concussion/derivatives'
working_dir = '/gpfs1/scratch/30days/uqsbollm/17042_detection_of_concussion'

subject_list = ['sub-S008LCBL']
# </editor-fold>

# <editor-fold desc="Create Workflow and link to subject list">
qsm_wf = Workflow(name='qsm')
qsm_wf.base_dir = opj(experiment_dir, working_dir)

# create infosource to iterate over subject list
infosource = Node(IdentityInterface(fields=['subject_id']), name="infosource")
infosource.iterables = [('subject_id', subject_list)]
# </editor-fold>

# <editor-fold desc="Select files">
templates = {'mag': '{subject_id}/anat/*gre_M_echo_*.nii.gz',
             'phs': '{subject_id}/anat/*gre_P_echo_*.nii.gz',
             'params': '{subject_id}/anat/*gre_P_echo_*.json'}
selectfiles = Node(SelectFiles(templates, base_directory=experiment_dir), name='selectfiles')

qsm_wf.connect([(infosource, selectfiles, [('subject_id', 'subject_id')])])
# </editor-fold>

# <editor-fold desc="Brain Extraction">
bet_n = MapNode(BET(frac=0.4, mask=True, robust=True),
                name='bet_node', iterfield=['in_file'])

qsm_wf.connect([(selectfiles, bet_n, [('mag', 'in_file')])])
# </editor-fold>

# <editor-fold desc="Scale phase data">
# Scale phase images
stats = MapNode(ImageStats(op_string='-R'),
                name='stats_node', iterfield=['in_file'])


def scale_to_pi(min_and_max):
    data_min = min_and_max[0][0]
    data_max = min_and_max[0][1]
    # TODO: Test at 3T with -4096 to + 4096 range
    return '-add %.10f -div %.10f -mul 6.28318530718 -sub 3.14159265359' % (data_min, data_max+data_min)


phs_range_n = MapNode(ImageMaths(),
                      name='phs_range_node', iterfield=['in_file'])

qsm_wf.connect([(selectfiles, stats, [('phs', 'in_file')]),
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

qsm_wf.connect([(selectfiles, params_n, [('params', 'in_file')])])
# </editor-fold>

# <editor-fold desc="QSM Processing">
# Run QSM processing
qsm_n = MapNode(tgv.QSMappingInterface(iterations=1, b0=7),
                name='qsm_node', iterfield=['file_phase', 'file_mask', 'TE', 'b0'])

qsm_wf.connect([
    (params_n, qsm_n, [('EchoTime', 'TE')]),
    (params_n, qsm_n, [('MagneticFieldStrength', 'b0')]),
    (bet_n, qsm_n, [('mask_file', 'file_mask')]),
    (phs_range_n, qsm_n, [('out_file', 'file_phase')])])
# </editor-fold>

# <editor-fold desc="Mask processing">
# Merge masks of individual echoes
merge_masks_n = Node(Merge(dimension='t'),
                     name="add_masks_node")

qsm_wf.connect([(bet_n, merge_masks_n, [('mask_file', 'in_files')])])

# Merge masks of individual echoes
merge_masks_n = Node(ImageMaths(op_string='-Tmean'),
                     name="add_masks_node")

qsm_wf.connect([(bet_n, merge_masks_n, [('mask_file', 'in_files')])])
# </editor-fold>

#<editor-fold desc="Datasink">
# Add data to datasink output folder
datasink = Node(DataSink(base_directory=experiment_dir, container=output_dir),
                name='datasink')

# qsm_wf.connect([(qsm_n, datasink, [('out_qsm', 'qsm')])])
qsm_wf.connect([(merge_masks_n, datasink, [('merged_file', 'masks')])])
#</editor-fold>

# <editor-fold desc="Run">
# run as MultiProc
qsm_wf.write_graph(graph2use='flat', format='png', simple_form=False)
qsm_wf.run('MultiProc', plugin_args={'n_procs': 9})
# </editor-fold>

