from os.path import join as opj
import os
from nipype.interfaces.fsl import (BET, ExtractROI, FAST, FLIRT, ImageMaths,
                                   MCFLIRT, SliceTimer, Threshold)
from nipype.interfaces.utility import IdentityInterface
from nipype.interfaces.io import SelectFiles, DataSink
from nipype.pipeline.engine import Workflow, Node

experiment_dir = '/QRISdata/Q0538/17042_detection_of_concussion/interim'
output_dir = '/QRISdata/Q0538/17042_detection_of_concussion/processed'
working_dir = '/gpfs1/scratch/30days/uqsbollm/17042_detection_of_concussion'

# list of subject identifiers
subject_list = ['sub-S008LCBL']

# Infosource - a function free node to iterate over the list of subject names
infosource = Node(IdentityInterface(fields=['subject_id']),name="infosource")
infosource.iterables = [('subject_id', subject_list)]

# SelectFiles - to grab the data (alternativ to DataGrabber)
anat_file = opj('{subject_id}/anat/*gre_M*.nii.gz')

templates = {'anat': anat_file}
selectfiles = Node(SelectFiles(templates,base_directory=experiment_dir),name="selectfiles")

# Datasink - creates output folder for important outputs
datasink = Node(DataSink(base_directory=experiment_dir,container=output_dir),name="datasink")

# Create a preprocessing workflow
preproc = Workflow(name='preproc')
preproc.base_dir = opj(experiment_dir, working_dir)

smooth = Node(ImageMaths(op_string='-fmean -s 2'), name="smooth")

# Connect all components of the preprocessing workflow
preproc.connect([(infosource, selectfiles, [('subject_id', 'subject_id')]),
                 (selectfiles, smooth, [('anat', 'in_file')]),
                 (smooth, datasink, [('out_file', 'preproc.@smooth')]),
                 ])

# Create preproc output graph
preproc.write_graph(graph2use='colored', format='png', simple_form=True)

# Visualize the graph
from IPython.display import Image
Image(filename=opj(preproc.base_dir, 'preproc', 'graph.png'))

# Visualize the detailed graph
preproc.write_graph(graph2use='flat', format='png', simple_form=True)
Image(filename=opj(preproc.base_dir, 'preproc', 'graph_detailed.png'))

preproc.run('MultiProc', plugin_args={'n_procs': 4})