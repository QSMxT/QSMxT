import os

def create_key(template, outtype=('nii.gz'), annotation_classes=None):
    if template is None or not template:
        raise ValueError('Template must be a valid format string')
    return (template, outtype, annotation_classes)


def infotodict(seqinfo):
    """Heuristic evaluator for determining which runs belong where

    allowed template fields - follow python string module:

    item: index within category
    subject: participant id
    seqitem: run number during scanning
    subindex: sub index within group
    """

    greM = create_key('{bids_subject_session_dir}/anat/{bids_subject_session_prefix}_gre_M_echo_')
    greP = create_key('{bids_subject_session_dir}/anat/{bids_subject_session_prefix}_gre_P_echo_')

    info = {greM: [], greP: [],}

    #print(seqinfo)

    for s in seqinfo:
        if ('NORM' in s.image_type) and not ('ND' in s.image_type):
           info[greM] = [s.series_id]
        if not ('NORM' in s.image_type) and not ('ND' in s.image_type):
           info[greP] = [s.series_id]

    return info

