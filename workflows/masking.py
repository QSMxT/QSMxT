from nipype.pipeline.engine import Workflow, MapNode
from nipype.interfaces.utility import IdentityInterface
from nipype.interfaces.fsl import ImageMaths
import interfaces.nipype_interface_masking as masking_interfaces


def masking_workflow(masking_setting):
    masking_type=masking_setting[0]
    threshold = 0.5
    romeo_weights = 'grad+second'
    for entry in masking_setting[2:]:
        try:
            threshold = float(entry)
        except:
            romeo_weights = entry            

    wf = Workflow(name=masking_type + "_workflow")
    
    inputnode = MapNode(
        interface=IdentityInterface(
            fields=['phase', 'mag']),
        iterfield=['phase', 'mag'],
        name='inputnode')
    
    outputnode = MapNode(
        interface=IdentityInterface(
            fields=['mask']),
        iterfield=['mask'],
        name='outputnode')
    
    if masking_type == "hagberg-phase-based":
        pb_mask = MapNode(
            interface=masking_interfaces.PbMaskingInterface(),
            iterfield=['phase'],
            name='hagberg-phase-based-masking'
        )
        wf.connect([
            (inputnode, pb_mask, [('phase', 'phase')]),
            (pb_mask, outputnode, [('mask', 'mask')])
        ])        
        
    elif masking_type == "romeo-phase-based":
        
        romeo = MapNode(
            interface=masking_interfaces.RomeoMaskingInterface(),
            iterfield=['phase', 'mag'],
            name='romeo-voxelquality'
        )
        romeo.inputs.weight_type = romeo_weights
        mn_phasemask = MapNode(
            interface=ImageMaths(
                suffix='_mask',
                op_string=f'-thr {threshold} -bin'
            ),
            iterfield=['in_file'],
            name='fslmaths_phase-mask'
        )
        wf.connect([
            (inputnode, romeo, [('phase', 'phase'),
                                ('mag', 'mag')]),
            (romeo, mn_phasemask, [('voxelquality', 'in_file')]),
            (mn_phasemask, outputnode, [('out_file', 'mask')])
        ])
        
    return wf
