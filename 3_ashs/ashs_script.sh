#!/bin/bash
#this is the ashs script for qsm28 dataset (or any in bids format with pp/mc scripts run)
#Thomas Shaw 11/4/18 
#run ashs after checking inputs exist
#run this on nlin, lin, and average for qsm study

subjName=$1
source ~/.bashrc
export ITK_GLOBAL_DEFAULT_NUMBER_OF_THREADS=4
export NSLOTS=4
module load freesurfer/6.0
module load singularity/2.4.2

ashs_singularity="singularity exec --bind $TMPDIR:/TMPDIR/ --pwd /TMPDIR/ /30days/$USER/ashs_20180427.simg"
bidsdir=/30days/$USER
ss="ses-01"
deriv=/$bidsdir/derivatives
t1wpp=$deriv/preprocessing/$subjName/${subjName}_${ss}_T1w_N4corrected_norm_brain_preproc.nii.gz
t2NLIN=/30days/$USER/derivatives/tse_mc/${subjName}/${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_norm_brain_preproc.nii.gz
t2LIN=$deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_LinMoCo_res-iso.3_N4corrected_norm_brain_preproc.nii.gz
t2AVE=$deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-mean_res-iso.3_N4corrected_norm_brain_preproc.nii.gz 
cp $deriv/tse_mc/$subjName/brain_TSE_nlin_mc_template0.nii.gz $t2NLIN
mkdir -p $deriv/ashs/${subjName}

cp $t1wpp $TMPDIR
cp $t2NLIN $TMPDIR
cp $t2LIN $TMPDIR
cp $t2AVE $TMPDIR
t1wpp=$TMPDIR/${subjName}_${ss}_T1w_N4corrected_norm_brain_preproc.nii.gz
t2NLIN=$TMPDIR/${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_norm_brain_preproc.nii.gz
t2LIN=$TMPDIR/${subjName}_${ss}_T2w_LinMoCo_res-iso.3_N4corrected_norm_brain_preproc.nii.gz
t2AVE=$TMPDIR/${subjName}_${ss}_T2w_run-mean_res-iso.3_N4corrected_norm_brain_preproc.nii.gz

mkdir -p $TMPDIR/ashs/${subjName}/1_nlin $TMPDIR/ashs/${subjName}/2_lin $TMPDIR/ashs/${subjName}/3_ave

echo "T1 : "${t1wpp}
echo "T2 : "${t2NLIN}
if [[ ! -e ${t1wpp} ]] ; then echo "$subjName t1 not found exiting">>$deriv/ashsErrorLog.txt 
    exit 1 
fi
if [[ ! -e $t2NLIN ]] ; then echo "$subjName t2 NON LINEAR not found exiting">>$deriv/ashsErrorLog.txt 
    exit 1
fi
if [[ ! -e $t2LIN ]] ; then echo "$subjName t2 LINEAR not found exiting">>$deriv/ashsErrorLog.txt 
    exit 1
fi
if [[ ! -e $t2AVE ]] ; then echo "$subjName t2 AVERAGE not found exiting">>$deriv/ashsErrorLog.txt 
    exit 1
fi
t1wpp=/TMPDIR/${subjName}_${ss}_T1w_N4corrected_norm_brain_preproc.nii.gz
t2NLIN=/TMPDIR/${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_norm_brain_preproc.nii.gz
t2LIN=/TMPDIR/${subjName}_${ss}_T2w_LinMoCo_res-iso.3_N4corrected_norm_brain_preproc.nii.gz
t2AVE=/TMPDIR/${subjName}_${ss}_T2w_run-mean_res-iso.3_N4corrected_norm_brain_preproc.nii.gz

$ashs_singularity /ashs-1.0.0/bin/ashs_main.sh -I $subjName -a /ashs_atlas_upennpmc_20170810 -g $t1wpp -f $t2NLIN -w /TMPDIR/ashs/${subjName}/1_nlin
$ashs_singularity /ashs-1.0.0/bin/ashs_main.sh -I $subjName -a /ashs_atlas_upennpmc_20170810 -g $t1wpp -f $t2LIN -w /TMPDIR/ashs/${subjName}/2_lin
$ashs_singularity /ashs-1.0.0/bin/ashs_main.sh -I $subjName -a /ashs_atlas_upennpmc_20170810 -g $t1wpp -f $t2AVE -w /TMPDIR/ashs/${subjName}/3_ave
cp -r $TMPDIR/ashs/$subjName $deriv/ashs/$subjName

