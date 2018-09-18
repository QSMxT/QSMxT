#!/bin/bash
for subjName in `cat /30days/$USER/subjnames.csv` ; do 
	qsub -v SUBJNAME=$subjName ~/scripts/QSM28/2_tse_mc/tse_mc_nlin_pbs_script.pbs
done
