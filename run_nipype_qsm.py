#!/usr/bin/env python3
from os.path import join as opj
import os
from nipype.interfaces.fsl import (BET, ImageMaths)
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


# Infosource - a function free node to iterate over the list of subject names
infosource = Node(IdentityInterface(fields=['subject_id']), name="infosource")
infosource.iterables = [('subject_id', subject_list)]


# SelectFiles - to grab the data (alternative to DataGrabber)
templates = {'mag': '{subject_id}/anat/*gre_M_echo_[1,2].nii.gz',
             'phs': '{subject_id}/anat/*gre_P_echo_[1,2].nii.gz',
             'params': '{subject_id}/anat/*gre_P_echo_[1,2].json'}
selectfiles = Node(SelectFiles(templates, base_directory=experiment_dir), name='selectfiles')


# create Nodes
bet_n = MapNode(BET(frac=0.4, mask=True, robust=True),
                name='bet_node', iterfield=['in_file'])

phs_range_n = MapNode(ImageMaths(op_string='-div 4096 -mul 6.28318530718 -sub 3.14159265359'),
                      name='phs_range_node', iterfield=['in_file'])


def read_json(in_file):
    import os
    if os.path.exists(in_file):
        import json
        with open(in_file, 'rt') as fp:
            data = json.load(fp)
            EchoTime = data['EchoTime']
            MagneticFieldStrength = data['MagneticFieldStrength']

    return EchoTime, MagneticFieldStrength


params_n = MapNode(interface=Function(input_names=['in_file'],
                                      output_names=['EchoTime', 'MagneticFieldStrength'],
                                      function=read_json),
                   name='read_json', iterfield=['in_file'])

qsm_n = MapNode(tgv.QSMappingInterface(iterations=1, b0=7),
                name='qsm_node', iterfield=['file_phase', 'file_mask', 'TE', 'b0'])

datasink = Node(DataSink(base_directory=experiment_dir, container=output_dir),
                name='datasink')


# Connect Nodes in  preprocessing workflow
preproc = Workflow(name='qsm')
preproc.base_dir = opj(experiment_dir, working_dir)
preproc.connect([(infosource, selectfiles, [('subject_id', 'subject_id')]),
                 (selectfiles, bet_n, [('mag', 'in_file')]),
                 (selectfiles, phs_range_n, [('phs', 'in_file')]),
                 (selectfiles, params_n, [('params', 'in_file')]),
                 (params_n, qsm_n, [('EchoTime', 'TE')]),
                 (params_n, qsm_n, [('MagneticFieldStrength', 'b0')]),
                 (bet_n, qsm_n, [('mask_file', 'file_mask')]),
                 (phs_range_n, qsm_n, [('out_file', 'file_phase')]),
                 (qsm_n, datasink, [('out_qsm', 'qsm')]),
                 ])

# run as MultiProc
preproc.write_graph(graph2use='flat', format='png', simple_form=False)
preproc.run('MultiProc', plugin_args={'n_procs': 1})