import glob
import os

from nipype.pipeline.engine import Workflow, Node
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
from qsmxt.interfaces import nipype_interface_nii2dcm as nii2dcm
from qsmxt.interfaces import nipype_interface_copyfile as copyfile
from qsmxt.interfaces import nipype_interface_copy_json_sidecar as copy_json_sidecar
from qsmxt.interfaces import nipype_interface_create_reference_dicom as create_reference_dicom

from qsmxt.scripts.logger import LogLevel, make_logger
from qsmxt.scripts.qsmxt_functions import gen_plugin_args, create_node
from qsmxt.workflows.masking import masking_workflow

import numpy as np
import nibabel as nib


def insert_before(wf, target_node_name, new_node, target_attribute):
    """
    Inserts a new node before a specified target node in a Nipype workflow, updating existing connections.

    :param wf: The workflow object.
    :param target_node_name: The name of the target node.
    :param new_node: The new Node object to insert.
    :param target_attribute: The target attribute in the target node to which connections are made.
    """
    target_node = wf.get_node(target_node_name)

    # Find all source nodes connected to the target attribute of the target node
    source_edges = [(u, v, d) for u, v, d in wf._graph.edges(data=True) if v._name == target_node_name and d['connect'][0][1] == target_attribute]

    # Disconnect the source nodes from the target node
    for u, v, d in source_edges:
        source_attribute = d['connect'][0][0]
        wf.disconnect([(u, v, [(source_attribute, target_attribute)])])

    # Add the new node to the workflow
    wf.add_nodes([new_node])

    # Connect the source nodes to the new node and the new node to the target node
    for u, _, _ in source_edges:
        source_attribute = d['connect'][0][0]
        wf.connect([(u, new_node, [(source_attribute, target_attribute)])])
        wf.connect([(new_node, target_node, [(target_attribute, target_attribute)])])

def get_node(wf, node_name):
    for node in wf._get_all_nodes():
        if node_name in node._name:
            return node
    return None

def get_preceding_node_and_attribute(wf, target_node_name, target_attribute):
    """
    Retrieves the input node and its output attribute name connected to a specific attribute 
    of a target node in a Nipype workflow.

    :param wf: The workflow object.
    :param target_node_name: The name of the target node.
    :param target_attribute: The target attribute in the target node.
    :return: A tuple (input node, output attribute name) if found, otherwise (None, None).
    """
    # Search for an edge where the target node and attribute match
    for u, v, d in wf._graph.edges(data=True):
        for source_attribute, target_attr in d['connect']:
            if v._name == target_node_name and target_attr == target_attribute:
                return u, source_attribute

    return None, None

def get_matching_files(bids_dir, subject, dtype="anat", suffixes=[], ext="nii*", session=None, space=None, run=None, part=None, acq=None, rec=None, inv=None):
    pattern = os.path.join(bids_dir, subject)
    if session:
        pattern = os.path.join(pattern, session)
    pattern = os.path.join(pattern, dtype) + os.path.sep

    # Build required entity patterns (without wildcards for exact matching)
    required_entities = []
    if space:
        required_entities.append(f"_space-{space}_")
    if acq:
        required_entities.append(f"_acq-{acq}_")
    if rec:
        required_entities.append(f"_rec-{rec}_")
    if run:
        required_entities.append(f"_run-{run}_")
    if inv:
        required_entities.append(f"_inv-{inv}_")
    if part:
        required_entities.append(f"_part-{part}_")

    # Get all files matching suffixes
    if suffixes:
        all_files = []
        for suffix in suffixes:
            all_files.extend(glob.glob(os.path.join(pattern, f"*_{suffix}.{ext}")))
    else:
        all_files = glob.glob(os.path.join(pattern, f"*.{ext}"))

    # Filter files that contain all required entities
    matching_files = []
    for filepath in all_files:
        filename = os.path.basename(filepath)
        if all(entity in filename for entity in required_entities):
            matching_files.append(filepath)
        
    return sorted(matching_files)

