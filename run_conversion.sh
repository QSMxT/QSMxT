

for file in `ls -1 ../raw/*.tar  | xargs -n 1 basename`; do
   filename=${file%.tar}
   echo $filename
   heudiconv -d '../raw/{subject}.tar' -s $filename -f heuristic.py -c dcm2niix -b -o .
done

OR
basePath=

#folder=mysubject

echo $basePath
for folder in `ls $basePath | xargs -n 1 basename`; do
   echo $folder
   heudiconv -d $basePath/{subject}/MONOPOLAR*/*.IMA -s $folder -f heuristic.py -c dcm2niix -b -o .
done
