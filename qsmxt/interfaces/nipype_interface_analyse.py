#!/usr/bin/env python3
import nibabel as nib
import numpy as np

from qsmxt.scripts.qsmxt_functions import extend_fname
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, File

# get labels dictionary by parsing a labels CSV file
def load_labels(label_filepath):
    # read label file
    with open(label_filepath, encoding='utf-8') as label_file:
        lines = label_file.readlines()

    # get all labels numbers and names
    label_nums = []
    label_names = []
    for line in lines:
        label_num, label_name = line.strip().split(',')
        label_nums.append(label_num)
        label_names.append(label_name)

    # fill labels dictionary -- e.g. { 'Accumbens' : [2, 13] }
    labels = {}
    for label_name in sorted(list(set(label_names))):
        labels[label_name] = []
        for i in range(len(label_names)):
            if label_names[i] == label_name:
                labels[label_name].append(int(label_nums[i]))
    
    return labels

# give names to segmentation labels that don't have one
def update_labels(labels, seg):
    for seg_num in sorted(list(set(seg.flatten()))):
        # get segmentation name 
        if seg_num == 0: continue
        seg_name = None
        for label_name in labels.keys():
            if seg_num in labels[label_name]:
                seg_name = label_name
                break
        if not seg_name: labels[str(int(seg_num))] = [int(seg_num)]

# get statistics for each label based on segmentations and qsm data
def get_stats(labels, seg, qsm):
    label_stats = {}
    for label_name in labels.keys():
        # get qsm values for this label
        qsm_seg = np.zeros_like(seg)
        for label_id in labels[label_name]:
            qsm_seg = np.logical_or(qsm_seg, seg == label_id)
        qsm_values = qsm[np.logical_and(qsm != 0, qsm_seg)]

        # skip if no values
        if len(qsm_values) == 0:
            label_stats[label_name] = []
            continue

        # get statistics and store them
        num_voxels = len(qsm_values)
        min_v = np.min(qsm_values)
        max_v = np.max(qsm_values)
        median = np.median(qsm_values)
        mean = np.mean(qsm_values)
        std  = np.std(qsm_values)
        label_stats[label_name] = [num_voxels, min_v, max_v, median, mean, std]
    return label_stats

def analyse(in_file, in_segmentation, out_csv, labels_file=None):
    labels = load_labels(labels_file) if labels_file else {}

    nii = nib.load(in_file)

    data = nii.get_fdata()
    seg = np.array(nib.load(in_segmentation).get_fdata(), dtype=int)

    update_labels(labels, seg)
    label_stats = get_stats(labels, seg, data)

    # write header to file
    with open(out_csv, 'w', encoding='utf-8') as f:
        f.write('roi,num_voxels,min,max,median,mean,std\n')

        # write data to file
        for label_name in labels.keys():
            line = [label_name]
            label_stats_i = label_stats[label_name]
            if len(label_stats_i) == 0:
                continue
            line.extend(label_stats_i)
            line = ",".join([str(x) for x in line])
            f.write(line)
            f.write('\n')

    return out_csv
    

class AnalyseInputSpec(BaseInterfaceInputSpec):
    in_file = File(mandatory=True, exists=True)
    in_segmentation = File(mandatory=True, exists=True)
    in_labels = File(mandatory=False, exists=True)


class AnalyseOutputSpec(TraitedSpec):
    out_csv = File(exists=True)


class AnalyseInterface(SimpleInterface):
    input_spec = AnalyseInputSpec
    output_spec = AnalyseOutputSpec

    def _run_interface(self, runtime):
        self._results['out_csv'] = analyse(
            in_file=self.inputs.in_file,
            in_segmentation=self.inputs.in_segmentation,
            out_csv=extend_fname(self.inputs.in_file, "_csv", ext='csv'),
            labels_file=self.inputs.in_labels
        )
        return runtime


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()

    parser.add_argument(
        'in_file',
        type=str
    )

    parser.add_argument(
        'in_segmentation',
        type=str
    )

    parser.add_argument(
        'out_csv',
        default=None,
        const=None,
        type=str
    )

    args = parser.parse_args()

    if not args.out_csv:
        args.out_csv = extend_fname(args.in_file, '_csv')

    analyse(args.in_file, args.in_segmentation, args.out_csv)

