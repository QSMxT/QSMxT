#!/usr/bin/env python3
from os.path import join as opj
import os
from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import SelectFiles, DataSink
from nipype.pipeline.engine import Workflow, Node, MapNode

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

# work on scratch space only
experiment_dir = '/gpfs1/scratch/30days/uqsbollm/CONCUSSION-Q0538/interim'

# output_dir = '/gpfs1/scratch/30days/uqsbollm/CONCUSSION-Q0538/derivatives'
output_dir = '/QRISdata/Q0538/17042_detection_of_concussion/derivatives'

working_dir = '/gpfs1/scratch/30days/uqsbollm/temp/CONCUSSION-Q0538'

# work on collection for input and output
# experiment_dir = '/QRISdata/Q0538/17042_detection_of_concussion/interim'
# output_dir = '/QRISdata/Q0538/17042_detection_of_concussion/derivatives'
# working_dir = '/gpfs1/scratch/30days/uqsbollm/temp/CONCUSSION-Q0538'

# work on CAI cluster
# experiment_dir = '/data/fastertemp/uqsbollm/uqrdmcache/CONCUSSION-Q0538/17042_detection_of_concussion/interim'
# output_dir = '/data/fastertemp/uqsbollm/uqrdmcache/CONCUSSION-Q0538/17042_detection_of_concussion/derivatives'
# working_dir = '/data/fastertemp/uqsbollm/scratch/CONCUSSION-Q0538'

subject_list = ['sub-S013FBBL']

# subject_list = ['sub-S008LCBL', 'sub-S009MC3D', 'sub-S009MC7D', 'sub-S009MCBL', 'sub-S010BD',
#                 'sub-S011RJBL', 'sub-S013FBBL', 'sub-S014WSBL', 'sub-S015KSBL', 'sub-S016JVBL',
#                 'sub-S017DPBL', 'sub-S018ALBL', 'sub-S019PLBL', 'sub-S020DABL']

# </editor-fold>

# <editor-fold desc="Create Workflow and link to subject list">
wf = Workflow(name='ants_group')
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

# <editor-fold desc="Datasink">
datasink = Node(DataSink(base_directory=experiment_dir, container=output_dir),
                name='datasink')

# wf.connect([(add_masks_n, datasink, [('out_file', 'mask_sum')])])

# </editor-fold>

# <editor-fold desc="Run">
# run as MultiProc
# wf.write_graph(graph2use='flat', format='png', simple_form=False)
wf.run('MultiProc', plugin_args={'n_procs': int(os.environ['NCPUS'])})
# wf.run(plugin='PBS', plugin_args={'-A UQ-CAI -l nodes=1:ppn=1,mem=5gb,vmem=5gb, walltime=01:00:00'})
# wf.run(plugin='PBSGraph', plugin_args=dict(qsub_args='-A UQ-CAI -l nodes=1:ppn=1,mem=20GB,vmem=20GB,walltime=04:00:00'))

# </editor-fold>

