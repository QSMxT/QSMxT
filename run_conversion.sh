

for file in `ls -1 ../raw/*.tar  | xargs -n 1 basename`; do
   filename=${file%.tar}
   echo $filename
   heudiconv -d '../raw/{subject}.tar' -s $filename -f heuristic.py -c dcm2niix -b -o .
done




# heudiconv -d '../raw/{subject}*.tar*' -f heuristic.py
#heudiconv -d '../raw/{subject}/*/*.dcm' -s s009_mc-20180504-132641 --ses 01 -f heuristic.py -o .
#heudiconv -d '../raw/{subject}.tar.gz' -s s008_lc-20180411-155608 --ses 01 -f heuristic.py -c dcm2niix -b --minmeta  -o .


# heudiconv -d /data/dumu/barth/7TShare/Data/3_studies/Hippocampus/{subject}/*/*IMA -s ## try `seq -w 1 29` or `ls -d /data/dumu/barth/7TShare/Data/3_studies/Hippocampus/` --ses 01 -f /data/dumu/barth/Data/3_studies/Hippocampus/convert_bids_script/heudiconv_file_bids.py -c dcm2niix -b --minmeta -o /winmounts/uqtshaw/uq-research/QSM28SUBJ-Q0530/qsm28subj/data/
