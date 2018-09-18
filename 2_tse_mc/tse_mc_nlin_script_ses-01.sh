#!/bin/bash
#nlin realignment/MC script. Needs to be without skull for nlin.
subjName=$1
ss="ses-01"
singularity="singularity exec --bind $TMPDIR:/TMPDIR --pwd /TMPDIR/$subjName /30days/uqtshaw/ants_fsl_robex_20180427.simg"
module load singularity/2.5.1
#mv files
mkdir $TMPDIR/$subjName
for x in 1 2 3 ; do 
rsync -r -c -v /RDS/Q0535/optimex/data/derivatives/preprocessing/${subjName}/${subjName}_${ss}_T2w_run-${x}_res-iso.3_N4corrected_norm_brain_preproc.nii.gz $TMPDIR/$subjName ;
done
cd $TMPDIR/$subjName
chmod +rwx $TMPDIR/$subjName/*
#nlin TSE MC
$singularity ls

#nlin TSE MC
$singularity antsMultivariateTemplateConstruction.sh -d '3' -b '0' -c '2' -j '4' -i '2' -k '1' -t SyN -m '50x80x20' -s CC -t GR -n '0' -r '0' -g '0.2' -o ${subjName}_${ss}_T2w_nlinMoCo_res-iso.3_N4corrected_brain_ ${subjName}_ses-01_T2w_run-*_res-iso.3_N4corrected_norm_brain_preproc.nii.gz

#FIXME#move file to correct name
mv $TMPDIR/${subjName}/${subjName}_${ss}_T2w_nlinMoCo_res-iso.3_N4corrected_brain_template0.nii.gz $TMPDIR/${subjName}/${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_brain_preproc.nii.gz

$singularity DenoiseImage -d 3 -n Rician -i ${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_brain_preproc.nii.gz -o ${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_denoised_brain_preproc.nii.gz -v

$singularity ~/scripts/niifti_normalise.sh -i ./${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_denoised_brain_preproc.nii.gz -o ./${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_denoised_norm_brain_preproc.nii.gz

#Check it exists
if [[ ! -e $TMPDIR/${subjName}/${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_denoised_norm_brain_preproc.nii.gz ]] ; then echo "ERROR $subjName $ss template not created" >> /30days/$USER/error_list.txt
fi
rsync -v -c -r ${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_denoised_norm_brain_preproc.nii.gz /RDS/Q0535/optimex/data/derivatives/preprocessing/$subjName
rsync -v -c -r ${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_denoised_brain_preproc.nii.gz /RDS/Q0535/optimex/data/derivatives/preprocessing/$subjName

