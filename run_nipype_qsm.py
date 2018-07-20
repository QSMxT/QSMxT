#!/usr/bin/env python3
from os.path import join as opj
import os
from nipype.interfaces.fsl import (BET, ImageMaths)
from nipype.interfaces.utility import IdentityInterface
from nipype.interfaces.io import SelectFiles, DataSink
from nipype.pipeline.engine import Workflow, Node
from nipype import MapNode
import nipype_interface_tgv_qsm as tgv

os.environ["FSLOUTPUTTYPE"] = "NIFTI_GZ"

experiment_dir = '.'
output_dir = '../processed'
working_dir = '../scratch'

subject_list = ['sub-']

# Infosource - a function free node to iterate over the list of subject names
infosource = Node(IdentityInterface(fields=['subject_id']), name="infosource")
infosource.iterables = [('subject_id', subject_list)]

# SelectFiles - to grab the data (alternative to DataGrabber)
templates = {'mag': '{subject_id}/anat/*gre_M_echo_1.nii.gz',
             'phs': '{subject_id}/anat/*gre_P_echo_1.nii.gz'}
selectfiles = Node(SelectFiles(templates, base_directory=experiment_dir), name='selectfiles')

# Datasink - creates output folder for important outputs
datasink = Node(DataSink(base_directory=experiment_dir, container=output_dir), name='datasink')


# create Nodes
bet_n = MapNode(BET(frac=0.4, mask=True, robust=True),
                name='bet_node', iterfield=['in_file'])

phs_range_n = MapNode(ImageMaths(op_string='-div 4096 -mul 6.28318530718 -sub 3.14159265359'),
                      name='phs_range_node', iterfield=['in_file'])

qsm_n = MapNode(tgv.QSMappingInterface(iterations=30, TE=0.04, b0=7),
                name='qsm_node', iterfield=['file_phase', 'file_mask'])


# Connect Nodes in  preprocessing workflow
preproc = Workflow(name='qsm')
preproc.base_dir = opj(experiment_dir, working_dir)
preproc.connect([(infosource, selectfiles, [('subject_id', 'subject_id')]),
                 (selectfiles, bet_n, [('mag', 'in_file')]),
                 (selectfiles, phs_range_n, [('phs', 'in_file')]),
                 (bet_n, qsm_n, [('mask_file', 'file_mask')]),
                 (phs_range_n, qsm_n, [('out_file', 'file_phase')]),
                 (qsm_n, datasink, [('out_qsm', 'qsm_final')]),
                 ])

# run
preproc.run('MultiProc', plugin_args={'n_procs': 9})
