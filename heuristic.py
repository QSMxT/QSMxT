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

    phase     = create_key('{bids_subject_session_dir}/anat/{bids_subject_session_prefix}_phase')
    magnitude = create_key('{bids_subject_session_dir}/anat/{bids_subject_session_prefix}_magnitude')

    info = { phase: [], magnitude: [] }
    #print(seqinfo)

    for s in seqinfo:
        if ('P' in s.image_type):
           info[phase] = [s.series_id]
        if ('M' and 'NORM' in s.image_type):
           info[magnitude] = [s.series_id]

    return info
