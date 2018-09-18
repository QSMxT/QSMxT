#!/bin/bash
#BIDS lin realignment/MC script
# tom shaw
# 12/04/18
subjName=$1
singularity="singularity exec /30days/uqtshaw/fsl_ants_robex_20180312.simg"
echo $subjName
module load singularity/2.4.2
cd /30days/uqtshaw/derivatives/preprocessing/${subjName}/
bidsdir=/30days/uqtshaw
ss="ses-01"
deriv=/$bidsdir/derivatives
#merge
#brain
$singularity fslmerge -t $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_merged_res-iso.3_N4corrected_norm_brain_preproc.nii.gz $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-1_res-iso.3_N4corrected_norm_brain_preproc.nii.gz $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-2_res-iso.3_N4corrected_norm_brain_preproc.nii.gz $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-3_res-iso.3_N4corrected_norm_brain_preproc.nii.gz
if [[ ! -e $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_merged_res-iso.3_N4corrected_norm_brain_preproc.nii.gz ]] ; then
    $singularity flirt -v -in $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-3_res-iso.3_N4corrected_norm_brain_preproc.nii.gz -ref $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-1_res-iso.3_N4corrected_norm_brain_preproc.nii.gz -applyxfm -usesqform -out $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-3_res-iso.3_N4corrected_norm_brain_preproc.nii.gz
    $singularity flirt -v -in $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-2_res-iso.3_N4corrected_norm_brain_preproc.nii.gz -ref $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-1_res-iso.3_N4corrected_norm_brain_preproc.nii.gz -applyxfm -usesqform -out $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-2_res-iso.3_N4corrected_norm_brain_preproc.nii.gz
    $singularity fslmerge -t $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_merged_res-iso.3_N4corrected_norm_brain_preproc.nii.gz $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-1_res-iso.3_N4corrected_norm_brain_preproc.nii.gz $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-2_res-iso.3_N4corrected_norm_brain_preproc.nii.gz $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-3_res-iso.3_N4corrected_norm_brain_preproc.nii.gz
fi
#skull
$singularity fslmerge -t $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_merged_res-iso.3_N4corrected_norm_preproc.nii.gz $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-1_res-iso.3_N4corrected_norm_preproc.nii.gz $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-2_res-iso.3_N4corrected_norm_preproc.nii.gz $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-3_res-iso.3_N4corrected_norm_preproc.nii.gz
if [[ ! -e $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_merged_res-iso.3_N4corrected_norm_preproc.nii.gz ]] ; then
    $singularity flirt -in $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-3_res-iso.3_N4corrected_norm_preproc.nii.gz -ref $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-1_res-iso.3_N4corrected_norm_preproc.nii.gz -applyxfm -usesqform -out $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-3_res-iso.3_N4corrected_norm_preproc.nii.gz
    $singularity flirt -in $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-2_res-iso.3_N4corrected_norm_preproc.nii.gz -ref $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-1_res-iso.3_N4corrected_norm_preproc.nii.gz -applyxfm -usesqform -out $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-2_res-iso.3_N4corrected_norm_preproc.nii.gz
    $singularity fslmerge -t $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_merged_res-iso.3_N4corrected_norm_preproc.nii.gz $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-1_res-iso.3_N4corrected_norm_preproc.nii.gz $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-2_res-iso.3_N4corrected_norm_preproc.nii.gz $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-3_res-iso.3_N4corrected_norm_preproc.nii.gz
fi
#mcflirt
#brain
$singularity mcflirt -in $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_merged_res-iso.3_N4corrected_norm_brain_preproc.nii.gz -out $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_merged_mcflirted_res-iso.3_N4corrected_norm_brain_preproc.nii.gz -stages 4 -sinc_final -meanvol -report
#skull
$singularity mcflirt -in $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_merged_res-iso.3_N4corrected_norm_preproc.nii.gz -out $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_merged_mcflirted_res-iso.3_N4corrected_norm_preproc.nii.gz -stages 4 -sinc_final -meanvol -report
#tmean
#brain
$singularity fslmaths $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_merged_mcflirted_res-iso.3_N4corrected_norm_brain_preproc.nii.gz -Tmean $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_LinMoCo_res-iso.3_N4corrected_norm_brain_preproc.nii.gz
#skull
$singularity fslmaths $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_merged_mcflirted_res-iso.3_N4corrected_norm_preproc.nii.gz -Tmean $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_LinMoCo_res-iso.3_N4corrected_norm_preproc.nii.gz

rm $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_merged_res-iso.3_N4corrected_norm_brain_preproc.nii.gz
rm $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_merged_res-iso.3_N4corrected_norm_preproc.nii.gz
rm $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_merged_mcflirted_res-iso.3_N4corrected_norm_brain_preproc.nii.gz
rm $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_merged_mcflirted_res-iso.3_N4corrected_norm_preproc.nii.gz
rm $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_merged_mcflirted_res-iso.3_N4corrected_norm_brain_preproc.nii.gz_mean_reg.nii.gz
rm $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_merged_mcflirted_res-iso.3_N4corrected_norm_preproc.nii.gz_mean_reg.nii.gz
rm $deriv/preprocessing/$subjName/${subjName}_${ss}_T2w_run-*_N4corrected_preproc.nii.gz
