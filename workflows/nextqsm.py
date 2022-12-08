from nipype.pipeline.engine import Workflow, Node, MapNode
from nipype.interfaces.utility import IdentityInterface
from nipype.interfaces.fsl import ImageMaths

from workflows.unwrapping import unwrapping_workflow
from interfaces import nipype_interface_nextqsm as nextqsm_interface
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
    nextqsm.estimated_memory_gb = 13
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


def add_b0nextqsm_workflow(wf, mn_inputs, mn_params, mn_mask, n_datasink):
    # extract the fieldstrength of the first echo for input to nextqsm Node (not MapNode)
    def first(list=None):
        return list[0]
    n_fieldStrength = Node(Function(input_names="list",
                                    output_names=["fieldStrength"],
                                    function=first),
                            name='extract_fieldStrength')
    n_mask = Node(Function(input_names="list", # TODO try to use B0 phase-based mask
                                        output_names=["out_file"],
                                        function=first),
                                name='extract_Mask')
    wf_unwrapping = unwrapping_workflow("romeoB0")
    wf_nextqsmB0 = nextqsm_B0_workflow()
    
    wf.connect([
        (mn_inputs, wf_unwrapping, [('phase_files', 'inputnode.wrapped_phase')]),
        (mn_inputs, wf_unwrapping, [('magnitude_files', 'inputnode.mag')]),
        (mn_params, wf_unwrapping, [('EchoTime', 'inputnode.TE')]),
        
        (wf_unwrapping, wf_nextqsmB0, [('outputnode.B0', 'inputnode.B0'),]),
        
        (mn_mask, n_mask, [('masks_filled', 'list'),]),
        (n_mask, wf_nextqsmB0, [('out_file', 'inputnode.mask'),]),
        (mn_params, n_fieldStrength, [('MagneticFieldStrength', 'list')]),
        (n_fieldStrength, wf_nextqsmB0, [('fieldStrength', 'inputnode.fieldStrength'),]),
        
        (wf_nextqsmB0, n_datasink, [('outputnode.qsm', 'final_qsm')]),
    ])

    return wf


def add_nextqsm_workflow(wf, run_args, mn_inputs, mn_params, mn_mask, n_datasink):
    wf_unwrapping = unwrapping_workflow(run_args.unwrapping_algorithm)
    wf_nextqsm = nextqsm_workflow()
    
    wf.connect([
        (mn_inputs, wf_unwrapping, [('phase_files', 'inputnode.wrapped_phase')]),
        (mn_inputs, wf_unwrapping, [('magnitude_files', 'inputnode.mag')]),
        (mn_params, wf_unwrapping, [('EchoTime', 'inputnode.TE')]),
        
        (wf_unwrapping, wf_nextqsm, [('outputnode.unwrapped_phase', 'inputnode.unwrapped_phase')]),
        (mn_mask, wf_nextqsm, [('masks_filled', 'inputnode.mask')]),
        (mn_params, wf_nextqsm, [('EchoTime', 'inputnode.TE'),
                                 ('MagneticFieldStrength', 'inputnode.fieldStrength')]),
        
        (wf_nextqsm, n_datasink, [('outputnode.qsm', 'qsm_echo'),
                                  ('outputnode.qsm_average', 'qsm_final')])
    ])

    return wf

