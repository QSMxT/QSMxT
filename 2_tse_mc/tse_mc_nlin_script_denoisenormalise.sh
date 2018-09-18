#!/bin/bash
#qsm28 nlin realignment/MC script. Needs to be without skull for nlin.
#setup defaults:
subjName=$1
data_dir=/30days/uqtshaw/
singularity="singularity exec --bind $TMPDIR:/TMPDIR /30days/uqtshaw/ants_fsl_robex_20180427.simg"
echo $subjName
ss="ses-01"
module load singularity/2.4.2
#mv files
cp $data_dir/derivatives/tse_mc/${subjName}/${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_brain_preproc.nii.gz $TMPDIR
#denoise 
$singularity DenoiseImage -d 3 -n Rician -v -i /TMPDIR/${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_brain_preproc.nii.gz -o /TMPDIR/${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_denoised_brain_preproc.nii.gz
#normalise it
$singularity ~/scripts/niifti_normalise.sh -i /TMPDIR/${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_denoised_brain_preproc.nii.gz -o /TMPDIR/${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_denoised_norm_brain_preproc.nii.gz
#cp it out
cp $TMPDIR/${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_denoised_norm_brain_preproc.nii.gz $data_dir/derivatives/tse_mc/${subjName}
#Check it exists
if [[ ! -e $data_dir/derivatives/tse_mc/${subjName}/${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_denoised_norm_brain_preproc.nii.gz ]] ; then echo "ERROR $subjName not normalised omg!" >> $data_dir/error_list.txt
    exit 1 
fi

