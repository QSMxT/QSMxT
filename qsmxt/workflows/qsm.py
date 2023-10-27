import glob
import os

from nipype.pipeline.engine import Workflow, Node, MapNode
from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import DataSink

from nipype.interfaces.ants.registration import RegistrationSynQuick
from nipype.interfaces.ants.resampling import ApplyTransforms
from qsmxt.interfaces import nipype_interface_fastsurfer as fastsurfer
from qsmxt.interfaces import nipype_interface_mgz2nii as mgz2nii
from qsmxt.interfaces import nipype_interface_analyse as analyse
from qsmxt.interfaces import nipype_interface_romeo as romeo
from qsmxt.interfaces import nipype_interface_tgv_qsm_jl as tgvjl
from qsmxt.interfaces import nipype_interface_qsmjl as qsmjl
from qsmxt.interfaces import nipype_interface_nextqsm as nextqsm
from qsmxt.interfaces import nipype_interface_laplacian_unwrapping as laplacian
from qsmxt.interfaces import nipype_interface_processphase as processphase
from qsmxt.interfaces import nipype_interface_axialsampling as sampling
from qsmxt.interfaces import nipype_interface_t2star_r2star as t2s_r2s
from qsmxt.interfaces import nipype_interface_clearswi as swi
from qsmxt.interfaces import nipype_interface_nonzeroaverage as nonzeroaverage
from qsmxt.interfaces import nipype_interface_twopass as twopass
from qsmxt.interfaces import nipype_interface_resample_like as resample_like
from qsmxt.interfaces import nipype_interface_qsm_referencing as qsm_referencing

from qsmxt.scripts.logger import LogLevel, make_logger
from qsmxt.scripts.qsmxt_functions import gen_plugin_args, create_node
from qsmxt.workflows.masking import masking_workflow

import numpy as np
import nibabel as nib

def get_matching_files(bids_dir, subject, dtype="anat", suffixes=[], ext="nii*", session=None, run=None, part=None, acq=None):
    pattern = os.path.join(bids_dir, subject)
    if session:
        pattern = os.path.join(pattern, session)
    pattern = os.path.join(pattern, dtype) + os.path.sep
    if acq:
        pattern += f"*acq-{acq}_*"
    if run:
        pattern += f"*run-{run}_*"
    if part:
        pattern += f"*part-{part}_*"
    dir, fname = os.path.split(pattern)
    if suffixes:
        if fname:
            matching_files = [glob.glob(f"{pattern}_{suffix}.{ext}") for suffix in suffixes]
        else:
            matching_files = [glob.glob(os.path.join(dir, f"*{suffix}.{ext}")) for suffix in suffixes]
    else:
        matching_files = [glob.glob(f"{pattern}.{ext}")]
    return sorted([item for sublist in matching_files for item in sublist])

