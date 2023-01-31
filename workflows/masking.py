from nipype.pipeline.engine import Workflow, Node, MapNode
from nipype.interfaces.utility import IdentityInterface, Function

from interfaces import nipype_interface_masking as masking
from interfaces import nipype_interface_erode as erode
from interfaces import nipype_interface_bet2 as bet2
from interfaces import nipype_interface_phaseweights as phaseweights
from interfaces import nipype_interface_twopass as twopass

def masking_workflow(run_args, mask_files, magnitude_available, fill_masks, add_bet, name, index):

    wf = Workflow(name=f"{name}_workflow")

    n_inputs = Node(
        interface=IdentityInterface(
            fields=['phase', 'magnitude', 'mask']
        ),
        name='masking_inputs'
    )

    n_outputs = Node(
        interface=IdentityInterface(
            fields=['mask', 'threshold']
        ),
        name='masking_outputs'
    )

    if not mask_files:
        mn_erode = MapNode(
            interface=erode.ErosionInterface(
                num_erosions=run_args.mask_erosions[index % len(run_args.mask_erosions)]
            ),
            iterfield=['in_file'],
            name='scipy_numpy_nibabel_erode'
        )

        # do phase weights if necessary
        if run_args.masking_algorithm == 'threshold' and run_args.masking_input == 'phase' and not (fill_masks and run_args.filling_algorithm == 'bet'):
            mn_phaseweights = MapNode(
                interface=phaseweights.RomeoMaskingInterface(),
                iterfield=['phase', 'magnitude'] if magnitude_available else ['phase'],
                name='romeo-voxelquality',
                mem_gb=3
            )
            mn_phaseweights.inputs.weight_type = "grad+second"
            wf.connect([
                (n_inputs, mn_phaseweights, [('phase', 'phase')]),
            ])
            if magnitude_available:
                mn_phaseweights.inputs.weight_type = "grad+second+mag"
                wf.connect([
                    (n_inputs, mn_phaseweights, [('magnitude', 'magnitude')])
                ])

        # do threshold masking if necessary
        if run_args.masking_algorithm == 'threshold' and not (fill_masks and run_args.filling_algorithm == 'bet'):
            n_threshold_masking = Node(
                interface=masking.MaskingInterface(
                    threshold_algorithm=run_args.threshold_algorithm,
                    threshold_algorithm_factor=run_args.threshold_algorithm_factor[index % len(run_args.threshold_algorithm_factor)],
                    fill_masks=fill_masks,
                    mask_suffix=name,
                    filling_algorithm=run_args.filling_algorithm
                ),
                name='scipy_numpy_nibabel_threshold-masking'
                # inputs : ['in_files']
            )
            if run_args.threshold_value[index % len(run_args.threshold_value)]:
                n_threshold_masking.inputs.threshold = run_args.threshold_value[index % len(run_args.threshold_value)]

            if run_args.masking_input == 'phase':    
                wf.connect([
                    (mn_phaseweights, n_threshold_masking, [('quality_map', 'in_files')])
                ])
            elif run_args.masking_input == 'magnitude':
                wf.connect([
                    (n_inputs, n_threshold_masking, [('magnitude', 'in_files')])
                ])
            if not add_bet:
                wf.connect([
                    (n_threshold_masking, mn_erode, [('mask', 'in_file')])
                ])

        # run bet if necessary
        if run_args.masking_algorithm in ['bet', 'bet-firstecho'] or add_bet or (run_args.filling_algorithm == 'bet' and fill_masks):
            mn_bet = MapNode(
                interface=bet2.Bet2Interface(fractional_intensity=run_args.bet_fractional_intensity),
                iterfield=['in_file'],
                name='fsl-bet'
            )
            if run_args.masking_algorithm == 'bet-firstecho':
                def get_first(magnitude): return [magnitude[0] for f in magnitude]
                n_getfirst = Node(
                    interface=Function(
                        input_names=['magnitude'],
                        output_names=['magnitude'],
                        function=get_first
                    ),
                    name='func_get-first'
                )
                wf.connect([
                    (n_inputs, n_getfirst, [('magnitude', 'magnitude')])
                ])
                wf.connect([
                    (n_getfirst, mn_bet, [('magnitude', 'in_file')])
                ])
            else:
                wf.connect([
                    (n_inputs, mn_bet, [('magnitude', 'in_file')])
                ])

            # add bet to threshold-based mask if necessary
            if add_bet:
                mn_mask_plus_bet = MapNode(
                    interface=twopass.TwopassNiftiInterface(),
                    name='numpy_nibabel_mask-plus-bet',
                    iterfield=['in_file1', 'in_file2'],
                )
                wf.connect([
                    (n_threshold_masking, mn_mask_plus_bet, [('mask', 'in_file1')]),
                    (mn_bet, mn_mask_plus_bet, [('mask', 'in_file2')])
                ])
                wf.connect([
                    (mn_mask_plus_bet, mn_erode, [('out_file', 'in_file')])
                ])
            else:
                wf.connect([
                    (mn_bet, mn_erode, [('mask', 'in_file')])
                ])

    # outputs
    if mask_files:
        wf.connect([
            (n_inputs, n_outputs, [('mask', 'mask')]),
        ])
    else:
        wf.connect([
            (mn_erode, n_outputs, [('out_file', 'mask')]),
        ])
        if run_args.masking_algorithm == 'threshold' and not (fill_masks and run_args.filling_algorithm == 'bet'):
            wf.connect([
                (n_threshold_masking, n_outputs, [('threshold', 'threshold')])
            ])

    return wf

