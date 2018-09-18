#!/bin/bash
for subjName in `cat /30days/$USER/subjnames_ses-12_redo.csv` ; do 
	qsub -v SUBJNAME=$subjName ~/scripts/OPTIMEX/1_preprocessing/pp_pbs_script_ses-12.pbs
done
