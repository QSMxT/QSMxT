#!/usr/bin/env python3
from os.path import join as opj
import os
from nipype.interfaces.fsl import (BET, ImageMaths, ImageStats)
from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import SelectFiles, DataSink, JSONFileGrabber, JSONFileSink
from nipype.pipeline.engine import Workflow, Node
from nipype import MapNode
import nipype_interface_tgv_qsm as tgv

os.environ["FSLOUTPUTTYPE"] = "NIFTI_GZ"

experiment_dir = '/QRISdata/Q0538/17042_detection_of_concussion/interim'
output_dir = '/QRISdata/Q0538/17042_detection_of_concussion/derivatives'
working_dir = '/gpfs1/scratch/30days/uqsbollm/17042_detection_of_concussion'

subject_list = ['sub-S008LCBL']


qsm_wf = Workflow(name='qsm')
qsm_wf.base_dir = opj(experiment_dir, working_dir)

infosource = Node(IdentityInterface(fields=['subject_id']), name="infosource")
infosource.iterables = [('subject_id', subject_list)]

templates = {'mag': '{subject_id}/anat/*gre_M_echo_*.nii.gz',
             'phs': '{subject_id}/anat/*gre_P_echo_*.nii.gz',
             'params': '{subject_id}/anat/*gre_P_echo_*.json'}
selectfiles = Node(SelectFiles(templates, base_directory=experiment_dir), name='selectfiles')

qsm_wf.connect([(infosource, selectfiles, [('subject_id', 'subject_id')])])


bet_n = MapNode(BET(frac=0.4, mask=True, robust=True),
                name='bet_node', iterfield=['in_file'])

stats = MapNode(ImageStats(op_string='-R'),
                name='stats_node', iterfield=['in_file'])


def scale_to_pi(min_and_max):
    data_min = min_and_max[0][0]
    data_max = min_and_max[0][1]
    return '-add %.10f -div %.10f -mul 6.28318530718 -sub 3.14159265359' % (data_min, data_max+data_min)


phs_range_n = MapNode(ImageMaths(),
                      name='phs_range_node', iterfield=['in_file'])

qsm_wf.connect([(selectfiles, bet_n, [('mag', 'in_file')]),
                (selectfiles, stats, [('phs', 'in_file')]),
                (selectfiles, phs_range_n, [('phs', 'in_file')]),
                (stats, phs_range_n, [(('out_stat', scale_to_pi), 'op_string')])
                ])


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



qsm_n = MapNode(tgv.QSMappingInterface(iterations=1, b0=7),
                name='qsm_node', iterfield=['file_phase', 'file_mask', 'TE', 'b0'])

qsm_wf.connect([
    (params_n, qsm_n, [('EchoTime', 'TE')]),
    (params_n, qsm_n, [('MagneticFieldStrength', 'b0')]),
    (bet_n, qsm_n, [('mask_file', 'file_mask')]),
    (phs_range_n, qsm_n, [('out_file', 'file_phase')])])







datasink = Node(DataSink(base_directory=experiment_dir, container=output_dir),
                name='datasink')

qsm_wf.connect([(qsm_n, datasink, [('out_qsm', 'qsm')])])





# run as MultiProc
qsm_wf.write_graph(graph2use='flat', format='png', simple_form=False)
qsm_wf.run('MultiProc', plugin_args={'n_procs': 1})