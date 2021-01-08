#!/usr/bin/env python3
import os
import os.path
from nipype.interfaces.utility import IdentityInterface, Function
from nipype.interfaces.io import SelectFiles, DataSink, DataGrabber
from nipype.pipeline.engine import Workflow, Node, MapNode
from nipype.interfaces.minc import Resample, BigAverage, VolSymm
import nipype_interface_nii2mnc as nii2mnc
import nipype_interface_mnc2nii as mnc2nii
import argparse


def create_workflow(qsm_dir, voliso_dir, xfm_dir, out_dir, work_dir, templates):

    wf = Workflow(name='qsm_template')
    wf.base_dir = os.path.join(work_dir)

    datasource_qsm = Node(
        interface=DataGrabber(
            sort_filelist=True
        ),
        name='datasource_qsm'
    )
    datasource_qsm.inputs.base_directory = qsm_dir
    datasource_qsm.inputs.template = '*.nii*'

    datasource_xfm = Node(
        interface=DataGrabber(
            sort_filelist=True
        ),
        name='datasource_xfm'
    )
    datasource_xfm.inputs.base_directory = xfm_dir
    datasource_xfm.inputs.template = '*/*.xfm'

    datasource_template = Node(
        interface=DataGrabber(
            sort_filelist=True
        ),
        name='datasource_template'
    )
    datasource_template.inputs.base_directory = voliso_dir
    datasource_template.inputs.template = '*.mnc'

    mn_qsm_mnc = MapNode(
        interface=nii2mnc.Nii2MncInterface(),
        iterfield=['in_file'],
        name='nii2mnc'
    )
    wf.connect([
        (datasource_qsm, mn_qsm_mnc, [('outfiles', 'in_file')])
    ])

    resample = MapNode(
        interface=Resample(
            nearest_neighbour_interpolation=True
        ),
        name='resample',
        iterfield=['input_file', 'transformation']
    )
    wf.connect(mn_qsm_mnc, 'out_file', resample, 'input_file')
    wf.connect(datasource_xfm, 'outfiles', resample, 'transformation')
    wf.connect(datasource_template, 'outfiles', resample, 'like')

    resample_nii = MapNode(
        interface=mnc2nii.Mnc2NiiInterface(),
        name='resample_nii',
        iterfield=['input_file']
    )
    wf.connect(resample, 'output_file', resample_nii, 'input_file')

    bigaverage = Node(
        interface=BigAverage(
            output_float=True,
            robust=False
        ),
        name='bigaverage',
        iterfield=['input_file']
    )

    wf.connect(resample, 'output_file', bigaverage, 'input_files')

    bigaverage_nii = Node(
        interface=mnc2nii.Mnc2NiiInterface(),
        name='bigaverage_nii'
    )
    wf.connect(bigaverage, 'output_file', bigaverage_nii, 'input_file')

    datasink = Node(
        interface=DataSink(
            base_directory=out_dir,
            container=out_dir
        ),
        name='datasink'
    )

    wf.connect([(bigaverage_nii, datasink, [('output_file', 'qsm_template')])])
    wf.connect([(resample_nii, datasink, [('output_file', 'qsm_transformed')])])
    wf.connect([(datasource_xfm, datasink, [('outfiles', 'xfm_transforms')])])

    return wf


if __name__ == "__main__":
    parser = argparse.ArgumentParser()

    parser.add_argument(
        "qsm_dir",
        type=str,
        help="qsm output directory"
    )

    parser.add_argument(
        "voliso_dir",
        type=str,
        help='voliso_dir'
    )

    parser.add_argument(
        "xfm_dir",
        type=str,
        help='xfm_dir'
    )

    parser.add_argument(
        "out_dir",
        type=str
    )

    parser.add_argument(
        "--work_dir",
        type=str,
        default=None
    )

    parser.add_argument(
        '--pbs',
        action='store_true',
        help='use PBS graph'
    )

    args = parser.parse_args()

    if not args.work_dir: args.work_dir = os.path.join(args.out_dir, "work")
    os.makedirs(os.path.abspath(args.work_dir), exist_ok=True)
    os.makedirs(os.path.abspath(args.out_dir), exist_ok=True)

    wf = create_workflow(
        qsm_dir=args.qsm_dir,
        out_dir=args.out_dir,
        voliso_dir=args.voliso_dir,
        xfm_dir=args.xfm_dir,
        work_dir=args.work_dir,
        templates=templates
    )

    if args.pbs:
        wf.run(
            plugin='PBSGraph',
            plugin_args={
                'qsub_args': '-A UQ-CAI -q Short -l nodes=1:ppn=1,mem=5GB,vmem=5GB,walltime=00:30:00'
            }
        )
    else:
        wf.run(
            plugin='MultiProc',
            plugin_args={
                'n_procs': int(os.environ["NCPUS"]) if "NCPUS" in os.environ else int(os.cpu_count())
            }
        )
