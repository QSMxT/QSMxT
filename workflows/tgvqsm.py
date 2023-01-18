from nipype.pipeline.engine import Workflow, Node, MapNode
from nipype.interfaces.utility import IdentityInterface, Function

from workflows.unwrapping import unwrapping_workflow
from interfaces import nipype_interface_tgv_qsm as tgv
from interfaces import nipype_interface_nonzeroaverage as nonzeroaverage
from interfaces import nipype_interface_twopass as twopass
from interfaces import nipype_interface_axialsampling as sampling


def tgvqsm_workflow(run_args, phase_file, name="tgvqsm"):
    wf = Workflow(name)

    inputnode = MapNode(
        interface=IdentityInterface(
            fields=['unwrapped_phase', 'mask', 'TE', 'fieldStrength']),
        iterfield=['unwrapped_phase', 'mask', 'TE', 'fieldStrength'],
        name='inputnode'
    )
# 'tgvqsm_iterations', 'tgvqsm_alphas', 'two_pass', 'tgvqsm_threads', 
    outputnode = MapNode(
        interface=IdentityInterface(
            fields=['qsm_singlepass', 'qsm_twopass']),
        iterfield=['qsm_singlepass', 'qsm_twopass'],
        name='outputnode'
    )

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
        'qsub_args': f'-A {run_args.pbs} -l walltime=03:00:00 -l select=1:ncpus={run_args.tgvqsm_threads}:mem=20gb:vmem=20gb',
        'overwrite': True
    }
    wf.connect([
        (inputnode, mn_qsm_filled, [('TE', 'TE')]),
        (inputnode, mn_qsm_filled, [('fieldStrength', 'b0')]),
        (inputnode, mn_qsm_filled, [('mask', 'mask_file')]),
        (inputnode, mn_qsm_filled, [('unwrapped_phase', 'phase_file')]),
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
            in_like=phase_file
        ),
        name='nibabel_numpy_nilearn_resample-qsm'
    )
    wf.connect([
        (n_qsm_filled_average, n_resample_qsm, [('out_file', 'in_file')]),
        (n_resample_qsm, outputnode, [('out_file', 'qsm_singlepass')]),
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
            'qsub_args': f'-A {run_args.pbs} -l walltime=03:00:00 -l select=1:ncpus={run_args.tgvqsm_threads}:mem=20gb:vmem=20gb',
            'overwrite': True
        }

        wf.connect([
            (inputnode, mn_qsm, [('TE', 'TE')]),
            (inputnode, mn_qsm, [('fieldStrength', 'b0')]),
            (inputnode, mn_qsm, [('mask', 'mask_file')]),
            (inputnode, mn_qsm, [('unwrapped_phase', 'phase_file')])
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
            # input : in_filesoutputnode
            # output: out_file
        )
        wf.connect([
            (mn_qsm_twopass, n_qsm_twopass_average, [('out_file', 'in_files')])
        ])

        # resample qsm to original
        n_resample_qsm_twopass = Node(
            interface=sampling.ResampleLikeInterface(
                in_like=phase_file
            ),
            name='nibabel_numpy_nilearn_resample-qsm-twopass'
        )
        wf.connect([
            (n_qsm_twopass_average, n_resample_qsm_twopass, [('out_file', 'in_file')]),
            (n_resample_qsm_twopass, outputnode, [('out_file', 'qsm_twopass')]),
        ])
 
    return wf

def add_tgvqsm_workflow(wf, run_args, mn_params, mn_inputs, mn_mask, n_datasink, phase_file, unwrapping_type):
    wf_unwrapping = unwrapping_workflow(unwrapping_type)
    wf_tgvqsm = tgvqsm_workflow(run_args, phase_file)

    wf.connect([
        (mn_inputs, wf_unwrapping, [('phase_files', 'inputnode.wrapped_phase')]),
        (mn_inputs, wf_unwrapping, [('magnitude_files', 'inputnode.mag')]),
        (mn_params, wf_unwrapping, [('EchoTime', 'inputnode.TE')]),
        
        (wf_unwrapping, wf_tgvqsm, [('outputnode.unwrapped_phase', 'inputnode.unwrapped_phase')]),
        (mn_mask, wf_tgvqsm, [('masks_filled', 'inputnode.mask')]),
        (mn_params, wf_tgvqsm, [('EchoTime', 'inputnode.TE'),
                                 ('MagneticFieldStrength', 'inputnode.fieldStrength')]),
        
        (wf_tgvqsm, n_datasink, [('outputnode.qsm_singlepass', 'qsm_singlepass'),
                                    ('outputnode.qsm_twopass', 'qsm_final')])
    ])

    return wf


def tgvqsm_B0_workflow(run_args, phase_file, name="tgvqsm_B0"):
    wf = Workflow(name)

    inputnode = Node(
        interface=IdentityInterface(
            fields=['unwrapped_phase', 'mask', 'fieldStrength', 'TE']),
        name='inputnode'
    )
# 'tgvqsm_iterations', 'tgvqsm_alphas', 'two_pass', 'tgvqsm_threads', 
    outputnode = Node(
        interface=IdentityInterface(
            fields=['qsm_singlepass', 'qsm_twopass']),
        iterfield=['qsm_singlepass', 'qsm_twopass'],
        name='outputnode'
    )

    # === Single-pass QSM reconstruction (filled) ===
    mn_qsm_filled = Node(
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
        'qsub_args': f'-A {run_args.pbs} -l walltime=03:00:00 -l select=1:ncpus={run_args.tgvqsm_threads}:mem=20gb:vmem=20gb',
        'overwrite': True
    }
    wf.connect([
        (inputnode, mn_qsm_filled, [('unwrapped_phase', 'phase_file')]),
        (inputnode, mn_qsm_filled, [('fieldStrength', 'b0')]),
        (inputnode, mn_qsm_filled, [('TE', 'TE')]),
        (inputnode, mn_qsm_filled, [('mask', 'mask_file')])
    ])


    # resample qsm to original
    n_resample_qsm = Node(
        interface=sampling.ResampleLikeInterface(
            in_like=phase_file
        ),
        name='nibabel_numpy_nilearn_resample-qsm'
    )
    wf.connect([
        (mn_qsm_filled, n_resample_qsm, [('out_file', 'in_file')]),
        (n_resample_qsm, outputnode, [('out_file', 'qsm_singlepass')]),
    ])

    # === Two-pass QSM reconstruction (not filled) ===
    if run_args.two_pass:
        mn_qsm = Node(
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
            'qsub_args': f'-A {run_args.pbs} -l walltime=03:00:00 -l select=1:ncpus={run_args.tgvqsm_threads}:mem=20gb:vmem=20gb',
            'overwrite': True
        }

        wf.connect([
            (inputnode, mn_qsm, [('TE', 'TE')]),
            (inputnode, mn_qsm, [('fieldStrength', 'b0')]),
            (inputnode, mn_qsm, [('mask', 'mask_file')]),
            (inputnode, mn_qsm, [('unwrapped_phase', 'phase_file')])
        ])

        # Two-pass combination step
        mn_qsm_twopass = Node(
            interface=twopass.TwopassNiftiInterface(),
            name='numpy_nibabel_twopass',
            iterfield=['in_file1', 'in_file2']
        )
        wf.connect([
            (mn_qsm, mn_qsm_twopass, [('out_file', 'in_file1')]),
            (mn_qsm_filled, mn_qsm_twopass, [('out_file', 'in_file2')]),
            #(mn_mask, mn_qsm_twopass, [('mask_file', 'in_maskFile')])
        ])

        # resample qsm to original
        n_resample_qsm_twopass = Node(
            interface=sampling.ResampleLikeInterface(
                in_like=phase_file
            ),
            name='nibabel_numpy_nilearn_resample-qsm-twopass'
        )
        wf.connect([
            (mn_qsm_twopass, n_resample_qsm_twopass, [('out_file', 'in_file')]),
            (n_resample_qsm_twopass, outputnode, [('out_file', 'qsm_twopass')]),
        ])
 
    return wf

def add_b0tgvqsm_workflow(wf, run_args, mn_params, mn_inputs, mn_mask, n_datasink, phase_file):
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
    n_echoTime = Node(Function(input_names="list", # TODO try to use B0 phase-based mask
                            output_names=["echoTime"],
                            function=first),
                            name='extract_TE')
    
    wf_unwrapping = unwrapping_workflow("romeoB0")
    wf_tgvqsmB0 = tgvqsm_B0_workflow(run_args, phase_file)

    wf.connect([
        (mn_inputs, wf_unwrapping, [('phase_files', 'inputnode.wrapped_phase')]),
        (mn_inputs, wf_unwrapping, [('magnitude_files', 'inputnode.mag')]),
        (mn_params, wf_unwrapping, [('EchoTime', 'inputnode.TE')]),
        
        (wf_unwrapping, wf_tgvqsmB0, [('outputnode.unwrapped_phase', 'inputnode.unwrapped_phase')]),

        (mn_params, n_echoTime, [('EchoTime', 'list')]),
        (n_echoTime, wf_tgvqsmB0, [('echoTime', 'inputnode.TE'),]),
        (mn_mask, n_mask, [('masks_filled', 'list'),]),
        (n_mask, wf_tgvqsmB0, [('out_file', 'inputnode.mask'),]),
        (mn_params, n_fieldStrength, [('MagneticFieldStrength', 'list')]),
        (n_fieldStrength, wf_tgvqsmB0, [('fieldStrength', 'inputnode.fieldStrength'),]),
        
        (wf_tgvqsmB0, n_datasink, [('outputnode.qsm_singlepass', 'qsm_singlepass'),
                                    ('outputnode.qsm_twopass', 'qsm_final')])
    ])

    return wf