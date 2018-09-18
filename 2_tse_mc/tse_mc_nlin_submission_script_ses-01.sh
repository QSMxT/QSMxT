#!/bin/bash
for subjName in `cat /30days/$USER/subjnames_ses-01_redo.csv` ; do 
	qsub -v SUBJNAME=$subjName ~/scripts/OPTIMEX/2_tse_mc/tse_mc_nlin_pbs_script_ses-01.pbs
done
