from nipype.pipeline.engine import Workflow, Node, MapNode
from nipype.interfaces.utility import IdentityInterface, Function

import interfaces.nipype_interface_laplacian_unwrapping as laplacian
import interfaces.nipype_interface_romeo as romeo

from interfaces import nipype_interface_tgv_qsm as tgv
from interfaces import nipype_interface_qsmjl as qsmjl
from interfaces import nipype_interface_nextqsm as nextqsm

import psutil

def qsm_workflow(run_args, name):
    wf = Workflow(name=f"{name}_workflow")

    n_inputs = Node(
        interface=IdentityInterface(
            fields=['phase', 'phase_unwrapped', 'frequency', 'magnitude', 'mask', 'TE', 'b0_strength', 'b0_direction', 'vsz']
        ),
        name='qsm_inputs'
    )

    n_outputs = Node(
        interface=IdentityInterface(
            fields=['qsm']
        ),
        name='qsm_outputs'
    )

    # === PHASE UNWRAPPING ===
    if run_args.unwrapping_algorithm:
        n_unwrapping = Node(
            interface=IdentityInterface(
                fields=['phase_unwrapped']
            ),
            name='phase-unwrapping'
        )
        if run_args.unwrapping_algorithm == 'laplacian':
            mn_laplacian = MapNode(
                interface=qsmjl.LaplacianUnwrappingInterface(),
                iterfield=['phase', 'mask'],
                name='qsmjl_laplacian-unwrapping',
                n_procs=min(run_args.process_threads, 2)
            )
            wf.connect([
                (n_inputs, mn_laplacian, [('phase', 'phase')]),
                (n_inputs, mn_laplacian, [('mask', 'mask')]),
                (mn_laplacian, n_unwrapping, [('phase_unwrapped', 'phase_unwrapped')])
            ])
        if run_args.unwrapping_algorithm == 'romeo':
            if run_args.combine_phase:
                wf.connect([
                    (n_inputs, n_unwrapping, [('phase_unwrapped', 'phase_unwrapped')]),
                ])
            else:
                mn_romeo = MapNode(
                    interface=romeo.RomeoInterface(),
                    iterfield=['phase', 'magnitude'],
                    name='mrt_romeo',
                    mem_gb=3
                )
                wf.connect([
                    (n_inputs, mn_romeo, [('phase', 'phase'), ('magnitude', 'magnitude')]),
                    (mn_romeo, n_unwrapping, [('phase_unwrapped', 'phase_unwrapped')])
                ])

    # === PHASE TO FREQUENCY ===
    n_frequency = Node(
        interface=IdentityInterface(
            fields=['frequency']
        ),
        name='frequency-inputs'
    )
    if run_args.qsm_algorithm in ['nextqsm', 'rts']:
        if not run_args.combine_phase:
            mn_phase_to_freq = MapNode(
                interface=qsmjl.PhaseToFreqInterface(), 
                name='qsmjl_phase-to-freq',
                iterfield=['phase', 'TE']
            )
            wf.connect([
                (n_unwrapping, mn_phase_to_freq, [('phase_unwrapped', 'phase')]),
                (n_inputs, mn_phase_to_freq, [('TE', 'TE')]),
                (n_inputs, mn_phase_to_freq, [('vsz', 'vsz')]),
                (n_inputs, mn_phase_to_freq, [('b0_strength', 'b0_strength')]),
                (mn_phase_to_freq, n_frequency, [('frequency', 'frequency')])
            ])
        else:
            wf.connect([
                (n_inputs, n_frequency, [('frequency', 'frequency')])
            ])
        
    # === BACKGROUND FIELD REMOVAL ===
    if run_args.qsm_algorithm in ['rts']:
        mn_bf = MapNode(
            interface=IdentityInterface(
                fields=['tissue_frequency', 'mask']
            ),
            iterfield=['tissue_frequency', 'mask'],
            name='bf-removal'
        )
        if True:
            mn_vsharp = MapNode(
                interface=qsmjl.VsharpInterface(),
                iterfield=['frequency', 'mask'],
                name='qsmjl_vsharp',
                n_procs=min(run_args.process_threads, 2),
                mem_gb=3
            )
            wf.connect([
                (n_frequency, mn_vsharp, [('frequency', 'frequency')]),
                (n_inputs, mn_vsharp, [('mask', 'mask')]),
                (n_inputs, mn_vsharp, [('vsz', 'vsz')]),
                (mn_vsharp, mn_bf, [('tissue_frequency', 'tissue_frequency')]),
                (mn_vsharp, mn_bf, [('vsharp_mask', 'mask')]),
            ])
        if False:
            mn_pdf = MapNode(
                interface=qsmjl.PdfInterface(),
                iterfield=['frequency', 'mask'],
                name='qsmjl_pdf',
                n_procs=min(run_args.process_threads, 2),
                mem_gb=3
            )
            wf.connect([
                (n_frequency, mn_pdf, [('frequency', 'frequency')]),
                (n_inputs, mn_pdf, [('mask', 'mask')]),
                (n_inputs, mn_pdf, [('vsz', 'vsz')]),
                (mn_pdf, mn_bf, [('tissue_frequency', 'tissue_frequency')]),
                (n_inputs, mn_bf, [('mask', 'mask')]),
            ])

    # === DIPOLE INVERSION ===
    if run_args.qsm_algorithm == 'nextqsm':
        mn_qsm = MapNode(
            interface=nextqsm.NextqsmInterface(),
            name='nextqsm',
            iterfield=['phase', 'mask'],
            mem_gb=min(13, psutil.virtual_memory().available/10e8 * 0.9)
        )
        wf.connect([
            (n_frequency, mn_qsm, [('frequency', 'phase')]),
            (n_inputs, mn_qsm, [('mask', 'mask')]),
            (mn_qsm, n_outputs, [('qsm', 'qsm')]),
        ])
    if run_args.qsm_algorithm == 'rts':
        mn_qsm = MapNode(
            interface=qsmjl.RtsQsmInterface(),
            name='qsmjl_rts',
            iterfield=['tissue_frequency', 'mask'],
            n_procs=min(run_args.process_threads, 2),
            mem_gb=5
        )
        wf.connect([
            (mn_bf, mn_qsm, [('tissue_frequency', 'tissue_frequency')]),
            (mn_bf, mn_qsm, [('mask', 'mask')]),
            (n_inputs, mn_qsm, [('vsz', 'vsz')]),
            (n_inputs, mn_qsm, [('b0_direction', 'b0_direction')]),
            (mn_qsm, n_outputs, [('qsm', 'qsm')]),
        ])
    if run_args.qsm_algorithm == 'tgv':
        mn_qsm = MapNode(
            interface=tgv.QSMappingInterface(
                iterations=run_args.tgv_iterations,
                alpha=run_args.tgv_alphas,
                erosions=run_args.tgv_erosions,
                num_threads=run_args.process_threads,
                out_suffix='_tgv',
                extra_arguments='--ignore-orientation --no-resampling'
            ),
            iterfield=['phase_file', 'TE', 'mask_file'],
            name='tgv',
            mem_gb=6
        )
        mn_qsm.plugin_args = {
            'qsub_args': f'-A {run_args.qsub_account_string} -l walltime=03:00:00 -l select=1:ncpus={run_args.process_threads}:mem=20gb:vmem=20gb',
            'overwrite': True
        }
        wf.connect([
            (n_inputs, mn_qsm, [('mask', 'mask_file')]),
            (n_inputs, mn_qsm, [('TE', 'TE')]),
            (n_inputs, mn_qsm, [('b0_strength', 'b0_strength')]),
            (mn_qsm, n_outputs, [('qsm', 'qsm')]),
        ])
        if run_args.unwrapping_algorithm:
            wf.connect([
                (n_unwrapping, mn_qsm, [('phase_unwrapped', 'phase_file')])
            ])
        else:
            wf.connect([
                (n_inputs, mn_qsm, [('phase', 'phase_file')])
            ])

    
    return wf

