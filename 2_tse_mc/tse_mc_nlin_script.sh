#!/bin/bash
#qsm28 nlin realignment/MC script. Needs to be without skull for nlin.
#setup defaults:
subjName=$1
data_dir=/30days/uqtshaw/
singularity="singularity exec --bind $TMPDIR:/TMPDIR --pwd /TMPDIR/$subjName /30days/uqtshaw/ants_fsl_robex_20180427.simg"
echo $subjName
ss="ses-01"
module load singularity/2.4.2
#mv files
mkdir $data_dir/derivatives/tse_mc/$subjName
cp $data_dir/derivatives/preprocessing/${subjName}/${subjName}_ses-01_T2w_run-*_res-iso.3_N4corrected_norm_brain_preproc.nii.gz $data_dir/derivatives/tse_mc/$subjName
rm ${subjName}_ses-01_T2w_run-mean_res-iso.3_N4corrected_norm_brain_preproc.nii.gz
#Copy everything to TMPDIR
echo ${TMPDIR} is TMPDIR
mkdir ${TMPDIR}/${subjName}
for x in 1 2 3 ; do 
cp $data_dir/derivatives/tse_mc/$subjName/${subjName}_${ss}_T2w_run-${x}_res-iso.3_N4corrected_norm_brain_preproc.nii.gz $TMPDIR/$subjName/
done
cd $TMPDIR/$subjName
chmod +rwx $TMPDIR/$subjName/*
#nlin TSE MC
$singularity ls
$singularity antsMultivariateTemplateConstruction.sh -d '3' -b '1' -c '2' -j '4' -i '3' -k '1' -t SyN -m '50x80x20' -s CC -t GR -n '0' -r '1' -g '0.2' -o ${subjName}_${ss}_T2w_nlinMoCo_res-iso.3_N4corrected_brain_ ${subjName}_ses-01_T2w_run-*_res-iso.3_N4corrected_norm_brain_preproc.nii.gz
#move it out
cp -r $TMPDIR/* $data_dir/derivatives/tse_mc/
#move file to correct name
mv $data_dir/derivatives/tse_mc/${subjName}/${subjName}_${ss}_T2w_nlinMoCo_res-iso.3_N4corrected_brain_template0.nii.gz $data_dir/derivatives/tse_mc/${subjName}/${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_brain_preproc.nii.gz 
#normalise it
$singularity ~/scripts/niifti_normalise.sh -i ./${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_brain_preproc.nii.gz -o ./${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_norm_brain_preproc.nii.gz
cp $TMPDIR/$subjName/${subjName}_${ss}_T2w_NlinMoCo_res-iso.3_N4corrected_norm_brain_preproc.nii.gz $data_dir/derivatives/tse_mc/${subjName}
#Check it exists
if [[ ! -e $data_dir/derivatives/tse_mc/${subjName}/${subjName}_${ss}_T2w_NlinMoCoMI_res-iso.3_N4corrected_norm_brain_preproc.nii.gz ]] ; then echo "ERROR $subjName template not created" >> $data_dir/error_list.txt
    exit 1 
fi

