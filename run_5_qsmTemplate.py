#!/usr/bin/env python3
import os
import os.path
from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import SelectFiles, DataSink, DataGrabber
from nipype.pipeline.engine import Workflow, Node, MapNode
from nipype.interfaces.minc import Resample, BigAverage, VolSymm
from interfaces import nipype_interface_nii2mnc as nii2mnc
from interfaces import nipype_interface_mnc2nii as mnc2nii
from interfaces import nipype_interface_niiremoveheader as niiremoveheader
import argparse


def create_workflow(qsm_output_dir, magnitude_template_output_dir, qsm_template_output_dir, qsm_template_work_dir):

    wf = Workflow(name='workflow_qsm_template', base_dir=qsm_template_work_dir)

    n_datasource_qsm = Node(
        interface=DataGrabber(
            sort_filelist=True
        ),
        name='datasource_qsm'
    )
    n_datasource_qsm.inputs.base_directory = qsm_output_dir
    n_datasource_qsm.inputs.template = 'qsm_final/*/*.nii*'

    n_datasource_xfm = Node(
        interface=DataGrabber(
            sort_filelist=True
        ),
        name='datasource_xfm'
    )
    n_datasource_xfm.inputs.base_directory = magnitude_template_output_dir
    n_datasource_xfm.inputs.template = 'transformations/*/*.xfm'

    n_datasource_grid = Node(
        interface=DataGrabber(
            sort_filelist=True
        ),
        name='datasource_grid'
    )
    n_datasource_grid.inputs.base_directory = magnitude_template_output_dir
    n_datasource_grid.inputs.template = 'transformations/*/*0.mnc'

    n_datasource_template = Node(
        interface=DataGrabber(
            sort_filelist=True
        ),
        name='datasource_template'
    )
    n_datasource_template.inputs.base_directory = magnitude_template_output_dir
    n_datasource_template.inputs.template = 'template/*/*.nii*'

    # strip nifti header
    mn_niiremoveheader = MapNode(
        interface=niiremoveheader.NiiRemoveHeaderInterface(),
        name='subject_removeheader',
        iterfield=['in_file']
    )
    wf.connect([
        (n_datasource_qsm, mn_niiremoveheader, [('outfiles', 'in_file')])
    ])

    # convert subject QSM images to mnc
    mn_qsm_mnc = MapNode(
        interface=nii2mnc.Nii2MncInterface(),
        iterfield=['in_file'],
        name='subject_qsm_nii2mnc'
    )
    wf.connect([
        (mn_niiremoveheader, mn_qsm_mnc, [('out_file', 'in_file')])
    ])

    # convert magnitude template to mnc
    n_magnitude_template_mnc = Node(
        interface=nii2mnc.Nii2MncInterface(),
        name='magnitude_template_nii2mnc'
    )
    wf.connect([
        (n_datasource_template, n_magnitude_template_mnc, [('outfiles', 'in_file')])
    ])

    mn_resample = MapNode(
        interface=Resample(
            nearest_neighbour_interpolation=True
        ),
        name='resample',
        iterfield=['input_file', 'transformation', 'input_grid_files']
    )
    wf.connect(mn_qsm_mnc, 'out_file', mn_resample, 'input_file')
    wf.connect(n_datasource_xfm, 'outfiles', mn_resample, 'transformation')
    wf.connect(n_datasource_grid, 'outfiles', mn_resample, 'input_grid_files')
    wf.connect(n_magnitude_template_mnc, 'out_file', mn_resample, 'like')

    mn_resample_nii = MapNode(
        interface=mnc2nii.Mnc2NiiInterface(),
        name='resample_nii',
        iterfield=['in_file']
    )
    wf.connect(mn_resample, 'output_file', mn_resample_nii, 'in_file')

    n_bigaverage = Node(
        interface=BigAverage(
            output_float=True,
            robust=False
        ),
        name='bigaverage',
        iterfield=['input_files']
    )

    wf.connect(mn_resample, 'output_file', n_bigaverage, 'input_files')

    n_bigaverage_nii = Node(
        interface=mnc2nii.Mnc2NiiInterface(),
        name='bigaverage_nii'
    )
    wf.connect(n_bigaverage, 'output_file', n_bigaverage_nii, 'in_file')

    datasink = Node(
        interface=DataSink(
            base_directory=qsm_template_output_dir
            #container=out_dir
        ),
        name='datasink'
    )

    wf.connect([(n_bigaverage_nii, datasink, [('out_file', 'qsm_template')])])
    wf.connect([(mn_resample_nii, datasink, [('out_file', 'qsm_transformed')])])

    return wf


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="QSMxT qsmTemplate: QSM template builder. Produces a group template based on QSM results from " +
                    "multiple subjects. Requires an initial magnitude group template generated using " +
                    "./run_4_magnitudeTemplate.py.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        "qsm_output_dir",
        type=str,
        help="the qsm output directory produced by ./run_2_qsm.py"
    )

    parser.add_argument(
        "magnitude_template_output_dir",
        type=str,
        help='the magnitude template output directory produced by ./run_4_magnitudeTemplate.py'
    )

    parser.add_argument(
        "qsm_template_output_dir",
        type=str,
        help='the intended output directory for the qsm group template'
    )

    parser.add_argument(
        '--work_dir',
        default=None,
        help='nipype working directory; defaults to \'work\' within \'out_dir\''
    )

    parser.add_argument(
        '--pbs',
        default=None,
        dest='qsub_account_string',
        help='run the pipeline via PBS and use the argument as the QSUB account string'
    )

    args = parser.parse_args()

    if not args.work_dir: args.work_dir = args.qsm_template_output_dir
    os.makedirs(os.path.abspath(args.qsm_template_output_dir), exist_ok=True)
    os.makedirs(os.path.abspath(args.work_dir), exist_ok=True)

    os.environ["PATH"] += os.pathsep + os.path.join(os.path.dirname(os.path.abspath(__file__)), "scripts")

    wf = create_workflow(
        qsm_output_dir=os.path.abspath(args.qsm_output_dir),
        magnitude_template_output_dir=os.path.abspath(args.magnitude_template_output_dir),
        qsm_template_output_dir=os.path.abspath(args.qsm_template_output_dir),
        qsm_template_work_dir=os.path.abspath(args.work_dir)
    )

    if args.qsub_account_string:
        wf.run(
            plugin='PBSGraph',
            plugin_args={
                'qsub_args': f'-A {qsub_account_string} -q Short -l nodes=1:ppn=1,mem=5GB,vmem=5GB,walltime=00:30:00'
            }
        )
    else:
        wf.run(
            plugin='MultiProc',
            plugin_args={
                'n_procs': int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count())
            }
        )
