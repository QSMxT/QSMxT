from nipype.pipeline.engine import Workflow, MapNode, Node
from nipype.interfaces.utility import IdentityInterface
from nipype.interfaces.fsl import ImageMaths
import interfaces.nipype_interface_masking as masking_interfaces
import interfaces.nipype_interface_threshold as threshold_interface


def masking_workflow(masking_setting, extra_fill_strength):
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
    
    mask_filled = MapNode(
        interface=ImageMaths(
            suffix='_fillh',
            op_string="-fillh" if not extra_fill_strength else " ".join(
                ["-dilM" for _ in range(extra_fill_strength)] 
                + ["-fillh"] 
                + ["-ero" for _ in range(extra_fill_strength)]
            )
        ),
        iterfield=['in_file'],
        name='fslmaths_mask-filled'
    )
    ero_dilM = MapNode(
        interface=ImageMaths(
            suffix='_ero_dilM',
            op_string="-ero -dilM"
        ),
        iterfield=['in_file'],
        name='fslmaths_mask-ero_dilM'
    )
    wf.connect([
        (mask_filled, ero_dilM, [('out_file', 'in_file')]),
        (ero_dilM, outputnode, [('out_file', 'mask')])
    ])
        
    if masking_type == "hagberg-phase-based":
        pb_mask = MapNode(
            interface=masking_interfaces.PbMaskingInterface(),
            iterfield=['phase'],
            name='hagberg-phase-based-masking'
        )
        wf.connect([
            (inputnode, pb_mask, [('phase', 'phase')]),
            (pb_mask, mask_filled, [('mask', 'in_file')])
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
            (mn_phasemask, mask_filled, [('out_file', 'in_file')])
        ])
    elif masking_type == 'gaussian-based':
        n_threshold = Node(
            interface=threshold_interface.ThresholdInterface(),
            iterfield=['in_files'],
            name='automated-threshold'
        )
        mn_gaussmask = MapNode(
                interface=ImageMaths(
                    suffix="_mask"
                ),
                iterfield=['in_file', 'op_string'],
                name='automated_threshold-mask'
                # output: 'out_file'
            )

        wf.connect([
            (inputnode, n_threshold, [('mag', 'in_files')]),
            (inputnode, mn_gaussmask, [('mag', 'in_file')]),
            (n_threshold, mn_gaussmask, [('op_string', 'op_string')]),
            (mn_gaussmask, mask_filled, [('out_file', 'in_file')])
        ])  
    

    return wf
