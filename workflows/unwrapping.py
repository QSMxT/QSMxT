from nipype.pipeline.engine import Workflow, MapNode, Node
from nipype.interfaces.utility import IdentityInterface
import interfaces.nipype_interface_romeo as romeo_interface
import interfaces.nipype_interface_laplacian_unwrapping as laplacian_interface


def unwrapping_workflow(unwrapping='laplacian'):
    wf = Workflow(name=unwrapping + "_workflow")
    
    inputnode = MapNode(
        interface=IdentityInterface(fields=['wrapped_phase', 'mag', 'TE']),
        iterfield=['wrapped_phase', 'mag', 'TE'],
        name='inputnode')
    
    outputnode = MapNode(
        interface=IdentityInterface(fields=['unwrapped_phase', 'B0']),
        iterfield=['unwrapped_phase'],
        name='outputnode')
    
    if unwrapping == "laplacian":
        laplacian = MapNode(
            interface=laplacian_interface.LaplacianInterface(),
            iterfield=['phase'],
            name='phase_unwrap_laplacian'
        )
        wf.connect([
            (inputnode, laplacian, [('wrapped_phase', 'phase')]),
            (laplacian, outputnode, [('out_file', 'unwrapped_phase')])
        ])        
        
    elif unwrapping == "romeo":
        romeo = MapNode(
            interface=romeo_interface.RomeoInterface(),
            iterfield=['phase', 'mag'],
            name='phase_unwrap_romeo'
        )
        wf.connect([
            (inputnode, romeo, [('wrapped_phase', 'phase'),
                                ('mag', 'mag')]),
            (romeo, outputnode, [('out_file', 'unwrapped_phase')])
        ])
        
    elif unwrapping == "romeoB0":
        romeo = Node(
        interface=romeo_interface.RomeoB0Interface(),
        name='phase_unwrap_romeo_B0'
        )
        wf.connect([
            (inputnode, romeo, [('wrapped_phase', 'phase'),
                                ('mag', 'mag'),
                                ('TE', 'TE')]),
            (romeo, outputnode, [('unwrapped_phase', 'unwrapped_phase'),
                                ('B0', 'B0')])
        ])
        
    return wf
