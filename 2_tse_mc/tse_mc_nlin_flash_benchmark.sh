#!/bin/bash
#nlin realignment/MC script for benchmarking flashlite.
subjName=sub-1001DS
ss="ses-06"
singularity="singularity exec --bind $TMPDIR:/TMPDIR --pwd /TMPDIR/tse_mc_benchmark_flash/ /30days/$USER/ants_fsl_robex_20180427.simg"
echo $subjName
module load singularity/2.4.2
chmod +rwx /30days/$USER/tse_mc_benchmark_flash/* 
#Copy everything to TMPDIR
cp -r /30days/$USER/tse_mc_benchmark_flash $TMPDIR
#nlin TSE MC
$singularity antsMultivariateTemplateConstruction.sh -d 3 -b 0 -c 2 -j 6 -i 3 -k 1 -t SyN -m '100x150x40' -s CC -t GR -n 0 -r 0 -g '0.1' -o brain_TSE_nlin_mc_ /TMPDIR/tse_mc_benchmark_flash/filenames.csv
#move it out
mkdir /30days/$USER/tse_mc_benchmark_flash_results
cp -r  $TMPDIR/* /30days/$USER/tse_mc_benchmark_flash_results
