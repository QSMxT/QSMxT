import glob
import os

from nipype.pipeline.engine import Workflow, Node, MapNode
from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import DataSink

from interfaces import nipype_interface_romeo as romeo
from interfaces import nipype_interface_tgv_qsm as tgv
from interfaces import nipype_interface_qsmjl as qsmjl
from interfaces import nipype_interface_nextqsm as nextqsm
from interfaces import nipype_interface_laplacian_unwrapping as laplacian
from interfaces import nipype_interface_processphase as processphase
from interfaces import nipype_interface_axialsampling as sampling
from interfaces import nipype_interface_t2star_r2star as t2s_r2s
from interfaces import nipype_interface_clearswi as swi
from interfaces import nipype_interface_nonzeroaverage as nonzeroaverage
from interfaces import nipype_interface_twopass as twopass

from scripts.logger import LogLevel, get_logger
from scripts.qsmxt_functions import gen_plugin_args, create_node
from workflows.masking import masking_workflow

import numpy as np

def init_qsm_workflow(run_args, subject, session, run):
    logger = get_logger('main')
    logger.log(LogLevel.INFO.value, f"Creating QSM workflow for {subject}/{session}/{run}...")

    # get relevant files from this run
    phase_pattern = os.path.join(run_args.bids_dir, run_args.phase_pattern.format(subject=subject, session=session, run=run))
    phase_files = sorted(glob.glob(phase_pattern))[:run_args.num_echoes]
    
    magnitude_pattern = os.path.join(run_args.bids_dir, run_args.magnitude_pattern.format(subject=subject, session=session, run=run))
    magnitude_files = sorted(glob.glob(magnitude_pattern))[:run_args.num_echoes]

    params_pattern = os.path.join(run_args.bids_dir, run_args.phase_pattern.format(subject=subject, session=session, run=run).replace("nii.gz", "nii").replace("nii", "json"))
    params_files = sorted(glob.glob(params_pattern))[:run_args.num_echoes]
    
    mask_pattern = os.path.join(run_args.bids_dir, run_args.mask_pattern.format(subject=subject, session=session, run=run))
    mask_files = sorted(glob.glob(mask_pattern))[:run_args.num_echoes] if run_args.use_existing_masks else []
    
    # handle any errors related to files and adjust any settings if needed
    if not phase_files:
        logger.log(LogLevel.WARNING.value, f"Skipping run {subject}/{session}/{run} - no phase files found matching pattern {phase_pattern}.")
        return
    if len(phase_files) != len(params_files):
        logger.log(LogLevel.WARNING.value, f"Skipping run {subject}/{session}/{run} - an unequal number of JSON and phase files are present.")
        return
    if len(phase_files) == 1 and run_args.combine_phase:
        run_args.combine_phase = False
    if run_args.use_existing_masks:
        if not mask_files:
            logger.log(LogLevel.WARNING.value, f"Run {subject}/{session}/{run}: --use_existing_masks specified but no masks found matching pattern: {mask_pattern}. Reverting to {run_args.masking_algorithm} masking.")
        if len(mask_files) > 1 and len(mask_files) != len(phase_files):
            logger.log(LogLevel.WARNING.value, f"Run {subject}/{session}/{run}: --use_existing_masks specified but unequal number of mask and phase files present. Reverting to {run_args.masking_algorithm} masking.")
            mask_files = []
        if len(mask_files) > 1 and run_args.combine_phase:
            logger.log(LogLevel.WARNING.value, f"Run {subject}/{session}/{run}: --combine_phase specified but multiple masks found with --use_existing_masks. The first mask will be used only.")
            mask_files = [mask_files[0] for x in mask_files]
        if mask_files:
            run_args.inhomogeneity_correction = False
            run_args.two_pass = False
            run_args.single_pass = True
            run_args.add_bet = False
    if not magnitude_files:
        if run_args.r2starmap:
            logger.log(LogLevel.WARNING.value, f"Cannot compute R2* for {subject}/{session}/{run} - no magnitude files found matching pattern: {magnitude_pattern}.")
            run_args.r2starmap = False
        if run_args.t2starmap:
            logger.log(LogLevel.WARNING.value, f"Cannot compute T2* for {subject}/{session}/{run} - no magnitude files found matching pattern: {magnitude_pattern}.")
            run_args.t2starmap = False
        if run_args.swi:
            logger.log(LogLevel.WARNING.value, f"Cannot compute SWI for {subject}/{session}/{run} - no magnitude files found matching pattern: {magnitude_pattern}.")
            run_args.swi = False
        if run_args.masking_input == 'magnitude':
            logger.log(LogLevel.WARNING.value, f"Run {subject}/{session}/{run} will use phase-based masking - no magnitude files found matching pattern: {magnitude_pattern}.")
            run_args.masking_input = 'phase'
            run_args.masking_algorithm = 'threshold'
            run_args.inhomogeneity_correction = False
            run_args.add_bet = False
        if run_args.add_bet:
            logger.log(LogLevel.WARNING.value, f"Run {subject}/{session}/{run} cannot use --add_bet option - no magnitude files found matching pattern: {magnitude_pattern}.")
            run_args.add_bet = False
        if run_args.combine_phase:
            logger.log(LogLevel.WARNING.value, f"Run {subject}/{session}/{run} cannot use --combine_phase option - no magnitude files found matching pattern: {magnitude_pattern}.")
            run_args.combine_phase = False
    
    # create nipype workflow for this run
    wf = Workflow(f"qsm_{run}", base_dir=os.path.join(run_args.output_dir, "workflow", "workflow_qsm", subject, session, run))

    # datasink
    n_outputs = Node(
        interface=DataSink(base_directory=run_args.output_dir),
        name='nipype_datasink'
    )

    # get files
    n_inputs = Node(
        IdentityInterface(
            fields=['phase', 'magnitude', 'params_files', 'mask']
        ),
        name='nipype_getfiles'
    )
    n_inputs.inputs.phase = phase_files
    n_inputs.inputs.magnitude = magnitude_files
    n_inputs.inputs.params_files = params_files
    if len(mask_files) == 1: mask_files = [mask_files[0] for _ in phase_files]
    n_inputs.inputs.mask = mask_files

    # read echotime and field strengths from json files
    def read_json_me(params_file):
        import json
        json_file = open(params_file, 'rt')
        data = json.load(json_file)
        te = data['EchoTime']
        json_file.close()
        return te
    def read_json_se(params_files):
        import json
        json_file = open(params_files[0], 'rt')
        data = json.load(json_file)
        B0 = data['MagneticFieldStrength']
        json_file.close()
        return B0
    mn_json_params = MapNode(
        interface=Function(
            input_names=['params_file'],
            output_names=['TE'],
            function=read_json_me
        ),
        iterfield=['params_file'],
        name='func_read-json-me'
    )
    wf.connect([
        (n_inputs, mn_json_params, [('params_files', 'params_file')])
    ])
    n_json_params = Node(
        interface=Function(
            input_names=['params_files'],
            output_names=['B0'],
            function=read_json_se
        ),
        iterfield=['params_file'],
        name='func_read-json-se'
    )
    wf.connect([
        (n_inputs, n_json_params, [('params_files', 'params_files')])
    ])

    # read voxel size 'vsz' from nifti file
    def read_nii(nii_file):
        import nibabel as nib
        if isinstance(nii_file, list): nii_file = nii_file[0]
        nii = nib.load(nii_file)
        return str(nii.header.get_zooms()).replace(" ", "")
    n_nii_params = Node(
        interface=Function(
            input_names=['nii_file'],
            output_names=['vsz'],
            function=read_nii
        ),
        name='nibabel_read-nii'
    )
    wf.connect([
        (n_inputs, n_nii_params, [('phase', 'nii_file')])
    ])

    # r2* and t2* mappping
    if run_args.t2starmap or run_args.r2starmap:
        n_t2s_r2s = Node(
            interface=t2s_r2s.T2sR2sInterface(),
            name='mrt_t2s-r2s'
        )
        wf.connect([
            (n_inputs, n_t2s_r2s, [('magnitude', 'magnitude')]),
            (mn_json_params, n_t2s_r2s, [('TE', 'TE')])
        ])
        if run_args.t2starmap: wf.connect([(n_t2s_r2s, n_outputs, [('t2starmap', 't2starmap')])])
        if run_args.r2starmap: wf.connect([(n_t2s_r2s, n_outputs, [('r2starmap', 'r2starmap')])])
        
    # scale phase data
    mn_phase_scaled = MapNode(
        interface=processphase.ScalePhaseInterface(),
        iterfield=['phase'],
        name='nibabel_numpy_scale-phase'
    )
    wf.connect([
        (n_inputs, mn_phase_scaled, [('phase', 'phase')])
    ])

    # swi
    if run_args.swi:
        n_swi = Node(
            interface=swi.ClearSwiInterface(),
            name='mrt_clearswi'
        )
        wf.connect([
            (mn_phase_scaled, n_swi, [('phase_scaled', 'phase')]),
            (n_inputs, n_swi, [('magnitude', 'magnitude')]),
            (mn_json_params, n_swi, [('TE', 'TE')]),
            (n_swi, n_outputs, [('swi', 'swi')]),
            (n_swi, n_outputs, [('swi_mip', 'swi.@swi_mip')]),
        ])

    # reorient to canonical
    def as_closest_canonical(phase, magnitude=None, mask=None):
        import os
        import nibabel as nib
        from scripts.qsmxt_functions import extend_fname
        out_phase = extend_fname(phase, "_canonical", out_dir=os.getcwd())
        out_mag = extend_fname(magnitude, "_canonical", out_dir=os.getcwd()) if magnitude else None
        out_mask = extend_fname(mask, "_canonical", out_dir=os.getcwd()) if mask else None
        if nib.aff2axcodes(nib.load(phase).affine) == ('R', 'A', 'S'): return phase, magnitude, mask
        nib.save(nib.as_closest_canonical(nib.load(phase)), out_phase)
        if magnitude: nib.save(nib.as_closest_canonical(nib.load(magnitude)), out_mag)
        if mask: nib.save(nib.as_closest_canonical(nib.load(mask)), out_mask)
        return out_phase, out_mag, out_mask
    mn_inputs_canonical = MapNode(
        interface=Function(
            input_names=['phase'] + (['magnitude'] if magnitude_files else []) + (['mask'] if mask_files else []),
            output_names=['phase', 'magnitude', 'mask'],
            function=as_closest_canonical
        ),
        iterfield=['phase'] + (['magnitude'] if magnitude_files else []) + (['mask'] if mask_files else []),
        name='nibabel_as-canonical'
    )
    wf.connect([
        (mn_phase_scaled, mn_inputs_canonical, [('phase_scaled', 'phase')])
    ])
    if magnitude_files:
        wf.connect([
            (n_inputs, mn_inputs_canonical, [('magnitude', 'magnitude')]),
        ])
    if mask_files:
        wf.connect([
            (n_inputs, mn_inputs_canonical, [('mask', 'mask')]),
        ])
    
    # resample to axial
    n_inputs_resampled = Node(
        interface=IdentityInterface(
            fields=['phase', 'magnitude', 'mask']
        ),
        name='nipype_inputs-resampled'
    )
    if magnitude_files:
        mn_resample_inputs = MapNode(
            interface=sampling.AxialSamplingInterface(
                obliquity_threshold=999 if run_args.obliquity_threshold == -1 else run_args.obliquity_threshold
            ),
            iterfield=['magnitude', 'phase', 'mask'] if mask_files else ['magnitude', 'phase'],
            mem_gb=min(3, run_args.mem_avail),
            name='nibabel_numpy_nilearn_axial-resampling'
        )
        mn_resample_inputs.plugin_args = gen_plugin_args(
            plugin_args={ 'overwrite': True },
            slurm_account=run_args.slurm[0],
            pbs_account=run_args.pbs,
            slurm_partition=run_args.slurm[1],
            name="axial_resampling",
            time="00:10:00",
            mem_gb=10,
            num_cpus=min(1, run_args.n_procs)
        )
        wf.connect([
            (mn_inputs_canonical, mn_resample_inputs, [('magnitude', 'magnitude')]),
            (mn_inputs_canonical, mn_resample_inputs, [('phase', 'phase')]),
            (mn_resample_inputs, n_inputs_resampled, [('magnitude', 'magnitude')]),
            (mn_resample_inputs, n_inputs_resampled, [('phase', 'phase')])
        ])
        if mask_files:
            wf.connect([
                (mn_inputs_canonical, mn_resample_inputs, [('mask', 'mask')]),
                (mn_resample_inputs, n_inputs_resampled, [('mask', 'mask')])
            ])
    else:
        wf.connect([
            (mn_inputs_canonical, n_inputs_resampled, [('phase', 'phase')])
        ])
        if mask_files:
            wf.connect([
                (mn_inputs_canonical, n_inputs_resampled, [('mask', 'mask')])
            ])

    # combine phase data if necessary
    n_inputs_combine = Node(
        interface=IdentityInterface(
            fields=['phase_unwrapped', 'frequency']
        ),
        name='phase-combined'
    )
    if run_args.combine_phase:
        n_romeo_combine = Node(
            interface=romeo.RomeoB0Interface(),
            name='mrt_romeo_combine',
            mem_gb=min(4, run_args.mem_avail)
        )
        n_romeo_combine.plugin_args = gen_plugin_args(
            plugin_args={ 'overwrite': True },
            slurm_account=run_args.slurm[0],
            pbs_account=run_args.pbs,
            slurm_partition=run_args.slurm[1],
            name="romeo_combine",
            time="00:10:00",
            mem_gb=10,
            num_cpus=min(1, run_args.n_procs)
        )
        wf.connect([
            (mn_json_params, n_romeo_combine, [('TE', 'TE')]),
            (n_inputs_resampled, n_romeo_combine, [('phase', 'phase')]),
            (n_inputs_resampled, n_romeo_combine, [('magnitude', 'magnitude')]),
            (n_romeo_combine, n_inputs_combine, [('frequency', 'frequency')]),
            (n_romeo_combine, n_inputs_combine, [('phase_unwrapped', 'phase_unwrapped')]),
        ])

    # === MASKING ===
    wf_masking = masking_workflow(
        run_args=run_args,
        mask_available=len(mask_files) > 0,
        magnitude_available=len(magnitude_files) > 0,
        qualitymap_available=False,
        fill_masks=True,
        add_bet=run_args.add_bet and run_args.filling_algorithm != 'bet',
        name="mask",
        index=0
    )
    wf.connect([
        (n_inputs_resampled, wf_masking, [('phase', 'masking_inputs.phase')]),
        (n_inputs_resampled, wf_masking, [('magnitude', 'masking_inputs.magnitude')]),
        (n_inputs_resampled, wf_masking, [('mask', 'masking_inputs.mask')]),
        (mn_json_params, wf_masking, [('TE', 'masking_inputs.TE')])
    ])

    # === QSM ===
    wf_qsm = qsm_workflow(run_args, "qsm", len(magnitude_files) > 0, qsm_erosions=run_args.tgv_erosions)

    wf.connect([
        (n_inputs_resampled, wf_qsm, [('phase', 'qsm_inputs.phase')]),
        (n_inputs_combine, wf_qsm, [('phase_unwrapped', 'qsm_inputs.phase_unwrapped')]),
        (n_inputs_combine, wf_qsm, [('frequency', 'qsm_inputs.frequency')]),
        (n_inputs_resampled, wf_qsm, [('magnitude', 'qsm_inputs.magnitude')]),
        (wf_masking, wf_qsm, [('masking_outputs.mask', 'qsm_inputs.mask')]),
        (mn_json_params, wf_qsm, [('TE', 'qsm_inputs.TE')]),
        (n_json_params, wf_qsm, [('B0', 'qsm_inputs.B0')]),
        (n_nii_params, wf_qsm, [('vsz', 'qsm_inputs.vsz')])
    ])
    wf_qsm.get_node('qsm_inputs').inputs.b0_direction = "(0,0,1)"
    
    n_qsm_average = Node(
        interface=nonzeroaverage.NonzeroAverageInterface(),
        name="nibabel_numpy_qsm-average"
    )
    wf.connect([
        (wf_qsm, n_qsm_average, [('qsm_outputs.qsm', 'in_files')]),
        (wf_masking, n_qsm_average, [('masking_outputs.mask', 'in_masks')])
    ])
    wf.connect([
        (n_qsm_average, n_outputs, [('out_file', 'qsm.@final' if not run_args.two_pass else 'qsm.filled')])
    ])

    # two-pass algorithm
    if run_args.two_pass:
        wf_masking_intermediate = masking_workflow(
            run_args=run_args,
            mask_available=len(mask_files) > 0,
            magnitude_available=len(magnitude_files) > 0,
            qualitymap_available=True,
            fill_masks=False,
            add_bet=False,
            name="mask-intermediate",
            index=1
        )
        wf.connect([
            (n_inputs_resampled, wf_masking_intermediate, [('phase', 'masking_inputs.phase')]),
            (n_inputs_resampled, wf_masking_intermediate, [('magnitude', 'masking_inputs.magnitude')]),
            (n_inputs_resampled, wf_masking_intermediate, [('mask', 'masking_inputs.mask')]),
            (mn_json_params, wf_masking_intermediate, [('TE', 'masking_inputs.TE')]),
            (wf_masking, wf_masking_intermediate, [('masking_outputs.quality_map', 'masking_inputs.quality_map')])
        ])

        wf_qsm_intermediate = qsm_workflow(run_args, "qsm-intermediate", len(magnitude_files) > 0, qsm_erosions=0)
        wf.connect([
            (n_inputs_resampled, wf_qsm_intermediate, [('phase', 'qsm_inputs.phase')]),
            (n_inputs_resampled, wf_qsm_intermediate, [('magnitude', 'qsm_inputs.magnitude')]),
            (n_inputs_combine, wf_qsm_intermediate, [('phase_unwrapped', 'qsm_inputs.phase_unwrapped')]),
            (n_inputs_combine, wf_qsm_intermediate, [('frequency', 'qsm_inputs.frequency')]),
            (mn_json_params, wf_qsm_intermediate, [('TE', 'qsm_inputs.TE')]),
            (n_json_params, wf_qsm_intermediate, [('B0', 'qsm_inputs.B0')]),
            (n_nii_params, wf_qsm_intermediate, [('vsz', 'qsm_inputs.vsz')]),
            (wf_masking_intermediate, wf_qsm_intermediate, [('masking_outputs.mask', 'qsm_inputs.mask')])
        ])
        wf_qsm_intermediate.get_node('qsm_inputs').inputs.b0_direction = "(0,0,1)"
                
        # two-pass combination
        mn_qsm_twopass = create_node(
            interface=twopass.TwopassNiftiInterface(),
            name='numpy_nibabel_twopass',
            iterfield=['in_file1', 'in_file2', 'mask'],
            is_map=run_args.combine_phase
        )
        wf.connect([
            (wf_qsm_intermediate, mn_qsm_twopass, [('qsm_outputs.qsm', 'in_file1')]),
            (wf_masking_intermediate, mn_qsm_twopass, [('masking_outputs.mask', 'mask')]),
            (wf_qsm, mn_qsm_twopass, [('qsm_outputs.qsm', 'in_file2')])
        ])

        # averaging
        n_qsm_twopass_average = Node(
            interface=nonzeroaverage.NonzeroAverageInterface(),
            name="nibabel_numpy_twopass-average"
        )
        wf.connect([
            (mn_qsm_twopass, n_qsm_twopass_average, [('out_file', 'in_files')]),
            (wf_masking_intermediate, n_qsm_twopass_average, [('masking_outputs.mask', 'in_masks')])
        ])
        wf.connect([
            (n_qsm_twopass_average, n_outputs, [('out_file', 'qsm.final')])
        ])
        
    return wf

