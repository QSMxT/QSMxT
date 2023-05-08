import numpy as np

from nipype.pipeline.engine import Workflow, Node, MapNode
from nipype.interfaces.utility import IdentityInterface

import interfaces.nipype_interface_romeo as romeo

from interfaces import nipype_interface_tgv_qsm as tgv
from interfaces import nipype_interface_qsmjl as qsmjl
from interfaces import nipype_interface_nextqsm as nextqsm
from interfaces import nipype_interface_laplacian_unwrapping as laplacian
from interfaces import nipype_interface_processphase as processphase

from scripts.qsmxt_functions import gen_plugin_args

import psutil

def qsm_workflow(run_args, name, magnitude_available, qsm_erosions=0):
    wf = Workflow(name=f"{name}_workflow")

    n_inputs = Node(
        interface=IdentityInterface(
            fields=['phase', 'phase_unwrapped', 'frequency', 'magnitude', 'mask', 'TE', 'B0', 'b0_direction', 'vsz']
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
            laplacian_threads = min(2, run_args.n_procs) if run_args.multiproc else 2
            mn_laplacian = MapNode(
                interface=laplacian.LaplacianInterface(),
                iterfield=['phase'],
                name='mrt_laplacian-unwrapping',
                mem_gb=min(3, run_args.mem_avail),
                n_procs=laplacian_threads
            )
            #mn_laplacian = MapNode(
            #    interface=qsmjl.LaplacianUnwrappingInterface(num_threads=laplacian_threads),
            #    iterfield=['phase', 'mask'],
            #    name='qsmjl_laplacian-unwrapping',
            #    mem_gb=min(3, run_args.mem_avail),
            #    n_procs=laplacian_threads
            #)
            wf.connect([
                (n_inputs, mn_laplacian, [('phase', 'phase')]),
                #(n_inputs, mn_laplacian, [('mask', 'mask')]),
                (mn_laplacian, n_unwrapping, [('phase_unwrapped', 'phase_unwrapped')])
            ])
            mn_laplacian.plugin_args = gen_plugin_args(
                plugin_args={ 'overwrite': True },
                slurm_account=run_args.slurm[0],
                pbs_account=run_args.pbs,
                slurm_partition=run_args.slurm[1],
                name="Laplacian",
                mem_gb=3,
                num_cpus=laplacian_threads
            )
        if run_args.unwrapping_algorithm == 'romeo':
            if run_args.combine_phase:
                wf.connect([
                    (n_inputs, n_unwrapping, [('phase_unwrapped', 'phase_unwrapped')]),
                ])
            else:
                mn_romeo = Node(
                    interface=romeo.RomeoB0Interface(),
                    #iterfield=['phase'] + (['magnitude'] if magnitude_available else []),
                    name='mrt_romeo',
                    mem_gb=min(3, run_args.mem_avail)
                )
                wf.connect([
                    (n_inputs, mn_romeo, [('phase', 'phase')]),
                    (n_inputs, mn_romeo, [('TE', 'TE')]),
                    (mn_romeo, n_unwrapping, [('phase_unwrapped', 'phase_unwrapped')])
                ])
                if magnitude_available:
                    wf.connect([
                        (n_inputs, mn_romeo, [('magnitude', 'magnitude')]),
                    ])


    # === PHASE TO FREQUENCY ===
    n_phase_normalized = Node(
        interface=IdentityInterface(
            fields=['phase_normalized']
        ),
        name='phase_normalized'
    )
    if run_args.qsm_algorithm in ['rts', 'tv', 'nextqsm'] and not run_args.combine_phase:
        normalize_phase_threads = min(2, run_args.n_procs) if run_args.multiproc else 2
        mn_normalize_phase = MapNode(
            interface=processphase.PhaseToNormalizedInterface(
                scale_factor=1e6 if run_args.qsm_algorithm == 'nextqsm' else 1e6/(2*np.pi)
            ),
            name='nibabel-numpy_normalize-phase',
            iterfield=['phase', 'TE'],
            mem_gb=min(3, run_args.mem_avail),
            n_procs=normalize_phase_threads
        )
        wf.connect([
            (n_unwrapping, mn_normalize_phase, [('phase_unwrapped', 'phase')]),
            (n_inputs, mn_normalize_phase, [('TE', 'TE')]),
            (n_inputs, mn_normalize_phase, [('B0', 'B0')]),
            (mn_normalize_phase, n_phase_normalized, [('phase_normalized', 'phase_normalized')])
        ])
        mn_normalize_phase.plugin_args = gen_plugin_args(
            plugin_args={ 'overwrite': True },
            slurm_account=run_args.slurm[0],
            pbs_account=run_args.pbs,
            slurm_partition=run_args.slurm[1],
            name="PhaToNormalized",
            mem_gb=3,
            num_cpus=normalize_phase_threads
        )
    if run_args.qsm_algorithm in ['rts', 'tv', 'nextqsm'] and run_args.combine_phase:
        normalize_freq_threads = min(2, run_args.n_procs) if run_args.multiproc else 2
        mn_normalize_freq = MapNode(
            interface=processphase.FreqToNormalizedInterface(
                scale_factor=1e6 if run_args.qsm_algorithm == 'nextqsm' else 1e6/(2*np.pi)
            ),
            name='nibabel-numpy_normalize-freq',
            iterfield=['frequency'],
            mem_gb=min(3, run_args.mem_avail),
            n_procs=normalize_freq_threads
        )
        wf.connect([
            (n_inputs, mn_normalize_freq, [('frequency', 'frequency')]),
            (n_inputs, mn_normalize_freq, [('B0', 'B0')]),
            (mn_normalize_freq, n_phase_normalized, [('phase_normalized', 'phase_normalized')])
        ])
        mn_normalize_freq.plugin_args = gen_plugin_args(
            plugin_args={ 'overwrite': True },
            slurm_account=run_args.slurm[0],
            pbs_account=run_args.pbs,
            slurm_partition=run_args.slurm[1],
            name="PhaToNormalized",
            mem_gb=3,
            num_cpus=normalize_freq_threads
        )
    if run_args.qsm_algorithm == 'tgv' and run_args.combine_phase:
        freq_to_phase_threads = min(2, run_args.n_procs) if run_args.multiproc else 2
        mn_freq_to_phase = MapNode(
            interface=processphase.FreqToPhaseInterface(TE=0.005, wraps=True),
            name='nibabel-numpy_freq-to-phase',
            iterfield=['frequency'],
            mem_gb=min(3, run_args.mem_avail),
            n_procs=freq_to_phase_threads
        )
        wf.connect([
            (n_inputs, mn_freq_to_phase, [('frequency', 'frequency')]),
            (mn_freq_to_phase, n_phase_normalized, [('phase', 'phase_normalized')])
        ])
        mn_freq_to_phase.plugin_args = gen_plugin_args(
            plugin_args={ 'overwrite': True },
            slurm_account=run_args.slurm[0],
            pbs_account=run_args.pbs,
            slurm_partition=run_args.slurm[1],
            name="FreqToPhase",
            mem_gb=3,
            num_cpus=freq_to_phase_threads
        )
        
    # === BACKGROUND FIELD REMOVAL ===
    if run_args.qsm_algorithm in ['rts', 'tv']:
        mn_bf = MapNode(
            interface=IdentityInterface(
                fields=['tissue_frequency', 'mask']
            ),
            iterfield=['tissue_frequency', 'mask'],
            name='bf-removal'
        )
        if run_args.bf_algorithm == 'vsharp':
            vsharp_threads = min(2, run_args.n_procs) if run_args.multiproc else 2
            mn_vsharp = MapNode(
                interface=qsmjl.VsharpInterface(num_threads=vsharp_threads),
                iterfield=['frequency', 'mask'],
                name='qsmjl_vsharp',
                n_procs=vsharp_threads,
                mem_gb=min(3, run_args.mem_avail)
            )
            wf.connect([
                (n_phase_normalized, mn_vsharp, [('phase_normalized', 'frequency')]),
                (n_inputs, mn_vsharp, [('mask', 'mask')]),
                (n_inputs, mn_vsharp, [('vsz', 'vsz')]),
                (mn_vsharp, mn_bf, [('tissue_frequency', 'tissue_frequency')]),
                (mn_vsharp, mn_bf, [('vsharp_mask', 'mask')]),
            ])
            mn_vsharp.plugin_args = gen_plugin_args(
                plugin_args={ 'overwrite': True },
                slurm_account=run_args.slurm[0],
                pbs_account=run_args.pbs,
                slurm_partition=run_args.slurm[1],
                name="VSHARP",
                mem_gb=3,
                num_cpus=vsharp_threads
            )
        if run_args.bf_algorithm == 'pdf':
            pdf_threads = min(8, run_args.n_procs) if run_args.multiproc else 8
            mn_pdf = MapNode(
                interface=qsmjl.PdfInterface(num_threads=pdf_threads),
                iterfield=['frequency', 'mask'],
                name='qsmjl_pdf',
                n_procs=pdf_threads,
                mem_gb=min(5, run_args.mem_avail),
            )
            wf.connect([
                (n_phase_normalized, mn_pdf, [('phase_normalized', 'frequency')]),
                (n_inputs, mn_pdf, [('mask', 'mask')]),
                (n_inputs, mn_pdf, [('vsz', 'vsz')]),
                (mn_pdf, mn_bf, [('tissue_frequency', 'tissue_frequency')]),
                (n_inputs, mn_bf, [('mask', 'mask')]),
            ])
            mn_pdf.plugin_args = gen_plugin_args(
                plugin_args={ 'overwrite': True },
                slurm_account=run_args.slurm[0],
                pbs_account=run_args.pbs,
                slurm_partition=run_args.slurm[1],
                name="PDF",
                time="01:00:00",
                mem_gb=5,
                num_cpus=pdf_threads
            )

    # === DIPOLE INVERSION ===
    if run_args.qsm_algorithm == 'nextqsm':
        nextqsm_threads = min(8, run_args.n_procs) if run_args.multiproc else 8
        mn_qsm = MapNode(
            interface=nextqsm.NextqsmInterface(),
            name='nextqsm',
            iterfield=['phase', 'mask'],
            mem_gb=min(13, run_args.mem_avail),
            n_procs=nextqsm_threads
        )
        wf.connect([
            (n_phase_normalized, mn_qsm, [('phase_normalized', 'phase')]),
            (n_inputs, mn_qsm, [('mask', 'mask')]),
            (mn_qsm, n_outputs, [('qsm', 'qsm')]),
        ])
        mn_qsm.plugin_args = gen_plugin_args(
            plugin_args={ 'overwrite': True },
            slurm_account=run_args.slurm[0],
            pbs_account=run_args.pbs,
            slurm_partition=run_args.slurm[1],
            name="RTS",
            mem_gb=13,
            num_cpus=nextqsm_threads
        )
    if run_args.qsm_algorithm == 'rts':
        rts_threads = min(2, run_args.n_procs) if run_args.multiproc else 2
        mn_qsm = MapNode(
            interface=qsmjl.RtsQsmInterface(num_threads=rts_threads),
            name='qsmjl_rts',
            iterfield=['tissue_frequency', 'mask'],
            n_procs=rts_threads,
            mem_gb=min(5, run_args.mem_avail),
            terminal_output="file_split"
        )
        wf.connect([
            (mn_bf, mn_qsm, [('tissue_frequency', 'tissue_frequency')]),
            (mn_bf, mn_qsm, [('mask', 'mask')]),
            (n_inputs, mn_qsm, [('vsz', 'vsz')]),
            (n_inputs, mn_qsm, [('b0_direction', 'b0_direction')]),
            (mn_qsm, n_outputs, [('qsm', 'qsm')]),
        ])
        mn_qsm.plugin_args = gen_plugin_args(
            plugin_args={ 'overwrite': True },
            slurm_account=run_args.slurm[0],
            pbs_account=run_args.pbs,
            slurm_partition=run_args.slurm[1],
            name="RTS",
            mem_gb=5,
            num_cpus=rts_threads
        )
    if run_args.qsm_algorithm == 'tv':
        tv_threads = min(2, run_args.n_procs) if run_args.multiproc else 2
        mn_qsm = MapNode(
            interface=qsmjl.TvQsmInterface(num_threads=tv_threads),
            name='qsmjl_tv',
            iterfield=['tissue_frequency', 'mask'],
            n_procs=tv_threads,
            mem_gb=min(5, run_args.mem_avail),
            terminal_output="file_split"
        )
        wf.connect([
            (mn_bf, mn_qsm, [('tissue_frequency', 'tissue_frequency')]),
            (mn_bf, mn_qsm, [('mask', 'mask')]),
            (n_inputs, mn_qsm, [('vsz', 'vsz')]),
            (n_inputs, mn_qsm, [('b0_direction', 'b0_direction')]),
            (mn_qsm, n_outputs, [('qsm', 'qsm')]),
        ])
        mn_qsm.plugin_args = gen_plugin_args(
            plugin_args={ 'overwrite': True },
            slurm_account=run_args.slurm[0],
            pbs_account=run_args.pbs,
            slurm_partition=run_args.slurm[1],
            name="TV",
            time="01:00:00",
            mem_gb=5,
            num_cpus=tv_threads
        )
    if run_args.qsm_algorithm == 'tgv':
        tgv_threads = min(20, run_args.n_procs)
        mn_qsm = MapNode(
            interface=tgv.QSMappingInterface(
                iterations=run_args.tgv_iterations,
                alpha=run_args.tgv_alphas,
                erosions=qsm_erosions,
                num_threads=tgv_threads,
                out_suffix='_tgv',
                extra_arguments='--ignore-orientation --no-resampling'
            ),
            iterfield=['phase', 'TE', 'mask'],
            name='tgv',
            mem_gb=min(6, run_args.mem_avail),
            n_procs=tgv_threads
        )
        mn_qsm.plugin_args = gen_plugin_args(
            plugin_args={ 'overwrite': True },
            slurm_account=run_args.slurm[0],
            pbs_account=run_args.pbs,
            slurm_partition=run_args.slurm[1],
            name="TGV",
            time="01:00:00",
            mem_gb=6,
            num_cpus=tgv_threads
        )
        wf.connect([
            (n_inputs, mn_qsm, [('mask', 'mask')]),
            (n_inputs, mn_qsm, [('B0', 'B0')]),
            (mn_qsm, n_outputs, [('qsm', 'qsm')]),
        ])
        if run_args.combine_phase:
            mn_qsm.inputs.TE = [0.005]
            wf.connect([
                (n_phase_normalized, mn_qsm, [('phase_normalized', 'phase')])
            ])
        else:
            wf.connect([
                (n_inputs, mn_qsm, [('phase', 'phase')]),
                (n_inputs, mn_qsm, [('TE', 'TE')])
            ])

    
    return wf