def init_qsm_workflow(run_args, subject, session=None, acq=None, rec=None, inv=None, suffix=None, run=None):
    logger = make_logger('main')
    run_id = f"{subject}" + (f".{session}" if session else "") + \
        (f".acq-{acq}" if acq else "") + \
        (f".rec-{rec}" if rec else "") + \
        (f".{suffix}" if suffix else "") + \
        (f".inv-{inv}" if inv else "") + \
        (f".run-{run}" if run else "")
    
    logger.log(LogLevel.INFO.value, f"Creating QSMxT workflow for {run_id}...")

    # Retrieve relevant files for this run.
    t1w_files = get_matching_files(run_args.bids_dir, subject=subject, dtype="anat", suffixes=["T1w"], session=session, run=None, part=None, acq=None, rec=None, inv=None)
    phase_files = get_matching_files(run_args.bids_dir, subject=subject, dtype="anat", suffixes=[suffix] if suffix else None, session=session, run=run, part="phase", acq=acq, rec=rec, inv=inv)[:run_args.num_echoes]
    magnitude_files = get_matching_files(run_args.bids_dir, subject=subject, dtype="anat", suffixes=[suffix] if suffix else None, session=session, run=run, part="mag", acq=acq, rec=rec, inv=inv)[:run_args.num_echoes]
    phase_params_files = [path.replace('.nii.gz', '.nii').replace('.nii', '.json') for path in phase_files]
    mag_params_files = [path.replace('.nii.gz', '.nii').replace('.nii', '.json') for path in magnitude_files]
    params_files = phase_params_files if len(phase_params_files) else mag_params_files
    mask_files = [
        mask_file for mask_file in get_matching_files(os.path.join(run_args.bids_dir, "derivatives", run_args.existing_masks_pipeline),
                                                      subject=subject, dtype="anat", suffixes=["mask"], session=session, run=None, part=None, acq=acq, rec=rec, inv=inv)[:run_args.num_echoes]
        if ('_space-orig' in mask_file or '_space-' not in mask_file)
           and ('_label-brain' in mask_file or '_label-' not in mask_file)
           and ('qsmxt-workflow' not in mask_file)
    ]
    qsm_files = [
        qsm_file for qsm_file in get_matching_files(os.path.join(run_args.bids_dir, "derivatives", run_args.existing_qsm_pipeline),
                                                    subject=subject, dtype="anat", suffixes=["Chimap"], session=session, run=None, part=None, acq=acq, rec=rec, inv=inv)
        if ('qsmxt-workflow' not in qsm_file)
    ]
    seg_files = [
        seg_file for seg_file in get_matching_files(os.path.join(run_args.bids_dir, "derivatives", run_args.existing_segmentation_pipeline),
                                                    subject=subject, dtype="anat", suffixes=["dseg"], session=session, space="qsm", run=None, part=None, acq=acq, rec=rec, inv=inv)
        if ('qsmxt-workflow' not in seg_file)
    ]

    # handle any errors related to files and adjust any settings if needed
    if run_args.do_segmentation and not t1w_files:
        logger.log(LogLevel.WARNING.value, f"{run_id}: Skipping segmentation - no T1w files found!")
        run_args.do_segmentation = False
    if run_args.do_segmentation and not magnitude_files:
        logger.log(LogLevel.WARNING.value, f"{run_id}: Skipping segmentation - no GRE magnitude files found to register T1w segmentations to!")
        run_args.do_segmentation = False
    if run_args.do_segmentation and len(t1w_files) > 1:
        logger.log(LogLevel.WARNING.value, f"{run_id}: Using {t1w_files[0]} for segmentation - multiple T1w files found!")
    if run_args.do_qsm and not phase_files:
        # Only warn if this isn't a T1w-only acquisition
        if suffix != "T1w":
            logger.log(LogLevel.WARNING.value, f"{run_id}: Skipping QSM - No phase files found!")
        run_args.do_qsm = False
        run_args.do_swi = False
    if len(phase_files) != len(phase_params_files) and any([run_args.do_qsm, run_args.do_swi]):
        logger.log(LogLevel.ERROR.value, f"{run_id}: An unequal number of JSON and phase files are present - QSM and SWI are not possible!")
        run_args.do_qsm = False
        run_args.do_swi = False
    if len(magnitude_files) != len(mag_params_files) and any([run_args.do_swi, run_args.do_r2starmap, run_args.do_t2starmap, run_args.do_segmentation]):
        logger.log(LogLevel.ERROR.value, f"{run_id}: An unequal number of JSON and magnitude files are present - SWI, R2* mapping, T2* mapping, and segmentation are not possible!")
        run_args.do_swi = False
        run_args.do_r2starmap = False
        run_args.do_t2starmap = False
        run_args.do_segmentation = False
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
        mag_dims = [nib.load(magnitude_files[i]).header['dim'][1:4].tolist() for i in range(len(magnitude_files))]
        phs_dims = [nib.load(magnitude_files[i]).header['dim'][1:4].tolist() for i in range(len(phase_files))]
        if not all(x == phs_dims[0] for x in mag_dims):
            logger.log(LogLevel.ERROR.value, f"{run_id}: Cannot use magnitude for masking - dimensions of magnitude files are not all equal to phase files! magnitude={mag_dims}; phase={phs_dims}.")
            run_args.masking_input = 'phase'
            run_args.inhomogeneity_correction = False
            run_args.add_bet = False
        if run_args.use_existing_masks:
            mask_dims = [nib.load(mask_files[i]).header['dim'][1:4].tolist() for i in range(len(mask_files))]
            phs_dims = [nib.load(magnitude_files[i]).header['dim'][1:4].tolist() for i in range(len(phase_files))]
            if not all(x == phs_dims[0] for x in mask_dims):
                logger.log(LogLevel.ERROR.value, f"{run_id}: Cannot use existing masks - mask dimensions are not all equal to phase files! mask={mask_dims}; phase={phs_dims}.")
                run_args.use_existing_masks = False
    elif run_args.do_r2starmap or run_args.do_t2starmap:
        mag_dims = [nib.load(magnitude_files[i]).header['dim'][1:4].tolist() for i in range(len(magnitude_files))]
        if not all(x == mag_dims[0] for x in mag_dims):
            logger.log(LogLevel.ERROR.value, f"{run_id}: Cannot do T2*/R2* mapping - magnitude dimensions are not all equal! magnitude={mag_dims}.")
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
    if run_args.do_analysis and not (qsm_files or run_args.do_qsm):
        # Only warn if this isn't a T1w-only acquisition
        if suffix != "T1w":
            logger.log(LogLevel.WARNING.value, f"{run_id}: Skipping analysis - no QSM files found or --do_qsm not selected!")
        run_args.do_analysis = False
    if run_args.do_analysis and not (seg_files or run_args.do_segmentation):
        logger.log(LogLevel.WARNING.value, f"{run_id}: Skipping analysis - no segmentations found or --do_segmentation not selected!")
        run_args.do_analysis = False

    # ensure that all input files dimensions match and are 3d
    if phase_files and any([run_args.do_qsm, run_args.do_swi]):
        dims = nib.load(phase_files[0]).header.get_data_shape()
        if len(dims) != 3 or any(dim == 1 for dim in dims[:3]):
            logger.log(LogLevel.ERROR.value, f"{run_id}: Input dimensions must be 3D! Got {phase_files[0]}={dims}.")
        for i in range(1, len(phase_files)):
            dims_i = nib.load(phase_files[i]).header.get_data_shape()
            if dims != dims_i:
                logger.log(LogLevel.ERROR.value, f"{run_id}: Incompatible dimensions detected! {phase_files[0]}={dims}; {phase_files[i]}={dims}.")
        
        if any([run_args.do_t2starmap, run_args.do_r2starmap, run_args.do_segmentation, run_args.masking_input == 'magnitude', run_args.inhomogeneity_correction, run_args.add_bet]):
            if not run_args.do_qsm:
                dims = nib.load(magnitude_files[0]).header.get_data_shape()
            if len(dims) != 3 or any(dim == 1 for dim in dims[:3]):
                logger.log(LogLevel.ERROR.value, f"{run_id}: Input dimensions must be 3D! Got {magnitude_files[0]}={dims}.")
            for i in range(len(magnitude_files)):
                dims_i = nib.load(magnitude_files[i]).header.get_data_shape()
                if dims != dims_i:
                    logger.log(LogLevel.ERROR.value, f"{run_id}: Incompatible dimensions detected! {phase_files[0] if run_args.do_qsm else magnitude_files[0]}={dims}; {magnitude_files[i]}={dims}.")
        
        if run_args.do_qsm and run_args.use_existing_masks and mask_files:
            dims = nib.load(phase_files[0]).header.get_data_shape()
            for i in range(len(mask_files)):
                dims_i = nib.load(mask_files[i]).header.get_data_shape()
                if dims != dims_i:
                    logger.log(LogLevel.ERROR.value, f"{run_id}: Incompatible dimensions detected! {phase_files[0]}={dims}; {mask_files[i]}={dims}.")
    
    def calculate_memory_usage(nifti_file):
        nifti = nib.load(nifti_file)
        header = nifti.header
        dimensions = header.get_data_shape()
        num_voxels = np.prod(dimensions)
        bytepix = header.get_data_dtype().itemsize
        memory_usage_bytes = num_voxels * bytepix
        mem_gb = memory_usage_bytes / (1024 ** 3)
        return mem_gb, dimensions, bytepix
    
    if phase_files:
        mem_phase, dimensions_phase, bytepix_phase = calculate_memory_usage(phase_files[0])
        mem_phase_64 = np.prod(dimensions_phase) * 8 / (1024 ** 3)
        logger.log(LogLevel.DEBUG.value, f"GRE phase files are {dimensions_phase} * {len(phase_files)} echoes * {bytepix_phase} bytes/voxel == {round(mem_phase * len(phase_files), 3)} GB ({round(mem_phase_64 * len(phase_files), 3)} GB at 64-bit)")
    if magnitude_files:
        mem_mag, dimensions_mag, bytepix_mag = calculate_memory_usage(magnitude_files[0])
        mem_mag_64 = np.prod(dimensions_mag) * 8 / (1024 ** 3)
        logger.log(LogLevel.DEBUG.value, f"GRE magnitude files are {dimensions_mag} * {len(magnitude_files)} echoes * {bytepix_mag} bytes/voxel == {round(mem_mag * len(magnitude_files), 3)} GB ({round(mem_mag_64 * len(magnitude_files), 3)} GB at 64-bit)")
    if t1w_files:
        mem_t1w, dimensions_t1w, bytepix_t1w = calculate_memory_usage(t1w_files[0])
        mem_t1w_64 = np.prod(dimensions_t1w) * 8 / (1024 ** 3)
        logger.log(LogLevel.DEBUG.value, f"T1w files are {dimensions_t1w} * {bytepix_t1w} bytes/voxel == {round(mem_t1w, 3)} GB ({round(mem_t1w_64, 3)} GB at 64-bit)")
    if mask_files and run_args.use_existing_masks:
        mem_mask, dimensions_mask, bytepix_mask = calculate_memory_usage(mask_files[0])
        mem_mask_64 = np.prod(dimensions_mask) * 8 / (1024 ** 3)
        logger.log(LogLevel.DEBUG.value, f"Mask files are {dimensions_mask} * {bytepix_mask} bytes/voxel == {round(mem_mask, 3)} GB ({round(mem_mask_64 * len(mask_files), 3)} GB at 64-bit)")

    if not any([run_args.do_qsm, run_args.do_swi, run_args.do_t2starmap, run_args.do_r2starmap, run_args.do_segmentation, run_args.do_analysis]):
        return
    
    # create nipype workflow for this run
    wf = Workflow(
        f"qsm" + (f"_acq-{acq}" if acq else "") + (f"_rec-{rec}" if rec else "") + (f"_{suffix}" if suffix else "") + (f"_inv-{inv}" if inv else "") + (f"_run-{run}" if run else ""),
        base_dir=os.path.join(run_args.workflow_dir,
                              os.path.join(subject, session) if session else subject,
                              acq or "", rec or "", suffix or "", inv or "", run or "")
    )

    # inputs and outputs
    n_inputs = create_node(
        IdentityInterface(fields=['phase', 'magnitude', 'params_files', 'mask']),
        name='nipype_getfiles'
    )
    n_inputs.inputs.phase = phase_files[0] if len(phase_files) == 1 else phase_files
    n_inputs.inputs.magnitude = magnitude_files[0] if len(magnitude_files) == 1 else magnitude_files
    n_inputs.inputs.params_files = params_files[0] if len(params_files) == 1 else params_files
    if not run_args.combine_phase and len(phase_files) > 1 and len(mask_files) == 1:
        mask_files = [mask_files[0] for _ in phase_files]
        n_inputs.inputs.mask = mask_files
    else:
        n_inputs.inputs.mask = mask_files[0] if len(mask_files) == 1 else mask_files

    n_outputs = create_node(
        IdentityInterface(fields=['qsm', 'qsm_singlepass', 'qsm_json', 'qsm_singlepass_json', 'swi', 'swi_mip', 't2s', 'r2s', 't1w_segmentation', 'qsm_segmentation', 'transform', 'analysis_csv', 'qsm_dicoms', 'swi_dicoms', 'swi_mip_dicoms']),
        name='qsmxt_outputs'
    )
    n_copyfile = Node(copyfile.DynamicCopyFiles(infields=[
        'qsm', 'qsm_singlepass', 'qsm_json', 'qsm_singlepass_json', 'swi', 'swi_mip', 't2s', 'r2s', 't1w_segmentation', 'qsm_segmentation', 'transform', 'analysis_csv', 'qsm_dicoms', 'swi_dicoms', 'swi_mip_dicoms'
    ]), name="copyfile")
    
    basedir = os.path.join(run_args.output_dir, subject, session if session else '')
    # Build basename for outputs.
    basename = f"{subject}"
    if session:
        basename += f"_{session}"
    if acq:
        basename += f"_acq-{acq}"
    if rec:
        basename += f"_rec-{rec}"
    if suffix:
        basename += f"_{suffix}"
    if inv:
        basename += f"_inv-{inv}"
    if run:
        basename += f"_run-{run}"
    
    n_copyfile.inputs.output_map = {
        'qsm': os.path.join(basedir, 'anat', f"{basename}_Chimap"),
        'qsm_singlepass': os.path.join(basedir, 'anat', f"{basename}_desc-singlepass_Chimap"),
        'qsm_json': os.path.join(basedir, 'anat', f"{basename}_Chimap"),
        'qsm_singlepass_json': os.path.join(basedir, 'anat', f"{basename}_desc-singlepass_Chimap"),
        'swi': os.path.join(basedir, 'anat', f"{basename}_swi"),
        'swi_mip': os.path.join(basedir, 'anat', f"{basename}_minIP"),
        't2s': os.path.join(basedir, 'anat', f"{basename}_T2starmap"),
        'r2s': os.path.join(basedir, 'anat', f"{basename}_R2starmap"),
        't1w_segmentation': os.path.join(basedir, 'anat', f"{basename}_space-orig_dseg"),
        'qsm_segmentation': os.path.join(basedir, 'anat', f"{basename}_space-qsm_dseg"),
        'transform': os.path.join(basedir, 'extra_data', f"{basename}_desc-t1w-to-qsm_transform"),
        'analysis_csv': os.path.join(basedir, 'extra_data', f"{basename}_qsm-analysis"),
        'qsm_dicoms': os.path.join(basedir, 'extra_data', f"{basename}_desc-dicoms_Chimap"),
        'swi_dicoms': os.path.join(basedir, 'extra_data', f"{basename}_desc-dicoms_swi"),
        'swi_mip_dicoms': os.path.join(basedir, 'extra_data', f"{basename}_desc-dicoms_minIP")
    }
    
    wf.connect([
        (n_outputs, n_copyfile, [
            ('qsm', 'qsm'),
            ('qsm_singlepass', 'qsm_singlepass'),
            ('qsm_json', 'qsm_json'),
            ('qsm_singlepass_json', 'qsm_singlepass_json'),
            ('swi', 'swi'),
            ('swi_mip', 'swi_mip'),
            ('t2s', 't2s'),
            ('r2s', 'r2s'),
            ('t1w_segmentation', 't1w_segmentation'),
            ('qsm_segmentation', 'qsm_segmentation'),
            ('transform', 'transform'),
            ('analysis_csv', 'analysis_csv'),
            ('qsm_dicoms', 'qsm_dicoms'),
            ('swi_dicoms', 'swi_dicoms'),
            ('swi_mip_dicoms', 'swi_mip_dicoms')
        ])
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
    n_json_params = create_node(
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
    n_nii_params = create_node(
        interface=Function(
            input_names=['nii_file'],
            output_names=['vsz'],
            function=read_nii
        ),
        name='nibabel_read-nii'
    )
    wf.connect([
        (n_inputs, n_nii_params, [('phase' if phase_files else 'magnitude', 'nii_file')])
    ])

    # reorient to canonical
    def as_closest_canonical(phase=None, magnitude=None, mask=None):
        import os
        import nibabel as nib
        from qsmxt.scripts.qsmxt_functions import extend_fname

        assert(phase or magnitude or mask)

        def as_closest_canonical_i(in_file):
            if nib.aff2axcodes(nib.load(in_file).affine) == ('R', 'A', 'S'):
                return in_file
            else:
                out_file = extend_fname(in_file, "_canonical", out_dir=os.getcwd())
                nib.save(nib.as_closest_canonical(nib.load(in_file)), out_file)
                return out_file
        
        out_phase = None
        out_mag = None
        out_mask = None

        if phase:
            if isinstance(phase, list):
                out_phase = [as_closest_canonical_i(phase_i) for phase_i in phase]
            else:
                out_phase = as_closest_canonical_i(phase)
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
    mn_inputs_canonical = create_node(
        interface=Function(
            input_names=[] + (['phase'] if (phase_files and (run_args.do_swi or run_args.do_qsm)) else []) + (['magnitude'] if magnitude_files else []) + (['mask'] if mask_files and run_args.use_existing_masks else []),
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
    if phase_files and (run_args.do_swi or run_args.do_qsm):
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
        n_t2s_r2s_mem = mem_mag_64 * (len(magnitude_files) + 2)
        n_t2s_r2s = create_node(
            interface=t2s_r2s.T2sR2sInterface(),
            mem_gb=n_t2s_r2s_mem,
            name='mrt_t2s-r2s'
        )
        n_t2s_r2s.plugin_args = gen_plugin_args(
            plugin_args={ 'overwrite': True },
            slurm_account=run_args.slurm[0],
            pbs_account=run_args.pbs,
            slurm_partition=run_args.slurm[1],
            name="t2s-r2s",
            time="01:00:00",
            mem_gb=n_t2s_r2s_mem
        )
        wf.connect([
            (mn_inputs_canonical, n_t2s_r2s, [('magnitude', 'magnitude')]),
            (mn_json_params, n_t2s_r2s, [('TE', 'TE')])
        ])
        if run_args.do_t2starmap: wf.connect([(n_t2s_r2s, n_outputs, [('t2starmap', 't2s')])])
        if run_args.do_r2starmap: wf.connect([(n_t2s_r2s, n_outputs, [('r2starmap', 'r2s')])])
    
    # swi
    if run_args.do_swi:
        n_swi_mem = mem_mag_64 * (len(magnitude_files) + 2) + mem_phase_64 * len(phase_files)
        n_swi_threads = min(6, run_args.n_procs) if run_args.multiproc else 6
        n_swi = create_node(
            interface=swi.ClearSwiInterface(
                num_threads=n_swi_threads
            ),
            name='mrt_clearswi',
            mem_gb=n_swi_mem,
            n_procs=n_swi_threads
        )
        n_swi.plugin_args = gen_plugin_args(
            plugin_args={ 'overwrite': True },
            slurm_account=run_args.slurm[0],
            pbs_account=run_args.pbs,
            slurm_partition=run_args.slurm[1],
            name="SWI",
            mem_gb=n_swi_mem,
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
        n_registration_threads = min(6, run_args.n_procs) if run_args.multiproc else 6
        n_registration_mem = 8
        n_registration = create_node(
            interface=RegistrationSynQuick(
                num_threads=n_registration_threads,
                fixed_image=magnitude_files[0],
                moving_image=t1w_files[0],
                output_prefix=f"{subject}_{session}" + (f"_acq-{acq}" if acq else "") + (f"_rec-{rec}" if rec else "") + (f"_{suffix}" if suffix else "") + (f"_inv-{inv}" if inv else "") + (f"_run-{run}" if run else "") + "_"
            ),
            name='ants_register-t1-to-qsm',
            n_procs=n_registration_threads,
            mem_gb=n_registration_mem
        )
        n_registration.plugin_args = gen_plugin_args(
            plugin_args={ 'overwrite': True },
            slurm_account=run_args.slurm[0],
            pbs_account=run_args.pbs,
            slurm_partition=run_args.slurm[1],
            name="ANTS",
            mem_gb=n_registration_mem,
            num_cpus=n_registration_threads
        )

        # segment t1
        n_fastsurfer_threads = min(8, run_args.n_procs) if run_args.multiproc else 8
        n_fastsurfer_mem = 12
        n_fastsurfer = create_node(
            interface=fastsurfer.FastSurferInterface(
                in_file=t1w_files[0],
                num_threads=n_fastsurfer_threads
            ),
            name='fastsurfer_segment-t1',
            n_procs=n_fastsurfer_threads,
            mem_gb=n_fastsurfer_mem
        )
        n_fastsurfer.plugin_args = gen_plugin_args(
            plugin_args={ 'overwrite': True },
            slurm_account=run_args.slurm[0],
            pbs_account=run_args.pbs,
            slurm_partition=run_args.slurm[1],
            name="FASTSURFER",
            mem_gb=n_fastsurfer_mem,
            num_cpus=n_fastsurfer_threads
        )

        # convert segmentation to nii
        n_fastsurfer_aseg_nii = create_node(
            interface=mgz2nii.Mgz2NiiInterface(),
            name='numpy_numpy_nibabel_mgz2nii'
        )
        wf.connect([
            (n_fastsurfer, n_fastsurfer_aseg_nii, [('out_file', 'in_file')])
        ])

        # resample segmentation in T1w space
        n_fastsurfer_aseg_nii_resampled = create_node(
            interface=resample_like.ResampleLikeInterface(
                ref_file=t1w_files[0],
                interpolation='nearest'
            ),
            name='nibabel_numpy_nilearn_t1w-seg-resampled'
        )
        wf.connect([
            (n_fastsurfer_aseg_nii, n_fastsurfer_aseg_nii_resampled, [('out_file', 'in_file')])
        ])

        # get first canonical magnitude
        n_getfirst_canonical_magnitude = create_node(
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
        n_transform_segmentation = create_node(
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
            (n_fastsurfer_aseg_nii_resampled, n_outputs, [('out_file', 't1w_segmentation')]),
            (n_transform_segmentation, n_outputs, [('output_image', 'qsm_segmentation')]),
            (n_registration, n_outputs, [('out_matrix', 'transform')])
        ])

    if run_args.do_qsm:
        
        # resample to axial
        n_inputs_resampled = create_node(
            interface=IdentityInterface(
                fields=['phase', 'magnitude', 'mask']
            ),
            name='nipype_inputs-resampled'
        )
        if magnitude_files:
            mn_resample_mem = (np.prod(dimensions_phase) * bytepix_phase) / (1024 ** 3) * 2 * 16
            mn_resample_inputs = create_node(
                interface=sampling.AxialSamplingInterface(
                    obliquity_threshold=999 if run_args.obliquity_threshold == -1 else run_args.obliquity_threshold
                ),
                iterfield=['magnitude', 'phase'],
                mem_gb=mn_resample_mem,
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
                mem_gb=mn_resample_mem
            )
            wf.connect([
                (mn_inputs_canonical, mn_resample_inputs, [('magnitude', 'magnitude')]),
                (mn_phase_scaled, mn_resample_inputs, [('phase', 'phase')]),
                (mn_resample_inputs, n_inputs_resampled, [('magnitude', 'magnitude')]),
                (mn_resample_inputs, n_inputs_resampled, [('phase', 'phase')])
            ])
            if mask_files and run_args.use_existing_masks:
                mn_resample_mask_mem = (np.prod(dimensions_phase) * bytepix_phase) / (1024 ** 3) * 16
                mn_resample_mask = create_node(
                    interface=sampling.AxialSamplingInterface(
                        obliquity_threshold=999 if run_args.obliquity_threshold == -1 else run_args.obliquity_threshold
                    ),
                    iterfield=['mask'],
                    mem_gb=mn_resample_mask_mem,
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
                    mem_gb=mn_resample_mask_mem,
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
        n_inputs_combine = create_node(
            interface=IdentityInterface(
                fields=['phase_unwrapped', 'frequency']
            ),
            name='phase-combined'
        )
        if run_args.combine_phase:
            n_romeo_mem = (54.0860 * (np.prod(dimensions_phase) * 8 / (1024**3)) + 3.9819) # DONE
            n_romeo_combine = create_node(
                interface=romeo.RomeoB0Interface(),
                name='mrt_romeo_combine-phase',
                mem_gb=n_romeo_mem
            )
            n_romeo_combine.plugin_args = gen_plugin_args(
                plugin_args={ 'overwrite': True },
                slurm_account=run_args.slurm[0],
                pbs_account=run_args.pbs,
                slurm_partition=run_args.slurm[1],
                name="romeo_combine",
                time="00:10:00",
                mem_gb=n_romeo_mem,
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
            dimensions_phase=dimensions_phase,
            bytepix_phase=bytepix_phase,
            num_echoes=len(magnitude_files),
            index=0
        )
        wf.connect([
            (n_inputs_resampled, wf_masking, [('phase', 'masking_inputs.phase')]),
            (n_inputs_resampled, wf_masking, [('magnitude', 'masking_inputs.magnitude')]),
            (n_inputs_resampled, wf_masking, [('mask', 'masking_inputs.mask')]),
            (mn_json_params, wf_masking, [('TE', 'masking_inputs.TE')])
        ])

        # === QSM ===
        wf_qsm = qsm_workflow(run_args, "qsm", len(magnitude_files) > 0, len(phase_files) > 1 and not run_args.combine_phase, dimensions_phase, bytepix_phase, qsm_erosions=run_args.tgv_erosions)

        wf.connect([
            (n_inputs_resampled, wf_qsm, [('phase', 'qsm_inputs.phase')]),
            (n_inputs_combine, wf_qsm, [('phase_unwrapped', 'qsm_inputs.phase_unwrapped')]),
            (n_inputs_combine, wf_qsm, [('frequency', 'qsm_inputs.frequency')]),
            (n_inputs_resampled, wf_qsm, [('magnitude', 'qsm_inputs.magnitude')]),
            (wf_masking, wf_qsm, [('masking_outputs.mask', 'qsm_inputs.mask')]),
            (mn_json_params, wf_qsm, [('TE', 'qsm_inputs.TE')]),
            (n_json_params, wf_qsm, [('B0', 'qsm_inputs.B0')]),
            (n_nii_params, wf_qsm, [('vsz', 'qsm_inputs.vsz')]),
        ])
        wf_qsm.get_node('qsm_inputs').inputs.b0_direction = [0, 0, 1]
        
        n_qsm_average = create_node(
            interface=nonzeroaverage.NonzeroAverageInterface(),
            name="nibabel_numpy_qsm-average"
        )
        wf.connect([
            (wf_qsm, n_qsm_average, [('qsm_outputs.qsm', 'in_files')]),
            (wf_masking, n_qsm_average, [('masking_outputs.mask', 'in_masks')])
        ])

        n_resample_qsm = create_node(
            interface=resample_like.ResampleLikeInterface(),
            name='nibabel_numpy_nilearn_qsm-resampled'
        )
        wf.connect([
            (n_qsm_average, n_resample_qsm, [('out_file', 'in_file')]),
            (mn_inputs_canonical, n_resample_qsm, [('phase', 'ref_file')])
        ])

        # Create JSON sidecar for QSM output
        n_qsm_json_sidecar = create_node(
            interface=copy_json_sidecar.CopyJsonSidecarInterface(
                source_json=phase_params_files[0] if phase_params_files else params_files[0],
                additional_image_types=['QSM']
            ),
            name='copy_qsm_json_sidecar'
        )

        if run_args.qsm_reference:
            n_qsm_referenced = create_node(
                interface=qsm_referencing.ReferenceQSMInterface(
                    in_seg_values=run_args.qsm_reference if isinstance(run_args.qsm_reference, list) and run_args.do_segmentation else None
                ),
                name='nibabel_numpy_qsm-referenced'
            )
            wf.connect([
                (n_resample_qsm, n_qsm_referenced, [('out_file', 'in_qsm')]),
                (n_qsm_referenced, n_qsm_json_sidecar, [('out_file', 'target_nifti')]),
                (n_qsm_referenced, n_outputs, [('out_file', 'qsm' if not run_args.two_pass else 'qsm_singlepass')]),
                (n_qsm_json_sidecar, n_outputs, [('out_json', 'qsm_json' if not run_args.two_pass else 'qsm_singlepass_json')])
            ])
            if isinstance(run_args.qsm_reference, list) and run_args.do_segmentation:
                wf.connect([
                    (n_transform_segmentation, n_qsm_referenced, [('output_image', 'in_seg')])
                ])
        else:
            wf.connect([
                (n_resample_qsm, n_qsm_json_sidecar, [('out_file', 'target_nifti')]),
                (n_resample_qsm, n_outputs, [('out_file', 'qsm' if not run_args.two_pass else 'qsm_singlepass')]),
                (n_qsm_json_sidecar, n_outputs, [('out_json', 'qsm_json' if not run_args.two_pass else 'qsm_singlepass_json')])
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
                dimensions_phase=dimensions_phase,
                bytepix_phase=bytepix_phase,
                num_echoes=len(magnitude_files),
                index=1
            )
            wf.connect([
                (n_inputs_resampled, wf_masking_intermediate, [('phase', 'masking_inputs.phase')]),
                (n_inputs_resampled, wf_masking_intermediate, [('magnitude', 'masking_inputs.magnitude')]),
                (n_inputs_resampled, wf_masking_intermediate, [('mask', 'masking_inputs.mask')]),
                (mn_json_params, wf_masking_intermediate, [('TE', 'masking_inputs.TE')]),
                (wf_masking, wf_masking_intermediate, [('masking_outputs.quality_map', 'masking_inputs.quality_map')])
            ])

            wf_qsm_intermediate = qsm_workflow(run_args, "qsm-intermediate", len(magnitude_files) > 0, len(phase_files) > 1 and not run_args.combine_phase, dimensions_phase, bytepix_phase, qsm_erosions=0)
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
            n_qsm_twopass_average = create_node(
                interface=nonzeroaverage.NonzeroAverageInterface(),
                name="nibabel_numpy_twopass-average"
            )
            wf.connect([
                (mn_qsm_twopass, n_qsm_twopass_average, [('out_file', 'in_files')]),
                #(wf_masking, n_qsm_twopass_average, [('masking_outputs.mask', 'in_masks')])
            ])

            n_resample_qsm = create_node(
                interface=resample_like.ResampleLikeInterface(),
                name='nibabel_numpy_nilearn_twopass-qsm-resampled'
            )
            wf.connect([
                (n_qsm_twopass_average, n_resample_qsm, [('out_file', 'in_file')]),
                (mn_inputs_canonical, n_resample_qsm, [('phase', 'ref_file')])
            ])

            # Create JSON sidecar for two-pass QSM output
            n_qsm_twopass_json_sidecar = create_node(
                interface=copy_json_sidecar.CopyJsonSidecarInterface(
                    source_json=phase_params_files[0] if phase_params_files else params_files[0],
                    additional_image_types=['QSM']
                ),
                name='copy_qsm_twopass_json_sidecar'
            )

            if run_args.qsm_reference:
                n_qsm_twopass_referenced = create_node(
                    interface=qsm_referencing.ReferenceQSMInterface(
                        in_seg_values=run_args.qsm_reference if isinstance(run_args.qsm_reference, list) and run_args.do_segmentation else None
                    ),
                    name='nibabel_numpy_qsm-twopass-referenced'
                )
                wf.connect([
                    (n_resample_qsm, n_qsm_twopass_referenced, [('out_file', 'in_qsm')]),
                    (n_qsm_twopass_referenced, n_qsm_twopass_json_sidecar, [('out_file', 'target_nifti')]),
                    (n_qsm_twopass_referenced, n_outputs, [('out_file', 'qsm')]),
                    (n_qsm_twopass_json_sidecar, n_outputs, [('out_json', 'qsm_json')])
                ])
                if isinstance(run_args.qsm_reference, list) and run_args.do_segmentation:
                    wf.connect([
                        (n_transform_segmentation, n_qsm_twopass_referenced, [('output_image', 'in_seg')])
                    ])
            else:
                wf.connect([
                    (n_resample_qsm, n_qsm_twopass_json_sidecar, [('out_file', 'target_nifti')]),
                    (n_resample_qsm, n_outputs, [('out_file', 'qsm')]),
                    (n_qsm_twopass_json_sidecar, n_outputs, [('out_json', 'qsm_json')])
                ])

    if run_args.do_analysis:
        def combine_lists(list1=None, list2=None):
            if list1 is None:
                return list2
            if list2 is None:
                return list1
            if not isinstance(list1, list): list1 = [list1]
            if not isinstance(list2, list): list2 = [list2]
            return list1 + list2
        n_combine_qsm_files = create_node(
            interface=Function(input_names=['list1', 'list2'], output_names=['qsm_files'], function=combine_lists),
            name='combine_lists1'
        )
        n_combine_seg_files = create_node(
            interface=Function(input_names=['list1', 'list2'], output_names=['seg_files'], function=combine_lists),
            name='combine_lists2'
        )
        n_combine_qsm_files.inputs.list1 = qsm_files
        n_combine_seg_files.inputs.list1 = seg_files
        if run_args.do_qsm:
            wf.connect([
                (n_resample_qsm, n_combine_qsm_files, [('out_file', 'list2')]),
            ])
        if run_args.do_segmentation:
            wf.connect([
                (n_transform_segmentation, n_combine_seg_files, [('output_image', 'list2')]),
            ])

        def create_combinations(list1, list2):
            import itertools
            combinations = list(itertools.product(list1, list2))
            in_files, in_segmentations = zip(*combinations)
            return list(in_files), list(in_segmentations)
        n_create_permutations = create_node(
            interface=Function(input_names=['list1', 'list2'], output_names=['qsm_files', 'seg_files'], function=create_combinations),
            name='create_permutations'
        )
        wf.connect(n_combine_qsm_files, 'qsm_files', n_create_permutations, 'list1')
        wf.connect(n_combine_seg_files, 'seg_files', n_create_permutations, 'list2')


        n_analyse_qsm = create_node(
            interface=analyse.AnalyseInterface(
                in_labels=run_args.labels_file,
                in_pipeline_name=os.path.split(run_args.output_dir)[1]
            ),
            name='nibabel_numpy_analyse-qsm',
            iterfield=['in_file', 'in_segmentation'],
            is_map=True
        )
        wf.connect(n_create_permutations, 'qsm_files', n_analyse_qsm, 'in_file')
        wf.connect(n_create_permutations, 'seg_files', n_analyse_qsm, 'in_segmentation')
        wf.connect(n_analyse_qsm, 'out_csv', n_outputs, 'analysis_csv')


    # insert DICOM conversion step
    if run_args.export_dicoms:
        for target_attribute in ['qsm', 'swi', 'swi_mip']:
            # Get the NIfTI file node
            node, node_attribute = get_preceding_node_and_attribute(wf, target_node_name='qsmxt_outputs', target_attribute=target_attribute)
            if node:
                logger.log(LogLevel.DEBUG.value, f"Found node {node._name} for {target_attribute}")
                
                # Determine which JSON sidecar to use based on the target
                json_attribute = None
                if target_attribute == 'qsm':
                    # Use qsm_json or qsm_singlepass_json depending on two_pass setting
                    json_attribute = 'qsm_json' if run_args.two_pass else ('qsm_json' if run_args.do_qsm else None)
                elif target_attribute == 'swi' or target_attribute == 'swi_mip':
                    # SWI and SWI MIP share the same metadata, use the original phase JSON
                    json_attribute = None  # Will use phase_params_files[0] directly
                
                # Determine image type suffixes
                image_type_suffix = []
                series_desc_suffix = ""
                if 'qsm' in target_attribute:
                    image_type_suffix = ['QSM']
                    series_desc_suffix = "_QSM"
                elif target_attribute == 'swi':
                    image_type_suffix = ['SWI']
                    series_desc_suffix = "_SWI"
                elif target_attribute == 'swi_mip':
                    image_type_suffix = ['SWI', 'PROJECTION IMAGE']
                    series_desc_suffix = "_SWI_MIP"
                
                # Create reference DICOM generator node
                n_create_ref_dicom = create_node(
                    interface=create_reference_dicom.CreateReferenceDicomInterface(
                        subject_id=subject,
                        session_id=session if session else "",
                        image_type_suffix=image_type_suffix,
                        series_description_suffix=series_desc_suffix
                    ),
                    name=f'n_create_ref_dicom_{target_attribute}'
                )
                
                # Connect the appropriate JSON source
                if json_attribute:
                    # Get JSON from the outputs
                    json_node, json_node_attribute = get_preceding_node_and_attribute(
                        wf, target_node_name='qsmxt_outputs', target_attribute=json_attribute
                    )
                    if json_node:
                        wf.connect([
                            (json_node, n_create_ref_dicom, [(json_node_attribute, 'source_json')])
                        ])
                else:
                    # Use the original phase params file for SWI
                    n_create_ref_dicom.inputs.source_json = phase_params_files[0] if phase_params_files else params_files[0]
                
                # Create nii2dcm node with reference DICOM
                n_nii2dcm = create_node(
                    interface=nii2dcm.Nii2DcmInterface(
                        centered=True if 'qsm' in target_attribute and not run_args.preserve_float else False,
                        preserve_float=run_args.preserve_float if 'qsm' in target_attribute else False,
                    ),
                    name=f'n_nii2dcm_{target_attribute}'
                )
                
                # Connect nodes
                wf.connect([
                    (node, n_nii2dcm, [(node_attribute, 'in_file')]),
                    (n_create_ref_dicom, n_nii2dcm, [('reference_dicom', 'ref_dicom')]),
                    (n_nii2dcm, n_outputs, [('out_dir', f"{target_attribute}_dicoms")])
                ])
    
    return wf

def qsm_workflow(run_args, name, magnitude_available, use_maps, dimensions_phase, bytepix_phase, qsm_erosions=0):
    wf = Workflow(name=f"{name}_workflow")

    slurm_account = run_args.slurm[0] if run_args.slurm and len(run_args.slurm) else None
    slurm_partition = run_args.slurm[1] if run_args.slurm and len(run_args.slurm) > 1 else None

    n_inputs = create_node(
        interface=IdentityInterface(
            fields=['phase', 'phase_unwrapped', 'frequency', 'magnitude', 'mask', 'TE', 'B0', 'b0_direction', 'vsz']
        ),
        name='qsm_inputs'
    )

    n_outputs = create_node(
        interface=IdentityInterface(
            fields=['qsm', 'qsm_dicoms']
        ),
        name='qsm_outputs'
    )

    # === PHASE UNWRAPPING ===
    if run_args.unwrapping_algorithm:
        n_unwrapping = create_node(
            interface=IdentityInterface(
                fields=['phase_unwrapped']
            ),
            name='phase-unwrapping'
        )
        if run_args.unwrapping_algorithm == 'laplacian':
            laplacian_threads = min(2, run_args.n_procs) if run_args.multiproc else 2
            laplacian_mem = 16.32256 * (np.prod(dimensions_phase) * 8 / (1024 ** 3)) + 1.12836 # DONE
            mn_laplacian = create_node(
                is_map=use_maps,
                interface=laplacian.LaplacianInterface(),
                iterfield=['phase'],
                name='mrt_laplacian-unwrapping',
                mem_gb=laplacian_mem,
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
                mem_gb=laplacian_mem,
                num_cpus=laplacian_threads
            )
        if run_args.unwrapping_algorithm == 'romeo':
            if run_args.combine_phase:
                wf.connect([
                    (n_inputs, n_unwrapping, [('phase_unwrapped', 'phase_unwrapped')]),
                ])
            else:
                romeo_mem = 9.81512 * (np.prod(dimensions_phase) * 8 / (1024 ** 3)) + 1.75 # DONE
                mn_romeo = create_node(
                    interface=romeo.RomeoB0Interface(),
                    name='mrt_romeo',
                    mem_gb=romeo_mem
                )
                mn_romeo.plugin_args = gen_plugin_args(
                    plugin_args={ 'overwrite': True },
                    slurm_account=run_args.slurm[0],
                    pbs_account=run_args.pbs,
                    slurm_partition=run_args.slurm[1],
                    name="Romeo",
                    mem_gb=romeo_mem,
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
    n_phase_normalized = create_node(
        interface=IdentityInterface(
            fields=['phase_normalized']
        ),
        name='phase_normalized'
    )
    if run_args.qsm_algorithm in ['rts', 'tv', 'nextqsm'] and not run_args.combine_phase:
        normalize_phase_threads = min(2, run_args.n_procs) if run_args.multiproc else 2
        mn_normalize_phase = create_node(
            interface=processphase.PhaseToNormalizedInterface(
                scale_factor=1e6/(2*np.pi)
            ),
            name='nibabel-numpy_normalize-phase',
            iterfield=['phase', 'TE'],
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
            num_cpus=normalize_phase_threads
        )
    if run_args.qsm_algorithm in ['rts', 'tv', 'nextqsm'] and run_args.combine_phase:
        normalize_freq_threads = min(2, run_args.n_procs) if run_args.multiproc else 2
        mn_normalize_freq = create_node(
            interface=processphase.FreqToNormalizedInterface(
                scale_factor=1e6/(2*np.pi)
            ),
            name='nibabel-numpy_normalize-freq',
            iterfield=['frequency'],
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
            num_cpus=normalize_freq_threads
        )
    if run_args.qsm_algorithm in ['tgv', 'tgvjl'] and run_args.combine_phase:
        freq_to_phase_threads = min(2, run_args.n_procs) if run_args.multiproc else 2
        mn_freq_to_phase = create_node(
            interface=processphase.FreqToPhaseInterface(TE=0.005, wraps=True),
            name='nibabel-numpy_freq-to-phase',
            iterfield=['frequency'],
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
            vsharp_mem = (8.15059 * (np.prod(dimensions_phase) * 8 / (1024 ** 3)) + 1.0839) # DONE
            mn_vsharp = create_node(
                interface=qsmjl.VsharpInterface(num_threads=vsharp_threads),
                iterfield=['frequency', 'mask'],
                name='qsmjl_vsharp',
                n_procs=vsharp_threads,
                mem_gb=vsharp_mem,
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
                mem_gb=vsharp_mem,
                num_cpus=vsharp_threads
            )
        if run_args.bf_algorithm == 'pdf':
            pdf_threads = min(8, run_args.n_procs) if run_args.multiproc else 8
            pdf_mem = 9.23275 * (np.prod(dimensions_phase) * 8) / (1024 ** 3) + 1.46 # DONE
            mn_pdf = create_node(
                interface=qsmjl.PdfInterface(num_threads=pdf_threads),
                iterfield=['frequency', 'mask'],
                name='qsmjl_pdf',
                n_procs=pdf_threads,
                mem_gb=pdf_mem,
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
                mem_gb=pdf_mem,
                num_cpus=pdf_threads
            )

    # === DIPOLE INVERSION ===
    if run_args.qsm_algorithm == 'nextqsm':
        nextqsm_threads = min(8, run_args.n_procs) if run_args.multiproc else 8
        nextqsm_mem = 69.64824 * (np.prod(dimensions_phase) * 8 / (1024 ** 3)) + 5.6689 # DONE
        mn_qsm = create_node(
            interface=nextqsm.NextqsmInterface(),
            name='nextqsm',
            iterfield=['phase', 'mask'],
            mem_gb=nextqsm_mem,
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
            mem_gb=nextqsm_mem,
            num_cpus=nextqsm_threads
        )
    if run_args.qsm_algorithm == 'rts':
        rts_threads = min(2, run_args.n_procs) if run_args.multiproc else 2
        rts_mem = (18.19 * (np.prod(dimensions_phase) * 3 / (1024 ** 3)) + 2) # DONE
        mn_qsm = create_node(
            interface=qsmjl.RtsQsmInterface(
                num_threads=rts_threads,
                tol=getattr(run_args, 'rts_tol', None) or 1e-4,
                delta_threshold=getattr(run_args, 'rts_delta', None) or 0.15,
                mu_regularization=getattr(run_args, 'rts_mu', None) or 1e5
            ),
            name='qsmjl_rts',
            iterfield=['tissue_frequency', 'mask'],
            n_procs=rts_threads,
            mem_gb=rts_mem,
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
            mem_gb=rts_mem,
            num_cpus=rts_threads
        )
    if run_args.qsm_algorithm == 'tv':
        tv_threads = min(2, run_args.n_procs) if run_args.multiproc else 2
        tv_mem = 4.5365 * (np.prod(dimensions_phase) * 8 / (1024 ** 3)) + 2 # DONE
        mn_qsm = create_node(
            interface=qsmjl.TvQsmInterface(num_threads=tv_threads),
            name='qsmjl_tv',
            iterfield=['tissue_frequency', 'mask'],
            n_procs=tv_threads,
            mem_gb=tv_mem,
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
            mem_gb=tv_mem,
            num_cpus=tv_threads
        )

    if run_args.qsm_algorithm == 'tgv':
        tgv_threads = min(4, run_args.n_procs)
        tgv_mem = 50.9915 * (np.prod(dimensions_phase) * 8 / (1024**3)) + 1.2 # DONE
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

