from nipype.pipeline.engine import Workflow, Node, MapNode
from nipype.interfaces.utility import IdentityInterface
from nipype.interfaces.fsl import ImageMaths
import interfaces.nipype_interface_nextqsm as nextqsm_interface
from interfaces import nipype_interface_nonzeroaverage as nonzeroaverage

def nextqsm_workflow(name='nextqsm'):
    wf = Workflow(name)
    
    inputnode = MapNode(
        interface=IdentityInterface(
            fields=['unwrapped_phase', 'mask', 'mag', 'TE', 'fieldStrength']),
        iterfield=['unwrapped_phase', 'mask', 'mag', 'TE', 'fieldStrength'],
        name='inputnode'
    )
    outputnode = MapNode(
        interface=IdentityInterface(
            fields=['qsm', 'qsm_average']),
        iterfield=['qsm'],
        name='outputnode'
    )
    
    phase_normalize = MapNode(
        interface=nextqsm_interface.NormalizeInterface(
            out_suffix='_normalized'
        ),
        iterfield=['phase', 'TE', 'fieldStrength'],
        name='normalize_phase'
        # output: 'out_file'
    )
    nextqsm = MapNode(
        interface=nextqsm_interface.NextqsmInterface(),
        iterfield=['phase', 'mask'],
        name='nextqsm'
        # output: 'out_file'
    )
    average = Node(
        interface=nonzeroaverage.NonzeroAverageInterface(),
        name='nibabel_nextqsm-average'
        # input : in_files
        # output: out_file
    )
    
    wf.connect([
        (inputnode, phase_normalize, [('TE', 'TE'),
                                      ('fieldStrength', 'fieldStrength'),
                                      ('unwrapped_phase', 'phase')]),
        (inputnode, nextqsm, [('mask', 'mask')]),
        (phase_normalize, nextqsm, [('out_file', 'phase')]),
        (nextqsm, outputnode, [('out_file', 'qsm')]),
        (nextqsm, average, [('out_file', 'in_files')]),
        (average, outputnode, [('out_file', 'qsm_averaged')]),
    ])
    return wf


def nextqsm_B0_workflow(name='nextqsm_B0', use_B0_mask=False, B0_threshold=0.5):
    wf = Workflow(name)
    
    inputnode = Node(
        interface=IdentityInterface(
            fields=['B0', 'mask', 'fieldStrength']),
        name='inputnode'
    )
    outputnode = Node(
        interface=IdentityInterface(
            fields=['qsm']),
        name='outputnode'
    )
    
    B0_normalize = Node(
        interface=nextqsm_interface.NormalizeB0Interface(
            out_suffix='_B0_normalized'
        ),
        name='nextqsm_normalize_B0'
        # output: 'out_file'
    )
    qsm = Node(
        interface=nextqsm_interface.NextqsmInterface(),
        iterfield=['phase', 'mask'],
        name='nextqsm'
        # output: 'out_file'
    )
    
    wf.connect([
        (inputnode, B0_normalize, [('fieldStrength', 'fieldStrength'),
                                     ('B0', 'B0_file')]),
        (inputnode, qsm, [('mask', 'mask')]),
        (B0_normalize, qsm, [('out_file', 'phase')]),
        (qsm, outputnode, [('out_file', 'qsm_final')]),
    ])
    
    # Phase-based Maskung on scaled B0
    if use_B0_mask:
        phaseweights = Node(
            interface=phaseweights.PhaseWeightsInterface(),
            iterfield=['in_file'],
            name='romeo_B0-weights'
            # output: 'out_file'
        )
        mask = Node(
            interface=ImageMaths(
                suffix='_mask',
                op_string=f'-thrp {B0_threshold} -bin -ero -dilM'
            ),
            iterfield=['in_file'],
            name='fslmaths_B0-mask'
            # input  : 'in_file'
            # output : 'out_file'
        )
        wf.disconnect([
            (inputnode, qsm, [('mask', 'mask')]),
        ])
        wf.connect([
            (inputnode, phaseweights, [('B0', 'in_file')]),
            (phaseweights, mask, [('out_file', 'in_file')]),
            (mask, qsm, [('out_file', 'mask')]),
        ])
    
    return wf
    