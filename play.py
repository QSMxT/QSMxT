from nipype.interfaces import fsl
import nipype_interface_tgv_qsm as tgv
from nipype.interfaces.fsl import ImageMaths
from nipype.pipeline.engine import Workflow, Node

ImageMaths.help()

phs_range = Node(ImageMaths(op_string='-div 4096 -mul 6.28318530718 -sub 3.14159265359'), name='phs_range')

qsm = tgv.QSMappingInterface()
qsm.help()


bet = fsl.BET()
type(bet.inputs)

bet.help()