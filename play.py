from nipype_interface_tgv_qsm import QSMappingInterface
from nipype.interfaces.fsl import ImageMaths
from nipype.pipeline.engine import Workflow, Node

ImageMaths.help()
phs_range = Node(ImageMaths(op_string='-div 4096 -mul 6.28318530718 -sub 3.14159265359'), name='phs_range')
phs_range.help()

qsm = QSMappingInterface()
qsm.help()
qsm_node = Node(QSMappingInterface(TE=0.004, b0=3), name='phs_range')

