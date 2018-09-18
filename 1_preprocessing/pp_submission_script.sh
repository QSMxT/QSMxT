#!/bin/bash
for subjName in `cat /30days/$USER/subjnames.csv` ; do 
	qsub -v SUBJNAME=$subjName ~/scripts/QSM28/1_preprocessing/pp_pbs_script.pbs
done
