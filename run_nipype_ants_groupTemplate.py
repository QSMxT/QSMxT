#!/usr/bin/env python3
from __future__ import print_function
from os.path import join as opj
from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import SelectFiles, DataSink
from nipype.pipeline.engine import Workflow, Node, MapNode
from future import standard_library
standard_library.install_aliases()
import os
import nipype.interfaces.utility as util
import nipype.interfaces.ants as ants
import nipype.interfaces.io as io
import nipype.pipeline.engine as pe  # pypeline engine
from nipype.workflows.smri.ants import antsRegistrationTemplateBuildSingleIterationWF


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


registrationImageTypes = ['T1']

interpolationMapping = {
    'INV_T1': 'LanczosWindowedSinc',
    'LABEL_MAP': 'NearestNeighbor',
    'T1': 'Linear'
}

# </editor-fold>

# <editor-fold desc="Create Workflow and link to subject list">
tbuilder = Workflow(name='ants_group')
tbuilder.base_dir = opj(experiment_dir, working_dir)

# create infosource to iterate over subject list
infosource = Node(IdentityInterface(fields=['subject_id']), name="infosource")
infosource.iterables = [('subject_id', subject_list)]

# </editor-fold>

# <editor-fold desc="Select files">
templates = {'mag': '{subject_id}/anat/*gre_M_echo_1*.nii.gz',
             'phs': '{subject_id}/anat/*gre_P_echo_1*.nii.gz',
             'params': '{subject_id}/anat/*gre_P_echo_1*.json',
             'InitialTemplateInputs': 'sub-S008LCBL/anat/sub-S008LCBL_gre_M_echo_1.nii.gz'}
selectfiles = Node(SelectFiles(templates, base_directory=experiment_dir), name='selectfiles')

tbuilder.connect([(infosource, selectfiles, [('subject_id', 'subject_id')])])

datasource = pe.Node(
    interface=util.IdentityInterface(fields=[
        'InitialTemplateInputs', 'ListOfImagesDictionaries',
        'registrationImageTypes', 'interpolationMapping'
    ]),
    run_without_submitting=True,
    name='InputImages')
datasource.inputs.registrationImageTypes = registrationImageTypes
datasource.inputs.interpolationMapping = interpolationMapping
datasource.inputs.sort_filelist = True

# </editor-fold>


# <editor-fold desc="Template Construction">
initAvg = pe.Node(interface=ants.AverageImages(), name='initAvg')
initAvg.inputs.dimension = 3
initAvg.inputs.normalize = True

tbuilder.connect(selectfiles, "InitialTemplateInputs", initAvg, "images")

buildTemplateIteration1 = antsRegistrationTemplateBuildSingleIterationWF(
    'iteration01')

BeginANTS = buildTemplateIteration1.get_node("BeginANTS")
# BeginANTS.plugin_args = {
#     'qsub_args':
#     '-S /bin/bash -pe smp1 8-12 -l mem_free=6000M -o /dev/null -e /dev/null queue_name',
#     'overwrite':
#     True
# }

tbuilder.connect(initAvg, 'output_average_image', buildTemplateIteration1,
                 'inputspec.fixed_image')
tbuilder.connect(selectfiles, 'mag',
                 buildTemplateIteration1, 'inputspec.ListOfImagesDictionaries')
tbuilder.connect(datasource, 'registrationImageTypes', buildTemplateIteration1,
                 'inputspec.registrationImageTypes')
tbuilder.connect(datasource, 'interpolationMapping', buildTemplateIteration1,
                 'inputspec.interpolationMapping')

buildTemplateIteration2 = antsRegistrationTemplateBuildSingleIterationWF(
    'iteration02')
BeginANTS = buildTemplateIteration2.get_node("BeginANTS")
# BeginANTS.plugin_args = {
#     'qsub_args':
#     '-S /bin/bash -pe smp1 8-12 -l mem_free=6000M -o /dev/null -e /dev/null queue_name',
#     'overwrite':
#     True
# }
tbuilder.connect(buildTemplateIteration1, 'outputspec.template',
                 buildTemplateIteration2, 'inputspec.fixed_image')
tbuilder.connect(datasource, 'ListOfImagesDictionaries',
                 buildTemplateIteration2, 'inputspec.ListOfImagesDictionaries')
tbuilder.connect(datasource, 'registrationImageTypes', buildTemplateIteration2,
                 'inputspec.registrationImageTypes')
tbuilder.connect(datasource, 'interpolationMapping', buildTemplateIteration2,
                 'inputspec.interpolationMapping')

# </editor-fold>

# <editor-fold desc="Datasink">
datasink = Node(DataSink(base_directory=experiment_dir, container=output_dir),
                name='datasink')

tbuilder.connect(buildTemplateIteration2, 'outputspec.template', datasink,
                 'PrimaryTemplate')
tbuilder.connect(buildTemplateIteration2,
                 'outputspec.passive_deformed_templates', datasink,
                 'PassiveTemplate')
tbuilder.connect(initAvg, 'output_average_image', datasink,
                 'PreRegisterAverage')
# </editor-fold>

# <editor-fold desc="Run">
# run as MultiProc
# wf.write_graph(graph2use='flat', format='png', simple_form=False)
tbuilder.run('MultiProc', plugin_args={'n_procs': int(os.environ['NCPUS'])})
# wf.run(plugin='PBS', plugin_args={'-A UQ-CAI -l nodes=1:ppn=1,mem=5gb,vmem=5gb, walltime=01:00:00'})
# wf.run(plugin='PBSGraph', plugin_args=dict(qsub_args='-A UQ-CAI -l nodes=1:ppn=1,mem=20GB,vmem=20GB,walltime=04:00:00'))

# </editor-fold>