def init_qsm_workflow(run_args, subject, session=None, acq=None, run=None):
    logger = make_logger('main')
    run_id = f"{subject}" + (f".{session}" if session else "") + (f".acq-{acq}" if acq else "") + (f".run-{run}" if run else "")
    logger.log(LogLevel.INFO.value, f"Creating QSMxT workflow for {run_id}...")

    # get relevant files from this run
    t1w_files = get_matching_files(run_args.bids_dir, subject=subject, dtype="anat", suffixes=["T1w"], ext="nii*", session=session, run=None, part=None, acq=None)
    phase_files = get_matching_files(run_args.bids_dir, subject=subject, dtype="anat", suffixes=[], session=session, run=run, part="phase", acq=acq)[:run_args.num_echoes]
    magnitude_files = [path.replace("part-phase", "part-mag") for path in phase_files if os.path.exists(path.replace("_part-phase", "_part-mag"))]
    params_files = [path.replace('.nii.gz', '.nii').replace('.nii', '.json') for path in (phase_files if len(phase_files) else magnitude_files) if os.path.exists(path.replace('.nii.gz', '.nii').replace('.nii', '.json'))]
    mask_files = [
        mask_file for mask_file in get_matching_files(os.path.join(run_args.bids_dir, "derivatives", run_args.existing_masks_pipeline), subject=subject, dtype="anat", suffixes=["mask"], session=session, run=None, part=None, acq=None)[:run_args.num_echoes]
        if ('_space-orig' in mask_file or '_space-' not in mask_file)
        and ('_label-brain' in mask_file or '_label-' not in mask_file)
    ]
    
    # handle any errors related to files and adjust any settings if needed
    if run_args.do_segmentation and not t1w_files:
        logger.log(LogLevel.WARNING.value, f"{run_id}: Skipping segmentation - no T1w files found!")
        run_args.do_segmentation = False
    if run_args.do_analysis and not run_args.do_segmentation:
        logger.log(LogLevel.WARNING.value, f"{run_id}: Skipping analysis - segmentations required!")
        run_args.do_analysis = False
    if run_args.do_segmentation and not magnitude_files:
        logger.log(LogLevel.WARNING.value, f"{run_id}: Skipping segmentation - no GRE magnitude files found to register T1w segmentations to!")
        run_args.do_segmentation = False
    if run_args.do_segmentation and len(t1w_files) > 1:
        logger.log(LogLevel.WARNING.value, f"{run_id}: Using {t1w_files[0]} for segmentation - multiple T1w files found!")
    if run_args.do_qsm and not phase_files:
        logger.log(LogLevel.WARNING.value, f"{run_id}: Skipping QSM - No phase files found!")
        run_args.do_qsm = False
        run_args.do_swi = False
    if len(phase_files) != len(params_files):
        logger.log(LogLevel.ERROR.value, f"{run_id}: Cannot process this run - an unequal number of JSON and phase files are present!")
        return
    if len(phase_files) == 1 and run_args.combine_phase:
        run_args.combine_phase = False
    if run_args.do_qsm and run_args.use_existing_masks:
        if not mask_files:
            logger.log(LogLevel.WARNING.value, f"{run_id}: --use_existing_masks specified but no masks found in {run_args.use_existing_masks} derivatives. Reverting to {run_args.masking_algorithm} masking algorithm.")
            run_args.use_existing_masks = False
        else:
            if len(mask_files) > 1:
                if run_args.combine_phase:
                    logger.log(LogLevel.WARNING.value, f"{run_id}: --combine_phase specified but multiple masks found with --use_existing_masks. Using the first mask only.")
                    mask_files = [mask_files[0]]
                elif len(mask_files) != len(phase_files):
                    logger.log(LogLevel.WARNING.value, f"{run_id}: --use_existing_masks specified but unequal number of mask and phase files present. Using the first mask only.")
                    mask_files = [mask_files[0]]
            if mask_files:
                run_args.inhomogeneity_correction = False
                run_args.two_pass = False
                run_args.add_bet = False
    if not magnitude_files:
        if run_args.do_r2starmap:
            logger.log(LogLevel.WARNING.value, f"{run_id}: Cannot compute R2* - no magnitude files found.")
            run_args.do_r2starmap = False
        if run_args.do_t2starmap:
            logger.log(LogLevel.WARNING.value, f"{run_id}: Cannot compute T2* - no magnitude files found.")
            run_args.do_t2starmap = False
        if run_args.do_swi:
            logger.log(LogLevel.WARNING.value, f"{run_id}: Cannot compute SWI - no magnitude files found.")
            run_args.do_swi = False
        if run_args.do_qsm:
            logger.log(LogLevel.WARNING.value, f"{run_id}: Cannot resample axially - no magnitude files were found (expect poor results only from oblique acquisitions)!")
            if run_args.masking_input == 'magnitude':
                logger.log(LogLevel.WARNING.value, f"{run_id}: Cannot use magnitude-based masking - no magnitude files found!")
                run_args.masking_input = 'phase'
                run_args.masking_algorithm = 'threshold'
                run_args.inhomogeneity_correction = False
                run_args.add_bet = False
            if run_args.add_bet:
                logger.log(LogLevel.WARNING.value, f"{run_id}: Cannot use --add_bet - no magnitude files found!")
                run_args.add_bet = False
    elif len(magnitude_files) != len(phase_files) and run_args.do_qsm and run_args.masking_input == 'magnitude':
        logger.log(LogLevel.WARNING.value, f"{run_id}: Cannot use magnitude-based masking - unequal number of phase and magnitude files found!")
        run_args.masking_input = 'phase'
        run_args.masking_algorithm = 'threshold'
        run_args.inhomogeneity_correction = False
        run_args.add_bet = False
    if len(magnitude_files) == 1:
        if run_args.do_r2starmap:
            logger.log(LogLevel.WARNING.value, f"{run_id}: Cannot compute R2* - at least two echoes are needed!")
            run_args.do_r2starmap = False
        if run_args.do_t2starmap:
            logger.log(LogLevel.WARNING.value, f"{run_id}: Cannot compute T2* - at least two echoes are needed!")
            run_args.do_t2starmap = False

    if run_args.do_qsm or run_args.do_swi:
        if not all(all(nib.load(phase_files[i]).header['dim'] == nib.load(phase_files[0]).header['dim']) for i in range(len(phase_files))):
            logger.log(LogLevel.ERROR.value, f"{run_id}: Cannot do QSM or SWI - dimensions of phase files are unequal!")
            run_args.do_qsm = False
            run_args.do_swi = False
    if run_args.do_qsm and (run_args.masking_input == 'magnitude' or run_args.inhomogeneity_correction or run_args.add_bet):
        if not all(all(nib.load(magnitude_files[i]).header['dim'] == nib.load(phase_files[0]).header['dim']) for i in range(len(magnitude_files))):
            logger.log(LogLevel.ERROR.value, f"{run_id}: Cannot use magnitude for masking - dimensions of magnitude files are not all equal to phase files!")
            run_args.masking_input = 'phase'
            run_args.inhomogeneity_correction = False
            run_args.add_bet = False
        if run_args.use_existing_masks:
            if not all(all(nib.load(mask_files[i]).header['dim'] == nib.load(phase_files[0]).header['dim']) for i in range(len(mask_files))):
                logger.log(LogLevel.ERROR.value, f"{run_id}: Cannot use existing masks - mask dimensions are not all equal to phase files!")
                run_args.use_existing_masks = False
    elif run_args.do_r2starmap or run_args.do_t2starmap:
        if not all(all(nib.load(magnitude_files[i]).header['dim'] == nib.load(magnitude_files[0]).header['dim']) for i in range(len(magnitude_files))):
            logger.log(LogLevel.ERROR.value, f"{run_id}: Cannot do T2*/R2* mapping - magnitude dimensions are not all equal!")
            run_args.do_r2starmap = False
            run_args.do_t2starmap = False

    if run_args.do_qsm or run_args.do_swi:
        if any(nib.load(phase_files[i]).header['dim'][0] > 3 for i in range(len(phase_files))):
            logger.log(LogLevel.ERROR.value, f"{run_id}: Cannot do QSM or SWI - >3D phase files detected! Each volume must be 3D, coil-combined, and represent a single echo for BIDS-compliance.")
            run_args.do_qsm = False
            run_args.do_swi = False
    if (run_args.do_qsm and (run_args.masking_input == 'magnitude' or run_args.inhomogeneity_correction or run_args.add_bet)) or run_args.do_r2starmap or run_args.do_t2starmap:
        if any(nib.load(magnitude_files[i]).header['dim'][0] > 3 for i in range(len(magnitude_files))):
            logger.log(LogLevel.ERROR.value, f"{run_id}: Cannot do magnitude-based masking - >3D magnitude files detected! Each volume must be 3D, coil-combined, and represent a single echo for BIDS-compliance.")
            run_args.masking_input = 'phase'
            run_args.inhomogeneity_correction = False
            run_args.add_bet = False
            run_args.do_r2starmap = False
            run_args.do_t2starmap = False
    if (run_args.do_qsm and run_args.use_existing_masks):
        if any(nib.load(mask_files[i]).header['dim'][0] > 3 for i in range(len(mask_files))):
            logger.log(LogLevel.ERROR.value, f"{run_id}: Cannot use existing masks - >3D masks detected! Each mask must be 3D only for BIDS-compliance.")
            run_args.use_existing_masks = False

    if not any([run_args.do_qsm, run_args.do_swi, run_args.do_t2starmap, run_args.do_r2starmap, run_args.do_segmentation]):
        return
    
    # create nipype workflow for this run
    wf = Workflow(f"qsmxt" + (f"_acq-{acq}" if acq else "") + (f"_run-{run}" if run else ""), base_dir=os.path.join(run_args.output_dir, "workflow", os.path.join(subject, session) if session else subject, acq or "", run or ""))

    # inputs and outputs
    n_inputs = Node(
        IdentityInterface(
            fields=['phase', 'magnitude', 'params_files', 'mask']
        ),
        name='nipype_getfiles'
    )
    n_inputs.inputs.phase = phase_files[0] if len(phase_files) == 1 else phase_files
    n_inputs.inputs.magnitude = magnitude_files[0] if len(magnitude_files) == 1 else magnitude_files
    n_inputs.inputs.params_files = params_files[0] if len(params_files) == 1 else params_files
    if not run_args.combine_phase and len(phase_files) > 1 and len(mask_files) == 1:
        mask_files = [mask_files[0] for x in phase_files]
        n_inputs.inputs.mask = mask_files
    else:
        n_inputs.inputs.mask = mask_files[0] if len(mask_files) == 1 else mask_files

    n_outputs = Node(
        IdentityInterface(
            fields=['qsm', 'qsm_singlepass', 'swi', 'swi_mip', 't2s', 'r2s', 't1w_segmentation', 'qsm_segmentation', 'transform', 'analysis_csv']
        ),
        name='qsmxt_outputs'
    )
    n_datasink = Node(
        interface=DataSink(base_directory=run_args.output_dir),
        name='qsmxt_datasink'
    )
    wf.connect([
        (n_outputs, n_datasink, [('qsm', 'qsm')]),
        (n_outputs, n_datasink, [('qsm_singlepass', 'qsm.singlepass')]),
        (n_outputs, n_datasink, [('swi', 'swi.@swi')]),
        (n_outputs, n_datasink, [('swi_mip', 'swi.@mip')]),
        (n_outputs, n_datasink, [('t2s', 't2s')]),
        (n_outputs, n_datasink, [('r2s', 'r2s')]),
        (n_outputs, n_datasink, [('t1w_segmentation', 'segmentations.t1w')]),
        (n_outputs, n_datasink, [('qsm_segmentation', 'segmentations.qsm')]),
        (n_outputs, n_datasink, [('transform', 'segmentations.transforms')]),
        (n_outputs, n_datasink, [('analysis_csv', 'analysis')])
    ])
    
    # read echotime and field strengths from json files
    def read_json_me(params_file):
        import json
        with open(params_file, 'rt') as json_file:
            data = json.load(json_file)
        te = data['EchoTime']
        json_file.close()
        return te
    def read_json_se(params_files):
        import json
        with open(params_files[0] if isinstance(params_files, list) else params_files, 'rt') as json_file:
            data = json.load(json_file)
        B0 = data['MagneticFieldStrength']
        json_file.close()
        return B0
    mn_json_params = create_node(
        interface=Function(
            input_names=['params_file'],
            output_names=['TE'],
            function=read_json_me
        ),
        iterfield=['params_file'],
        name='func_read-json-me',
        is_map=len(params_files) > 1
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
        import numpy as np
        if isinstance(nii_file, list): nii_file = nii_file[0]
        nii = nib.load(nii_file)
        return np.array(nii.header.get_zooms()).tolist()
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

    # reorient to canonical
    def as_closest_canonical(phase, magnitude=None, mask=None):
        import os
        import nibabel as nib
        from qsmxt.scripts.qsmxt_functions import extend_fname

        def as_closest_canonical_i(in_file):
            if nib.aff2axcodes(nib.load(in_file).affine) == ('R', 'A', 'S'):
                return in_file
            else:
                out_file = extend_fname(in_file, "_canonical", out_dir=os.getcwd())
                nib.save(nib.as_closest_canonical(nib.load(in_file)), out_file)
                return out_file
        
        out_phase = as_closest_canonical_i(phase) if not isinstance(phase, list) else [as_closest_canonical_i(phase_i) for phase_i in phase]
        out_mag = None
        out_mask = None
        if magnitude:
            if isinstance(magnitude, list):
                out_mag = [as_closest_canonical_i(magnitude_i) for magnitude_i in magnitude]
            else:
                out_mag = as_closest_canonical_i(magnitude)
        if mask:
            if isinstance(mask, list):
                out_mask = [as_closest_canonical_i(mask_i) for mask_i in mask]
            else:
                out_mask = as_closest_canonical_i(mask)
        
        return out_phase, out_mag, out_mask
    mn_inputs_canonical = Node(
        interface=Function(
            input_names=[] + (['phase'] if phase_files else []) + (['magnitude'] if magnitude_files else []) + (['mask'] if mask_files and run_args.use_existing_masks else []),
            output_names=['phase', 'magnitude', 'mask'],
            function=as_closest_canonical
        ),
        #iterfield=['phase'] + (['magnitude'] if magnitude_files else []) + (['mask'] if mask_files else []),
        name='nibabel_as-canonical',
        #is_map=len(phase_files) > 1
    )
    if phase_files and (run_args.do_swi or run_args.do_qsm):
        wf.connect([
            (n_inputs, mn_inputs_canonical, [('phase', 'phase')]),
        ])
    if magnitude_files:
        wf.connect([
            (n_inputs, mn_inputs_canonical, [('magnitude', 'magnitude')]),
        ])
    if mask_files and run_args.use_existing_masks:
        wf.connect([
            (n_inputs, mn_inputs_canonical, [('mask', 'mask')]),
        ])

    # scale phase
    mn_phase_scaled = create_node(
        interface=processphase.ScalePhaseInterface(),
        iterfield=['phase'],
        name='nibabel_numpy_scale-phase',
        is_map=len(phase_files) > 1
    )
    wf.connect([
        (mn_inputs_canonical, mn_phase_scaled, [('phase', 'phase')])
    ])

    # r2* and t2* mappping
    if run_args.do_t2starmap or run_args.do_r2starmap:
        n_t2s_r2s = Node(
            interface=t2s_r2s.T2sR2sInterface(),
            name='mrt_t2s-r2s'
        )
        wf.connect([
            (mn_inputs_canonical, n_t2s_r2s, [('magnitude', 'magnitude')]),
            (mn_json_params, n_t2s_r2s, [('TE', 'TE')])
        ])
        if run_args.do_t2starmap: wf.connect([(n_t2s_r2s, n_outputs, [('t2starmap', 't2s')])])
        if run_args.do_r2starmap: wf.connect([(n_t2s_r2s, n_outputs, [('r2starmap', 'r2s')])])
    
    if not (run_args.do_swi or run_args.do_qsm):
        return wf

    # swi
    if run_args.do_swi:
        n_swi_threads = min(run_args.n_procs, 6) if run_args.multiproc else 6
        n_swi = Node(
            interface=swi.ClearSwiInterface(
                num_threads=n_swi_threads
            ),
            name='mrt_clearswi',
            mem_gb=9
        )
        n_swi.plugin_args = gen_plugin_args(
            plugin_args={ 'overwrite': True },
            slurm_account=run_args.slurm[0],
            pbs_account=run_args.pbs,
            slurm_partition=run_args.slurm[1],
            name="SWI",
            mem_gb=9,
            num_cpus=n_swi_threads
        )
        wf.connect([
            (mn_phase_scaled, n_swi, [('phase', 'phase')]),
            (mn_inputs_canonical, n_swi, [('magnitude', 'magnitude')]),
            (mn_json_params, n_swi, [('TE', 'TEs' if len(phase_files) > 1 else 'TE')]),
            (n_swi, n_outputs, [('swi', 'swi')]),
            (n_swi, n_outputs, [('swi_mip', 'swi_mip')])
        ])

    # segmentation
    if run_args.do_segmentation:
        n_registration_threads = min(run_args.n_procs, 6) if run_args.multiproc else 6
        n_registration = Node(
            interface=RegistrationSynQuick(
                num_threads=n_registration_threads,
                fixed_image=magnitude_files[0],
                moving_image=t1w_files[0],
                output_prefix=f"{subject}_{session}" + (f"_acq-{acq}" if acq else "") + (f"_run-{run}" if run else "") + "_"
            ),
            name='ants_register-t1-to-qsm',
            n_procs=n_registration_threads,
            mem_gb=min(run_args.mem_avail, 8)
        )
        n_registration.plugin_args = gen_plugin_args(
            plugin_args={ 'overwrite': True },
            slurm_account=run_args.slurm[0],
            pbs_account=run_args.pbs,
            slurm_partition=run_args.slurm[1],
            name="ANTS",
            mem_gb=8,
            num_cpus=n_registration_threads
        )

        # segment t1
        n_fastsurfer_threads = min(run_args.n_procs, 8) if run_args.multiproc else 8
        n_fastsurfer = Node(
            interface=fastsurfer.FastSurferInterface(
                in_file=t1w_files[0],
                num_threads=n_fastsurfer_threads
            ),
            name='fastsurfer_segment-t1',
            n_procs=n_fastsurfer_threads,
            mem_gb=min(run_args.mem_avail, 12)
        )
        n_fastsurfer.plugin_args = gen_plugin_args(
            plugin_args={ 'overwrite': True },
            slurm_account=run_args.slurm[0],
            pbs_account=run_args.pbs,
            slurm_partition=run_args.slurm[1],
            name="FASTSURFER",
            mem_gb=12,
            num_cpus=n_fastsurfer_threads
        )

        # convert segmentation to nii
        n_fastsurfer_aseg_nii = Node(
            interface=mgz2nii.Mgz2NiiInterface(),
            name='numpy_numpy_nibabel_mgz2nii'
        )
        wf.connect([
            (n_fastsurfer, n_fastsurfer_aseg_nii, [('out_file', 'in_file')])
        ])

        # get first canonical magnitude
        n_getfirst_canonical_magnitude = Node(
            interface=Function(
                input_names=['magnitude'],
                output_names=['magnitude'],
                function=lambda magnitude: magnitude[0] if isinstance(magnitude, list) else magnitude
            ),
            name='func_getfirst-canonical-magnitude'
        )
        wf.connect([
            (mn_inputs_canonical, n_getfirst_canonical_magnitude, [('magnitude', 'magnitude')])
        ])

        # apply transforms to segmentation
        n_transform_segmentation = Node(
            interface=ApplyTransforms(
                dimension=3,
                interpolation="NearestNeighbor",
                output_image=f"{run_id.replace('.', '_')}_segmentation_trans.nii"
            ),
            name='ants_transform-segmentation-to-qsm'
        )
        wf.connect([
            (n_fastsurfer_aseg_nii, n_transform_segmentation, [('out_file', 'input_image')]),
            (n_getfirst_canonical_magnitude, n_transform_segmentation, [('magnitude', 'reference_image')]),
            (n_registration, n_transform_segmentation, [('out_matrix', 'transforms')])
        ])

        wf.connect([
            (n_fastsurfer_aseg_nii, n_outputs, [('out_file', 't1w_segmentation')]),
            (n_transform_segmentation, n_outputs, [('output_image', 'qsm_segmentation')]),
            (n_registration, n_outputs, [('out_matrix', 'transform')])
        ])

    if not run_args.do_qsm:
        return wf
    
    # resample to axial
    n_inputs_resampled = Node(
        interface=IdentityInterface(
            fields=['phase', 'magnitude', 'mask']
        ),
        name='nipype_inputs-resampled'
    )
    if magnitude_files:
        mn_resample_inputs = create_node(
            interface=sampling.AxialSamplingInterface(
                obliquity_threshold=999 if run_args.obliquity_threshold == -1 else run_args.obliquity_threshold
            ),
            iterfield=['magnitude', 'phase'],
            mem_gb=min(4, run_args.mem_avail),
            name='nibabel_numpy_nilearn_axial-resampling',
            is_map=len(phase_files) > 1
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
            (mn_phase_scaled, mn_resample_inputs, [('phase', 'phase')]),
            (mn_resample_inputs, n_inputs_resampled, [('magnitude', 'magnitude')]),
            (mn_resample_inputs, n_inputs_resampled, [('phase', 'phase')])
        ])
        if mask_files and run_args.use_existing_masks:
            mn_resample_mask = create_node(
                interface=sampling.AxialSamplingInterface(
                    obliquity_threshold=999 if run_args.obliquity_threshold == -1 else run_args.obliquity_threshold
                ),
                iterfield=['mask'],
                mem_gb=min(3, run_args.mem_avail),
                name='nibabel_numpy_nilearn_axial-resampling-mask',
                is_map=isinstance(n_inputs.inputs.mask, list) and len(n_inputs.inputs.mask) > 1
            )
            mn_resample_mask.plugin_args = gen_plugin_args(
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
                (mn_inputs_canonical, mn_resample_mask, [('mask', 'mask')]),
                (mn_resample_mask, n_inputs_resampled, [('mask', 'mask')])
            ])
    else:
        wf.connect([
            (mn_phase_scaled, n_inputs_resampled, [('phase', 'phase')])
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
            mem_gb=min(8, run_args.mem_avail)
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
            (mn_json_params, n_romeo_combine, [('TE', 'TEs')]),
            (n_inputs_resampled, n_romeo_combine, [('phase', 'phase')]),
            (n_inputs_resampled, n_romeo_combine, [('magnitude', 'magnitude')]),
            (n_romeo_combine, n_inputs_combine, [('frequency', 'frequency')]),
            (n_romeo_combine, n_inputs_combine, [('phase_unwrapped', 'phase_unwrapped')]),
        ])

    # === MASKING ===
    wf_masking = masking_workflow(
        run_args=run_args,
        mask_available=len(mask_files) > 0 and run_args.use_existing_masks,
        magnitude_available=len(magnitude_files) > 0,
        qualitymap_available=False,
        fill_masks=True,
        add_bet=run_args.add_bet and run_args.filling_algorithm != 'bet',
        use_maps=len(phase_files) > 1 and not run_args.combine_phase,
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
    wf_qsm = qsm_workflow(run_args, "qsm", len(magnitude_files) > 0, len(phase_files) > 1 and not run_args.combine_phase, qsm_erosions=run_args.tgv_erosions)

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
    wf_qsm.get_node('qsm_inputs').inputs.b0_direction = [0, 0, 1]
    
    n_qsm_average = Node(
        interface=nonzeroaverage.NonzeroAverageInterface(),
        name="nibabel_numpy_qsm-average"
    )
    wf.connect([
        (wf_qsm, n_qsm_average, [('qsm_outputs.qsm', 'in_files')]),
        (wf_masking, n_qsm_average, [('masking_outputs.mask', 'in_masks')])
    ])

    n_resample_qsm = Node(
        interface=resample_like.ResampleLikeInterface(),
        name='nibabel_numpy_nilearn_resample-qsm'
    )
    wf.connect([
        (n_qsm_average, n_resample_qsm, [('out_file', 'in_file')]),
        (mn_inputs_canonical, n_resample_qsm, [('phase', 'ref_file')])
    ])

    if run_args.qsm_reference:
        n_qsm_referenced = Node(
            interface=qsm_referencing.ReferenceQSMInterface(
                in_seg_values=run_args.qsm_reference if isinstance(run_args.qsm_reference, list) and run_args.do_segmentation else [1]
            ),
            name='nibabel_numpy_qsm-referenced'
        )
        wf.connect([
            (n_resample_qsm, n_qsm_referenced, [('out_file', 'in_qsm')]),
            (n_qsm_referenced, n_outputs, [('out_file', 'qsm' if not run_args.two_pass else 'qsm_singlepass')])
        ])
        if isinstance(run_args.qsm_reference, list) and run_args.do_segmentation:
            wf.connect([
                (n_transform_segmentation, n_qsm_referenced, [('output_image', 'in_seg')])
            ])
    else:
        wf.connect([
            (n_resample_qsm, n_outputs, [('out_file', 'qsm' if not run_args.two_pass else 'qsm_singlepass')])
        ])

    # two-pass algorithm
    if run_args.two_pass:
        wf_masking_intermediate = masking_workflow(
            run_args=run_args,
            mask_available=False,
            magnitude_available=len(magnitude_files) > 0,
            qualitymap_available=True,
            fill_masks=False,
            add_bet=False,
            use_maps=len(phase_files) > 1 and not run_args.combine_phase,
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

        wf_qsm_intermediate = qsm_workflow(run_args, "qsm-intermediate", len(magnitude_files) > 0, len(phase_files) > 1 and not run_args.combine_phase, qsm_erosions=0)
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
        wf_qsm_intermediate.get_node('qsm_inputs').inputs.b0_direction = [0, 0, 1]
                
        # two-pass combination
        mn_qsm_twopass = create_node(
            interface=twopass.TwopassNiftiInterface(),
            name='numpy_nibabel_twopass',
            iterfield=['in_file', 'in_filled', 'mask'],
            is_map=len(phase_files) > 1 and not run_args.combine_phase
        )
        wf.connect([
            (wf_qsm_intermediate, mn_qsm_twopass, [('qsm_outputs.qsm', 'in_file')]),
            (wf_masking_intermediate, mn_qsm_twopass, [('masking_outputs.mask', 'mask')]),
            (wf_qsm, mn_qsm_twopass, [('qsm_outputs.qsm', 'in_filled')])
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

        n_resample_qsm = Node(
            interface=resample_like.ResampleLikeInterface(),
            name='resample_qsm-twopass'
        )
        wf.connect([
            (n_qsm_twopass_average, n_resample_qsm, [('out_file', 'in_file')]),
            (mn_inputs_canonical, n_resample_qsm, [('phase', 'ref_file')])
        ])

        if run_args.qsm_reference:
            n_qsm_twopass_referenced = Node(
                interface=qsm_referencing.ReferenceQSMInterface(
                    in_seg_values=run_args.qsm_reference if isinstance(run_args.qsm_reference, list) and run_args.do_segmentation else [1]
                ),
                name='nibabel_numpy_qsm-referenced-twopass'
            )
            wf.connect([
                (n_resample_qsm, n_qsm_twopass_referenced, [('out_file', 'in_qsm')]),
                (n_qsm_twopass_referenced, n_outputs, [('out_file', 'qsm')])
            ])
            if isinstance(run_args.qsm_reference, list) and run_args.do_segmentation:
                wf.connect([
                    (n_transform_segmentation, n_qsm_twopass_referenced, [('output_image', 'in_seg')])
                ])
        else:
            wf.connect([
                (n_resample_qsm, n_outputs, [('out_file', 'qsm')])
            ])

    if run_args.do_segmentation and run_args.do_analysis:
        n_analyse_qsm = Node(
            interface=analyse.AnalyseInterface(
                in_labels=run_args.labels_file
            ),
            name='analyse_qsm',
            mem_gb=2
        )
        wf.connect([
            (n_transform_segmentation, n_analyse_qsm, [('output_image', 'in_segmentation')]),
            (n_resample_qsm, n_analyse_qsm, [('out_file', 'in_file')]),
            (n_analyse_qsm, n_outputs, [('out_csv', 'analysis_csv')])
        ])

    
    return wf

def qsm_workflow(run_args, name, magnitude_available, use_maps, qsm_erosions=0):
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
                is_map=use_maps,
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
                    mem_gb=min(8, run_args.mem_avail)
                )
                mn_romeo.plugin_args = gen_plugin_args(
                    plugin_args={ 'overwrite': True },
                    slurm_account=run_args.slurm[0],
                    pbs_account=run_args.pbs,
                    slurm_partition=run_args.slurm[1],
                    name="Romeo",
                    mem_gb=5,
                    num_cpus=romeo_threads
                )
                wf.connect([
                    (n_inputs, mn_romeo, [('phase', 'phase')]),
                    (n_inputs, mn_romeo, [('TE', 'TEs' if use_maps else 'TE')]),
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
            is_map=use_maps
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
            is_map=use_maps
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
    if run_args.qsm_algorithm in ['tgv', 'tgvjl'] and run_args.combine_phase:
        freq_to_phase_threads = min(2, run_args.n_procs) if run_args.multiproc else 2
        mn_freq_to_phase = create_node(
            interface=processphase.FreqToPhaseInterface(TE=0.005, wraps=True),
            name='nibabel-numpy_freq-to-phase',
            iterfield=['frequency'],
            mem_gb=min(3, run_args.mem_avail),
            n_procs=freq_to_phase_threads,
            is_map=use_maps
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
            name='bf-removal'
        )
        if run_args.bf_algorithm == 'vsharp':
            vsharp_threads = min(2, run_args.n_procs) if run_args.multiproc else 2
            mn_vsharp = create_node(
                interface=qsmjl.VsharpInterface(num_threads=vsharp_threads),
                iterfield=['frequency', 'mask'],
                name='qsmjl_vsharp',
                n_procs=vsharp_threads,
                mem_gb=min(3, run_args.mem_avail),
                is_map=use_maps
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
                is_map=use_maps
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
            is_map=use_maps
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
            name="NEXTQSM",
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
            is_map=use_maps
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
            is_map=use_maps
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
        tgv_threads = min(4, run_args.n_procs)
        tgv_mem = min(4, run_args.mem_avail) if run_args.multiproc else 6
        print("TGV")
        mn_qsm = create_node(
            interface=tgvjl.TGVQSMJlInterface(
                erosions=qsm_erosions,
                alpha=run_args.tgv_alphas,
                iterations=run_args.tgv_iterations,
                num_threads=tgv_threads
            ),
            name='tgv',
            iterfield=['phase', 'TE', 'mask'],
            is_map=use_maps,
            mem_gb=tgv_mem,
            n_procs=tgv_threads
        )
        mn_qsm.plugin_args = gen_plugin_args(
            plugin_args={ 'overwrite': True },
            slurm_account=run_args.slurm[0],
            pbs_account=run_args.pbs,
            slurm_partition=run_args.slurm[1],
            name='TGV',
            time="01:00:00",
            mem_gb=tgv_mem,
            num_cpus=tgv_threads
        )
        wf.connect([
            (n_inputs, mn_qsm, [('mask', 'mask')]),
            (n_inputs, mn_qsm, [('B0', 'B0')]),
            (mn_qsm, n_outputs, [('qsm', 'qsm')]),
        ])
        if run_args.combine_phase:
            mn_qsm.inputs.TE = 0.005
            wf.connect([
                (n_phase_normalized, mn_qsm, [('phase_normalized', 'phase')])
            ])
        else:
            wf.connect([
                (n_inputs, mn_qsm, [('phase', 'phase')]),
                (n_inputs, mn_qsm, [('TE', 'TE')])
            ])

    
    return wf

