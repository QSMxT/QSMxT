from nipype.pipeline.engine import Node, MapNode

from interfaces import nipype_interface_tgv_qsm as tgv
from interfaces import nipype_interface_nonzeroaverage as nonzeroaverage
from interfaces import nipype_interface_twopass as twopass
from interfaces import nipype_interface_axialsampling as sampling

def add_tgvqsm_workflow(wf, run_args, mn_params, mn_inputs, mn_mask, n_datasink, magnitude_file):
    # === Single-pass QSM reconstruction (filled) ===
    mn_qsm_filled = MapNode(
        interface=tgv.QSMappingInterface(
            iterations=run_args.tgvqsm_iterations,
            alpha=run_args.tgvqsm_alphas,
            erosions=0 if run_args.two_pass else 5,
            num_threads=run_args.tgvqsm_threads,
            out_suffix='_qsm-filled',
            extra_arguments='--ignore-orientation --no-resampling'
        ),
        iterfield=['phase_file', 'TE', 'b0', 'mask_file'],
        name='tgv-qsm_filled'
        # inputs: 'phase_file', 'TE', 'b0', 'mask_file'
        # output: 'out_file'
    )
    mn_qsm_filled.estimated_memory_gb = 6
    mn_qsm_filled.plugin_args = {
        'qsub_args': f'-A {run_args.qsub_account_string} -l walltime=03:00:00 -l select=1:ncpus={run_args.tgvqsm_threads}:mem=20gb:vmem=20gb',
        'overwrite': True
    }
    wf.connect([
        (mn_params, mn_qsm_filled, [('EchoTime', 'TE')]),
        (mn_params, mn_qsm_filled, [('MagneticFieldStrength', 'b0')]),
        (mn_mask, mn_qsm_filled, [('masks_filled', 'mask_file')]),
        (mn_inputs, mn_qsm_filled, [('phase_files', 'phase_file')]),
    ])

    # qsm averaging
    n_qsm_filled_average = Node(
        interface=nonzeroaverage.NonzeroAverageInterface(),
        name='numpy_nibabel_qsm-filled-average'
        # input : in_files
        # output : out_file
    )
    wf.connect([
        (mn_qsm_filled, n_qsm_filled_average, [('out_file', 'in_files')])
    ])

    # resample qsm to original
    n_resample_qsm = Node(
        interface=sampling.ResampleLikeInterface(
            in_like=magnitude_file
        ),
        name='nibabel_numpy_nilearn_resample-qsm'
    )
    wf.connect([
        (n_qsm_filled_average, n_resample_qsm, [('out_file', 'in_file')]),
        (n_resample_qsm, n_datasink, [('out_file', 'qsm_singlepass' if run_args.two_pass else 'qsm_final')]),
    ])

    # === Two-pass QSM reconstruction (not filled) ===
    if run_args.two_pass:
        mn_qsm = MapNode(
            interface=tgv.QSMappingInterface(
                iterations=run_args.tgvqsm_iterations,
                alpha=run_args.tgvqsm_alphas,
                erosions=0,
                num_threads=run_args.tgvqsm_threads,
                out_suffix='_qsm',
                extra_arguments='--ignore-orientation --no-resampling'
            ),
            iterfield=['phase_file', 'TE', 'b0', 'mask_file'],
            name='tgv-qsm_intermediate'
            # inputs: 'phase_file', 'TE', 'b0', 'mask_file'
            # output: 'out_file'
        )
        mn_qsm.estimated_memory_gb = 6

        # args for PBS
        mn_qsm.plugin_args = {
            'qsub_args': f'-A {run_args.qsub_account_string} -l walltime=03:00:00 -l select=1:ncpus={run_args.tgvqsm_threads}:mem=20gb:vmem=20gb',
            'overwrite': True
        }

        wf.connect([
            (mn_params, mn_qsm, [('EchoTime', 'TE')]),
            (mn_params, mn_qsm, [('MagneticFieldStrength', 'b0')]),
            (mn_mask, mn_qsm, [('masks', 'mask_file')]),
            (mn_inputs, mn_qsm, [('phase_files', 'phase_file')])
        ])

        # qsm averaging
        n_qsm_average = Node(
            interface=nonzeroaverage.NonzeroAverageInterface(),
            name='numpy_nibabel_qsm-average'
            # input : in_files
            # output : out_file
        )
        wf.connect([
            (mn_qsm, n_qsm_average, [('out_file', 'in_files')])
        ])

        # Two-pass combination step
        mn_qsm_twopass = MapNode(
            interface=twopass.TwopassNiftiInterface(),
            name='numpy_nibabel_twopass',
            iterfield=['in_file1', 'in_file2']
        )
        wf.connect([
            (mn_qsm, mn_qsm_twopass, [('out_file', 'in_file1')]),
            (mn_qsm_filled, mn_qsm_twopass, [('out_file', 'in_file2')]),
            #(mn_mask, mn_qsm_twopass, [('mask_file', 'in_maskFile')])
        ])

        n_qsm_twopass_average = Node(
            interface=nonzeroaverage.NonzeroAverageInterface(),
            name='numpy_nibabel_twopass-average'
            # input : in_files
            # output: out_file
        )
        wf.connect([
            (mn_qsm_twopass, n_qsm_twopass_average, [('out_file', 'in_files')])
        ])

        # resample qsm to original
        n_resample_qsm_twopass = Node(
            interface=sampling.ResampleLikeInterface(
                in_like=magnitude_file
            ),
            name='nibabel_numpy_nilearn_resample-qsm-twopass'
        )
        wf.connect([
            (n_qsm_twopass_average, n_resample_qsm_twopass, [('out_file', 'in_file')]),
            (n_resample_qsm_twopass, n_datasink, [('out_file', 'qsm_final')]),
        ])

    return wf
