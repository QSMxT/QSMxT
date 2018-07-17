#!/bin/bash
set -e 
#fail early


subject=sub-S008LCBL

for echoM in `ls $subject/anat/*gre_M*.nii.gz`; do
        echo $echoM  
        bet $echoM ${echoM%.nii.gz}_bet -R -f 0.6 -m
done

fail


for echoNbr in {0..5}; do
        singularity \
        exec \
        --bind $PWD:/data \
        CAIsr-qsm-v1.2.3-latest.simg \
        tgv_qsm \
        -p /data/${subject}_gre6phase_split_000${echoNbr}.nii \
        -m /data/${subject}_gre6magni_split_000${echoNbr}_bet_mask.nii \
        -f 2.89 \
        -e 0 \
        -t ${echoTime[5]} \
        -s \
        -o tgvqsm
done
