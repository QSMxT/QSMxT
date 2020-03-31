#!/bin/bash

# input dicom folder
dicomPath=dicom

# output bids folder
bidsPath=bids

scriptPath="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
heuristicPath="${scriptPath}/bidsmap.yaml"
bidsmapper.py -b "${heuristicPath}" -i 0 "${dicomPath}" "${bidsPath}"
bidscoiner.py "${dicomPath}" "${bidsPath}"

# for dicoms files
#for folder in `ls $dicomPath | xargs -n 1 basename`; do
#   echo $folder
#   heudiconv -d $dicomPath/{subject}/SEQNAME*/*.IMA -s $folder -f heuristic.py -c dcm2niix -b -o .
#done

# for dicoms in a tar files
#for file in `ls -1 "${dicomPath}"/*.tar  | xargs -n 1 basename`; do
#    filename="${file%.tar}"
#    echo "${filename}"
#    heudiconv -d "${dicomPath}/{subject}.tar" -s "${filename}" -f heuristic.py -c dcm2niix -b -o "${bidsPath}"
#done