def qsm_workflow(run_args, name, magnitude_available, qsm_erosions=0):
    wf = Workflow(name=f"{name}_workflow")

    slurm_account = run_args.slurm[0] if run_args.slurm and len(run_args.slurm) else None
    slurm_partition = run_args.slurm[1] if run_args.slurm and len(run_args.slurm) > 1 else None

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
            mn_laplacian = create_node(
                is_map=run_args.combine_phase,
                interface=laplacian.LaplacianInterface(),
                iterfield=['phase'],
                name='mrt_laplacian-unwrapping',
                mem_gb=min(3, run_args.mem_avail),
                n_procs=laplacian_threads
            )
            wf.connect([
                (n_inputs, mn_laplacian, [('phase', 'phase')]),
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
                romeo_threads = min(1, run_args.n_procs) if run_args.multiproc else 1
                mn_romeo = Node(
                    interface=romeo.RomeoB0Interface(),
                    name='mrt_romeo',
                    mem_gb=min(3, run_args.mem_avail)
                )
                mn_romeo.plugin_args = gen_plugin_args(
                    plugin_args={ 'overwrite': True },
                    slurm_account=run_args.slurm[0],
                    pbs_account=run_args.pbs,
                    slurm_partition=run_args.slurm[1],
                    name="Romeo",
                    mem_gb=3,
                    num_cpus=romeo_threads
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
        mn_normalize_phase = create_node(
            interface=processphase.PhaseToNormalizedInterface(
                scale_factor=1e6 if run_args.qsm_algorithm == 'nextqsm' else 1e6/(2*np.pi)
            ),
            name='nibabel-numpy_normalize-phase',
            iterfield=['phase', 'TE'],
            mem_gb=min(3, run_args.mem_avail),
            n_procs=normalize_phase_threads,
            is_map=run_args.combine_phase
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
        mn_normalize_freq = create_node(
            interface=processphase.FreqToNormalizedInterface(
                scale_factor=1e6 if run_args.qsm_algorithm == 'nextqsm' else 1e6/(2*np.pi)
            ),
            name='nibabel-numpy_normalize-freq',
            iterfield=['frequency'],
            mem_gb=min(3, run_args.mem_avail),
            n_procs=normalize_freq_threads,
            is_map=run_args.combine_phase
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
        mn_freq_to_phase = create_node(
            interface=processphase.FreqToPhaseInterface(TE=0.005, wraps=True),
            name='nibabel-numpy_freq-to-phase',
            iterfield=['frequency'],
            mem_gb=min(3, run_args.mem_avail),
            n_procs=freq_to_phase_threads,
            is_map=run_args.combine_phase
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
        mn_bf = create_node(
            interface=IdentityInterface(
                fields=['tissue_frequency', 'mask']
            ),
            iterfield=['tissue_frequency', 'mask'],
            name='bf-removal',
            is_map=run_args.combine_phase
        )
        if run_args.bf_algorithm == 'vsharp':
            vsharp_threads = min(2, run_args.n_procs) if run_args.multiproc else 2
            mn_vsharp = create_node(
                interface=qsmjl.VsharpInterface(num_threads=vsharp_threads),
                iterfield=['frequency', 'mask'],
                name='qsmjl_vsharp',
                n_procs=vsharp_threads,
                mem_gb=min(3, run_args.mem_avail),
                is_map=run_args.combine_phase
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
            mn_pdf = create_node(
                interface=qsmjl.PdfInterface(num_threads=pdf_threads),
                iterfield=['frequency', 'mask'],
                name='qsmjl_pdf',
                n_procs=pdf_threads,
                mem_gb=min(5, run_args.mem_avail),
                is_map=run_args.combine_phase
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
        mn_qsm = create_node(
            interface=nextqsm.NextqsmInterface(),
            name='nextqsm',
            iterfield=['phase', 'mask'],
            mem_gb=min(13, run_args.mem_avail),
            n_procs=nextqsm_threads,
            is_map=run_args.combine_phase
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
        mn_qsm = create_node(
            interface=qsmjl.RtsQsmInterface(num_threads=rts_threads),
            name='qsmjl_rts',
            iterfield=['tissue_frequency', 'mask'],
            n_procs=rts_threads,
            mem_gb=min(5, run_args.mem_avail),
            terminal_output="file_split",
            is_map=run_args.combine_phase
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
        mn_qsm = create_node(
            interface=qsmjl.TvQsmInterface(num_threads=tv_threads),
            name='qsmjl_tv',
            iterfield=['tissue_frequency', 'mask'],
            n_procs=tv_threads,
            mem_gb=min(5, run_args.mem_avail),
            terminal_output="file_split",
            is_map=run_args.combine_phase
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
        mn_qsm = create_node(
            interface=tgv.QSMappingInterface(
                iterations=run_args.tgv_iterations,
                alpha=run_args.tgv_alphas,
                erosions=qsm_erosions,
                num_threads=tgv_threads,
                out_suffix='_tgv',
                extra_arguments='--ignore-orientation --no-resampling',
            ),
            iterfield=['phase', 'TE', 'mask'],
            name='tgv',
            mem_gb=min(6, run_args.mem_avail),
            n_procs=tgv_threads,
            is_map=run_args.combine_phase
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

