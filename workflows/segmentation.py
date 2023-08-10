import glob
import os

from nipype.interfaces.io import DataSink
from nipype.pipeline.engine import Workflow, Node

from nipype.interfaces.ants.registration import RegistrationSynQuick
from nipype.interfaces.ants.resampling import ApplyTransforms

from interfaces import nipype_interface_fastsurfer as fastsurfer
from interfaces import nipype_interface_mgz2nii as mgz2nii

from scripts.logger import LogLevel, make_logger

def get_matching_files(bids_dir, subject, session, suffixes, part=None, acq=None, run=None):
    pattern = f"{bids_dir}/{subject}/{session}/anat/{subject}_{session}*"
    if acq:
        pattern += f"acq-{acq}_*"
    if run:
        pattern += f"run-{run}_*"
    if part:
        pattern += f"part-{part}_*"
    matching_files = [glob.glob(f"{pattern}{suffix}.nii*") for suffix in suffixes]
    return sorted([item for sublist in matching_files for item in sublist])

def init_segmentation_workflow(run_args, subject, session, acq=None, run=None):

    logger = make_logger('main')
    logger.log(LogLevel.INFO.value, f"Creating segmentation workflow for {subject}/{session}/acq-{acq}_run-{run}...")

    # get relevant files from this run
    t1w_files = get_matching_files(run_args.bids_dir, subject, session, ["T1w"], None, acq, run)
    mag_files = get_matching_files(run_args.bids_dir, subject, session, ["MEGRE", "T2starw"], "mag", None, None)
    
    if not t1w_files:
        logger.log(LogLevel.ERROR.value, f"No T1w files found for {subject}")
        return
    if not mag_files:
        logger.log(LogLevel.ERROR.value, f"No magnitude files matching pattern: {mag_files}")
        return
    if len(t1w_files) > 1:
        logger.log(LogLevel.WARNING.value, f"Multiple T1w files found! Using {t1w_files[0]}")
    
    t1w_file = t1w_files[0]
    mag_file = mag_files[0]

    wf = Workflow(f"segmentation" + (f"_acq-{acq}" if acq else "") + (f"_run-{run}" if run else ""), base_dir=os.path.join(run_args.output_dir, "workflow", subject, session, acq or "", run or ""))

    # register t1 to magnitude
    n_registration_threads = min(run_args.n_procs, 6) if run_args.multiproc else 6
    n_registration = Node(
        interface=RegistrationSynQuick(
            num_threads=n_registration_threads,
            fixed_image=mag_file,
            moving_image=t1w_file
        ),
        name='ants_register-t1-to-qsm',
        n_procs=n_registration_threads,
        mem_gb=min(run_args.mem_avail, 4)
    )
    
    # segment t1
    n_fastsurfer_threads = min(run_args.n_procs, 8) if run_args.multiproc else 8
    n_fastsurfer = Node(
        interface=fastsurfer.FastSurferInterface(
            in_file=t1w_file,
            num_threads=n_fastsurfer_threads
        ),
        name='fastsurfer_segment-t1',
        n_procs=n_fastsurfer_threads,
        mem_gb=min(run_args.mem_avail, 11)
    )
    n_fastsurfer.plugin_args = {
        'qsub_args': f'-A {run_args.pbs} -l walltime=03:00:00 -l select=1:ncpus={run_args.n_procs}:mem=20gb:vmem=20gb',
        'overwrite': True
    }

    # convert segmentation to nii
    n_fastsurfer_aseg_nii = Node(
        interface=mgz2nii.Mgz2NiiInterface(),
        name='numpy_numpy_nibabel_mgz2nii'
    )
    wf.connect([
        (n_fastsurfer, n_fastsurfer_aseg_nii, [('out_file', 'in_file')])
    ])

    # apply transforms to segmentation
    n_transform_segmentation = Node(
        interface=ApplyTransforms(
            dimension=3,
            reference_image=mag_file,
            interpolation="NearestNeighbor"
        ),
        name='ants_transform-segmentation-to-qsm'
    )
    wf.connect([
        (n_fastsurfer_aseg_nii, n_transform_segmentation, [('out_file', 'input_image')]),
        (n_registration, n_transform_segmentation, [('out_matrix', 'transforms')])
    ])

    n_outputs = Node(
        interface=DataSink(
            base_directory=run_args.output_dir
            #container=output_dir
        ),
        name='nipype_datasink'
    )
    wf.connect([
        (n_fastsurfer_aseg_nii, n_outputs, [('out_file', 'segmentations.t1w')]),
        (n_transform_segmentation, n_outputs, [('output_image', 'segmentations.qsm')])
    ])

    return wf

