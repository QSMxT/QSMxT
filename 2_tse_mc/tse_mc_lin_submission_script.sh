#!/bin/bash
for subjName in `cat /30days/uqtshaw/subjnames.csv` ; do 
	qsub -v SUBJNAME=$subjName ~/scripts/2_tse_mc/tse_mc_lin_pbs_script.pbs
done
