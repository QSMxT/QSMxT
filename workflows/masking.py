from nipype.pipeline.engine import Workflow, Node, MapNode
from nipype.interfaces.utility import IdentityInterface, Function

from interfaces import nipype_interface_masking as masking
from interfaces import nipype_interface_erode as erode
from interfaces import nipype_interface_bet2 as bet2
from interfaces import nipype_interface_phaseweights as phaseweights
from interfaces import nipype_interface_twopass as twopass

def masking_workflow(run_args, mn_inputs, mask_files, magnitude_available, fill_masks, add_bet, name):

    wf = Workflow(name=f"{name}_workflow")

    mn_outputs = MapNode(
        interface=IdentityInterface(
            fields=['masks', 'threshold']
        ),
        iterfield=['masks', 'threshold'],
        name='masking_outputs'
    )

    if not mask_files:
        # do phase weights if necessary
        if run_args.masking == 'phase-based':
            mn_phaseweights = MapNode(
                interface=phaseweights.RomeoMaskingInterface(),
                iterfield=['phase', 'mag'] if magnitude_available else ['phase'],
                name='romeo-voxelquality'
                # output: 'out_file'
            )
            if magnitude_available:
                mn_phaseweights.inputs.weight_type = "grad+second+mag"
                wf.connect([
                    (mn_inputs, mn_phaseweights, [('phase_files', 'phase')]),
                    (mn_inputs, mn_phaseweights, [('magnitude_files', 'mag')])
                ])
            else:
                mn_phaseweights.inputs.weight_type = "grad+second"
                wf.connect([
                    (mn_inputs, mn_phaseweights, [('phase_files', 'phase')]),
                ])

        # do threshold-based masking if necessary
        if run_args.masking in ['phase-based', 'magnitude-based']:
            n_threshold_masking = Node(
                interface=masking.MaskingInterface(
                    fill_masks=fill_masks,
                    mask_suffix=name
                ),
                name='scipy_numpy_nibabel_threshold-masking'
                # inputs : ['in_files']
            )
            if run_args.masking_threshold: n_threshold_masking.inputs.threshold = run_args.masking_threshold

            if run_args.masking in ['phase-based']:    
                wf.connect([
                    (mn_phaseweights, n_threshold_masking, [('out_file', 'in_files')])
                ])
            elif run_args.masking == 'magnitude-based':
                wf.connect([
                    (mn_inputs, n_threshold_masking, [('magnitude_files', 'in_files')])
                ])

        # run bet if necessary
        if run_args.masking in ['bet', 'bet-firstecho'] or add_bet:
            def get_first(magnitude_files): return [magnitude_files[0] for f in magnitude_files]
            n_getfirst = Node(
                interface=Function(
                    input_names=['magnitude_files'],
                    output_names=['magnitude_file'],
                    function=get_first
                ),
                name='func_get-first'
            )
            wf.connect([
                (mn_inputs, n_getfirst, [('magnitude_files', 'magnitude_files')])
            ])

            mn_bet = MapNode(
                interface=bet2.Bet2Interface(fractional_intensity=run_args.bet_fractional_intensity),
                iterfield=['in_file'],
                name='fsl-bet'
                # output: 'mask_file'
            )
            if run_args.masking == 'bet-firstecho':
                wf.connect([
                    (n_getfirst, mn_bet, [('magnitude_file', 'in_file')])
                ])
            else:
                wf.connect([
                    (mn_inputs, mn_bet, [('magnitude_files', 'in_file')])
                ])
            mn_bet_erode = MapNode(
                interface=erode.ErosionInterface(
                    num_erosions=2
                ),
                iterfield=['in_file'],
                name='scipy_numpy_nibabel_erode'
            )
            wf.connect([
                (mn_bet, mn_bet_erode, [('mask_file', 'in_file')])
            ])

            # add bet if necessary
            if add_bet:
                mn_mask_plus_bet = MapNode(
                    interface=twopass.TwopassNiftiInterface(),
                    name='numpy_nibabel_mask-plus-bet',
                    iterfield=['in_file1', 'in_file2'],
                )
                wf.connect([
                    (n_threshold_masking, mn_mask_plus_bet, [('masks', 'in_file1')]),
                    (mn_bet_erode, mn_mask_plus_bet, [('out_file', 'in_file2')])
                ])

    # outputs
    if mask_files:
        wf.connect([
            (mn_inputs, mn_outputs, [('mask_files', 'masks')]),
        ])
    elif run_args.masking in ['bet', 'bet-firstecho']:
        wf.connect([
            (mn_bet, mn_outputs, [('mask_file', 'masks')]),
        ])
    elif run_args.masking in ['magnitude-based', 'phase-based']:
        wf.connect([
            (n_threshold_masking, mn_outputs, [('threshold', 'threshold')])
        ])
        if not add_bet:
            wf.connect([
                (n_threshold_masking, mn_outputs, [('masks', 'masks')]),
            ])
        else:
            wf.connect([
                (mn_mask_plus_bet, mn_outputs, [('out_file', 'masks')])
            ])

    return wf

