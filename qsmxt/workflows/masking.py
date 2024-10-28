import numpy as np

from nipype.pipeline.engine import Workflow
from nipype.interfaces.utility import IdentityInterface, Function

from qsmxt.interfaces import nipype_interface_masking as masking
from qsmxt.interfaces import nipype_interface_erode as erode
from qsmxt.interfaces import nipype_interface_bet2 as bet2
from qsmxt.interfaces import nipype_interface_hdbet as hdbet
from qsmxt.interfaces import nipype_interface_phaseweights as phaseweights
from qsmxt.interfaces import nipype_interface_twopass as twopass
from qsmxt.interfaces import nipype_interface_makehomogeneous as makehomogeneous
from qsmxt.interfaces import nipype_interface_combinemagnitude as combinemagnitude

from qsmxt.scripts.qsmxt_functions import gen_plugin_args, create_node

def masking_workflow(run_args, mask_available, magnitude_available, qualitymap_available, fill_masks, add_bet, use_maps, name, dimensions_phase, bytepix_phase, num_echoes, index):

    wf = Workflow(name=f"{name}_workflow")

    slurm_account = run_args.slurm[0] if run_args.slurm and len(run_args.slurm) else None
    slurm_partition = run_args.slurm[1] if run_args.slurm and len(run_args.slurm) > 1 else None

    n_inputs = create_node(
        interface=IdentityInterface(
            fields=['phase', 'quality_map', 'magnitude', 'mask', 'TE']
        ),
        name='masking_inputs'
    )

    n_outputs = create_node(
        interface=IdentityInterface(
            fields=['mask', 'threshold', 'quality_map']
        ),
        name='masking_outputs'
    )

    if not mask_available:

        # determine whether bet will be used this run
        bet_used = magnitude_available and (
             run_args.masking_algorithm == 'bet'
             or run_args.add_bet
             or run_args.filling_algorithm == 'bet')
        bet_this_run = bet_used and (fill_masks or (run_args.mask_erosions and run_args.masking_algorithm == 'threshold' and not fill_masks))

        # combine magnitude if necessary
        if magnitude_available and run_args.combine_phase:
            n_combine_magnitude_mem = (np.prod(dimensions_phase) * 8) / (1024 ** 3) * num_echoes * 4
            n_combine_magnitude = create_node(
                interface=combinemagnitude.CombineMagnitudeInterface(),
                name='nibabel-numpy_combine-magnitude',
                mem_gb=n_combine_magnitude_mem
            )
            n_combine_magnitude.plugin_args = gen_plugin_args(
                plugin_args={ 'overwrite': True },
                slurm_account=slurm_account,
                pbs_account=run_args.pbs,
                slurm_partition=slurm_partition,
                name="comb-mag",
                time="01:00:00",
                mem_gb=n_combine_magnitude_mem
            )
            wf.connect([
                (n_inputs, n_combine_magnitude, [('magnitude', 'magnitude')])
            ])

        # get first phase image if necessary
        if run_args.combine_phase and run_args.masking_input == 'phase':
            n_getfirst_phase = create_node(
                interface=Function(
                    input_names=['phase', 'TE', 'is_list'],
                    output_names=['phase', 'TE'],
                    function=lambda phase, TE, is_list: [phase[0], TE[0]] if is_list else [phase, TE]
                ),
                name='func_get-first-phase'
            )
            n_getfirst_phase.inputs.is_list = True
            wf.connect([
                (n_inputs, n_getfirst_phase, [('phase', 'phase')]),
                (n_inputs, n_getfirst_phase, [('TE', 'TE')])
            ])

        # correct magnitude if necessary
        if run_args.inhomogeneity_correction and (bet_this_run or run_args.masking_input == 'magnitude'):
            mn_inhomogeneity_correction = create_node(
                interface=makehomogeneous.MakeHomogeneousInterface(),
                iterfield=['magnitude'],
                name='mrt_correct-inhomogeneity',
                is_map=use_maps
            )

            if run_args.combine_phase:
                wf.connect([
                    (n_combine_magnitude, mn_inhomogeneity_correction, [('magnitude_combined', 'magnitude')]),
                ])
            else:
                wf.connect([
                    (n_inputs, mn_inhomogeneity_correction, [('magnitude', 'magnitude')])
                ])

        # do phase weights if necessary
        if run_args.masking_algorithm == 'threshold' and run_args.masking_input == 'phase':
            mn_phaseweights_mem = 2.80122 * (np.prod(dimensions_phase) * 8 / (1024 ** 3)) + 0.95 # DONE
            mn_phaseweights_threads = 1
            if qualitymap_available:
                mn_phaseweights = create_node(
                    interface=IdentityInterface(['quality_map']),
                    name='romeo-voxelquality',
                    n_procs=mn_phaseweights_threads,
                    mem_gb=mn_phaseweights_mem,
                )
                wf.connect([(n_inputs, mn_phaseweights, [('quality_map', 'quality_map')])])
            else:
                mn_phaseweights = create_node(
                    interface=phaseweights.RomeoMaskingInterface(),
                    iterfield=['phase', 'magnitude'] if magnitude_available else ['phase'],
                    name='romeo-voxelquality',
                    mem_gb=mn_phaseweights_mem,
                    n_procs=mn_phaseweights_threads,
                    is_map=use_maps
                )
                mn_phaseweights.inputs.weight_type = "grad+second"
                wf.connect([
                    (n_getfirst_phase if (run_args.combine_phase and run_args.masking_input == 'phase') else n_inputs, mn_phaseweights, [('phase', 'phase')]),
                    (n_getfirst_phase if (run_args.combine_phase and run_args.masking_input == 'phase') else n_inputs, mn_phaseweights, [('TE', 'TEs' if use_maps else 'TE')]),
                    (mn_phaseweights, n_outputs, [('quality_map', 'quality_map')])
                ])
                if magnitude_available:
                    mn_phaseweights.inputs.weight_type = "grad+second+mag_weight+mag_coherence"
                    if run_args.combine_phase:
                        wf.connect([
                            (n_combine_magnitude, mn_phaseweights, [('magnitude_combined', 'magnitude')])
                        ])
                    else:
                        wf.connect([
                            (n_inputs, mn_phaseweights, [('magnitude', 'magnitude')])
                        ])
            
            mn_phaseweights.plugin_args = gen_plugin_args(
                plugin_args={ 'overwrite': True },
                slurm_account=slurm_account,
                pbs_account=run_args.pbs,
                slurm_partition=slurm_partition,
                name="voxelquality",
                time="01:00:00",
                mem_gb=mn_phaseweights_mem,
                num_cpus=mn_phaseweights_threads
            )

        # do bet mask if necessary
        if bet_this_run:
            bet_threads = min(8, run_args.n_procs) if run_args.multiproc else 8
            bet_mem = (np.prod(dimensions_phase) * bytepix_phase) / (1024 ** 3) * 10
            mn_bet = create_node(
                interface=bet2.Bet2Interface(fractional_intensity=run_args.bet_fractional_intensity),
                iterfield=['in_file'],
                name='fsl-bet',
                mem_gb=bet_mem,
                n_procs=bet_threads,
                is_map=use_maps
            )
            mn_bet.plugin_args = gen_plugin_args(
                plugin_args={ 'overwrite': True },
                slurm_account=slurm_account,
                pbs_account=run_args.pbs,
                slurm_partition=slurm_partition,
                name="bet",
                time="01:00:00",
                mem_gb=bet_mem,
                num_cpus=bet_threads
            )
            if run_args.inhomogeneity_correction:
                wf.connect([
                    (mn_inhomogeneity_correction, mn_bet, [('magnitude_corrected', 'in_file')])
                ])
            elif run_args.combine_phase:
                wf.connect([
                    (n_combine_magnitude, mn_bet, [('magnitude_combined', 'in_file')])
                ])
            else:
                wf.connect([
                    (n_inputs, mn_bet, [('magnitude', 'in_file')])
                ])

            # erode bet mask
            mn_bet_erode = create_node(
                interface=erode.ErosionInterface(
                    num_erosions=run_args.mask_erosions[index % len(run_args.mask_erosions)] if run_args.mask_erosions else 0
                ),
                iterfield=['in_file'],
                name='scipy_numpy_nibabel_bet_erode',
                is_map=use_maps
            )
            wf.connect([
                (mn_bet, mn_bet_erode, [('mask', 'in_file')])
            ])

            # output eroded bet mask if necessary
            if run_args.masking_algorithm == 'bet' or (fill_masks and run_args.filling_algorithm == 'bet'):
                wf.connect([
                    (mn_bet_erode, n_outputs, [('out_file', 'mask')])
                ])

        # do threshold masking if necessary
        if run_args.masking_algorithm == 'threshold' and not (fill_masks and run_args.filling_algorithm == 'bet'):
            n_threshold_masking = create_node(
                interface=masking.MaskingInterface(
                    threshold_algorithm='otsu' or run_args.threshold_algorithm,
                    threshold_algorithm_factor=run_args.threshold_algorithm_factor[index % len(run_args.threshold_algorithm_factor)],
                    fill_masks=fill_masks,
                    mask_suffix=name,
                    num_erosions=run_args.mask_erosions[index % len(run_args.mask_erosions)] if run_args.mask_erosions else 0,
                    filling_algorithm=run_args.filling_algorithm
                ),
                name='scipy_numpy_nibabel_threshold-masking'
                # inputs : ['in_files']
            )
            if run_args.threshold_value:
                n_threshold_masking.inputs.threshold = run_args.threshold_value[index % len(run_args.threshold_value)]
            if bet_this_run:
                wf.connect([
                    (mn_bet_erode, n_threshold_masking, [('out_file', 'bet_masks')])
                ])
            if run_args.masking_input == 'phase':
                wf.connect([
                    (mn_phaseweights, n_threshold_masking, [('quality_map', 'in_files')])
                ])
            elif run_args.masking_input == 'magnitude':
                if run_args.inhomogeneity_correction:
                    wf.connect([
                        (mn_inhomogeneity_correction, n_threshold_masking, [('magnitude_corrected', 'in_files')])
                    ])
                elif run_args.combine_phase:
                    wf.connect([
                        (n_combine_magnitude, n_threshold_masking, [('magnitude_combined', 'in_files')])
                    ])
                else:
                    wf.connect([
                        (n_inputs, n_threshold_masking, [('magnitude', 'in_files')])
                    ])
            if not add_bet:
                wf.connect([
                    (n_threshold_masking, n_outputs, [('mask', 'mask')])
                ])
            else:
                mn_mask_plus_bet = create_node(
                    interface=twopass.TwopassNiftiInterface(),
                    name='numpy_nibabel_mask-plus-bet',
                    iterfield=['in_file', 'in_filled'],
                    is_map=use_maps
                )
                wf.connect([
                    (n_threshold_masking, mn_mask_plus_bet, [('mask', 'in_file')]),
                    (mn_bet_erode, mn_mask_plus_bet, [('out_file', 'in_filled')]),
                    (mn_mask_plus_bet, n_outputs, [('out_file', 'mask')])
                ])

    # outputs
    if mask_available:
        wf.connect([
            (n_inputs, n_outputs, [('mask', 'mask')]),
        ])
    else:
        if run_args.masking_algorithm == 'threshold' and not (fill_masks and run_args.filling_algorithm == 'bet'):
            wf.connect([
                (n_threshold_masking, n_outputs, [('threshold', 'threshold')])
            ])

    return wf

