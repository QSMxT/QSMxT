from nipype.pipeline.engine import Workflow, Node, MapNode
from nipype.interfaces.utility import IdentityInterface

import interfaces.nipype_interface_laplacian_unwrapping as laplacian
import interfaces.nipype_interface_romeo as romeo

from interfaces import nipype_interface_tgv_qsm as tgv
from interfaces import nipype_interface_qsmjl as qsmjl
from interfaces import nipype_interface_nextqsm as nextqsm

import psutil

def qsm_workflow(run_args, mn_inputs, name):
    wf = Workflow(name=f"{name}_workflow")

    mn_outputs = MapNode(
        interface=IdentityInterface(
            fields=['unwrapped_phase', 'tissue_frequency', 'qsm', 'mask']
        ),
        iterfield=['unwrapped_phase', 'tissue_frequency', 'qsm', 'mask'],
        name='qsm_outputs'
    )

    # === PHASE UNWRAPPING ===
    if run_args.unwrapping_algorithm:
        mn_unwrapping = MapNode(
            interface=IdentityInterface(
                fields=['unwrapped_phase']
            ),
            iterfield=['unwrapped_phase'],
            name='phase-unwrapping'
        )
        if run_args.unwrapping_algorithm == 'laplacian':
            mn_laplacian = MapNode(
                interface=qsmjl.LaplacianUnwrappingInterface(),
                iterfield=['in_phase', 'in_mask'],
                name='qsmjl_laplacian-unwrapping',
                n_procs=min(run_args.process_threads, 2)
            )
            wf.connect([
                (mn_inputs, mn_laplacian, [('phase', 'in_phase')]),
                (mn_inputs, mn_laplacian, [('mask', 'in_mask')]),
                (mn_laplacian, mn_outputs, [('out_unwrapped', 'unwrapped_phase')]),
                (mn_laplacian, mn_unwrapping, [('out_unwrapped', 'unwrapped_phase')])
            ])
        if run_args.unwrapping_algorithm == 'romeo':
            mn_romeo = MapNode(
                interface=romeo.RomeoInterface(),
                iterfield=['phase', 'mag'],
                name='mrt_romeo',
            )
            wf.connect([
                (mn_inputs, mn_romeo, [('phase', 'phase'), ('magnitude', 'mag')]),
                (mn_romeo, mn_outputs, [('out_file', 'unwrapped_phase')]),
                (mn_romeo, mn_unwrapping, [('out_file', 'unwrapped_phase')])
            ])

    # === PHASE TO FREQUENCY ===
    if run_args.qsm_algorithm in ['nextqsm', 'rts']: 
        mn_phase_to_freq = MapNode(
            interface=qsmjl.PhaseToFreqInterface(), 
            name='qsmjl_phase-to-freq',
            iterfield=['in_phase', 'in_TEs']
            # in_phase, in_mask, in_TEs, in_vsz, in_b0str, out_frequency
        )
        wf.connect([
            (mn_unwrapping, mn_phase_to_freq, [('unwrapped_phase', 'in_phase')]),
            (mn_inputs, mn_phase_to_freq, [('TE', 'in_TEs')]),
            (mn_inputs, mn_phase_to_freq, [('vsz', 'in_vsz')]),
            (mn_inputs, mn_phase_to_freq, [('B0_str', 'in_b0str')]),
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
        if run_args.bf_algorithm == 'vsharp':
            mn_vsharp = MapNode(
                interface=qsmjl.VsharpInterface(),
                iterfield=['in_frequency', 'in_mask'],
                name='qsmjl_vsharp',
                n_procs=min(run_args.process_threads, 2),
                mem_gb=3
                # in_frequency, in_mask, in_vsz, out_freq, out_mask
            )
            wf.connect([
                (mn_phase_to_freq, mn_vsharp, [('out_frequency', 'in_frequency')]),
                (mn_inputs, mn_vsharp, [('mask', 'in_mask')]),
                (mn_inputs, mn_vsharp, [('vsz', 'in_vsz')]),
                (mn_vsharp, mn_bf, [('out_freq', 'tissue_frequency')]),
                (mn_vsharp, mn_bf, [('out_mask', 'mask')]),
                (mn_vsharp, mn_outputs, [('out_freq', 'tissue_frequency')]),
                (mn_vsharp, mn_outputs, [('out_mask', 'mask')])
            ])
        if run_args.bf_algorithm == 'pdf':
            mn_pdf = MapNode(
                interface=qsmjl.PdfInterface(),
                iterfield=['in_frequency', 'in_mask'],
                name='qsmjl_pdf',
                n_procs=min(run_args.process_threads, 2),
                mem_gb=3
            )
            wf.connect([
                (mn_phase_to_freq, mn_pdf, [('out_frequency', 'in_frequency')]),
                (mn_inputs, mn_pdf, [('mask', 'in_mask')]),
                (mn_inputs, mn_pdf, [('vsz', 'in_vsz')]),
                (mn_pdf, mn_bf, [('out_freq', 'tissue_frequency')]),
                (mn_inputs, mn_bf, [('mask', 'mask')]),
                (mn_pdf, mn_outputs, [('out_freq', 'tissue_frequency')]),
                (mn_inputs, mn_outputs, [('mask', 'mask')])
            ])
    else:
        wf.connect([
            (mn_inputs, mn_outputs, [('mask', 'mask')])
        ])

    # === DIPOLE INVERSION ===
    if run_args.qsm_algorithm == 'nextqsm':
        n_qsm = MapNode(
            interface=nextqsm.NextqsmInterface(),
            name='nextqsm',
            iterfield=['phase', 'mask'],
            mem_gb=min(13, psutil.virtual_memory().available/10e8 * 0.9)
            # phase, mask, out_file
        )
        wf.connect([
            (mn_phase_to_freq, n_qsm, [('out_frequency', 'phase')]),
            (mn_inputs, n_qsm, [('mask', 'mask')]),
            (n_qsm, mn_outputs, [('out_file', 'qsm')]),
        ])
    if run_args.qsm_algorithm == 'rts':
        n_qsm = MapNode(
            interface=qsmjl.RtsQsmInterface(),
            name='qsmjl_rts',
            iterfield=['in_frequency', 'in_mask'],
            n_procs=min(run_args.process_threads, 2)
            # in_frequency, in_mask, in_vsz, in_b0dir, out_qsm
        )
        wf.connect([
            (mn_bf, n_qsm, [('tissue_frequency', 'in_frequency')]),
            (mn_bf, n_qsm, [('mask', 'in_mask')]),
            (mn_inputs, n_qsm, [('vsz', 'in_vsz')]),
            (mn_inputs, n_qsm, [('B0_dir', 'in_b0dir')]),
            (n_qsm, mn_outputs, [('out_qsm', 'qsm')]),
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
            # inputs: 'phase_file', 'TE', 'b0', 'mask_file'
            # output: 'out_file'
        )
        mn_qsm.plugin_args = {
            'qsub_args': f'-A {run_args.qsub_account_string} -l walltime=03:00:00 -l select=1:ncpus={run_args.process_threads}:mem=20gb:vmem=20gb',
            'overwrite': True
        }
        wf.connect([
            (mn_inputs, mn_qsm, [('mask', 'mask_file')]),
            (mn_inputs, mn_qsm, [('TE', 'TE')]),
            (mn_inputs, mn_qsm, [('B0_str', 'b0')]),
            (mn_qsm, mn_outputs, [('out_file', 'qsm')]),
        ])
        if run_args.unwrapping_algorithm:
            wf.connect([
                (mn_unwrapping, mn_qsm, [('unwrapped_phase', 'phase_file')])
            ])
        else:
            wf.connect([
                (mn_inputs, mn_qsm, [('phase', 'phase_file')])
            ])

    
    return wf

