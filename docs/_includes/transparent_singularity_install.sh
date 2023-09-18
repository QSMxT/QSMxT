git clone https://github.com/NeuroDesk/transparent-singularity qsmxt_{{ site.software_version }}_{{ site.build_date }}
cd qsmxt_{{ site.software_version }}_{{ site.build_date }}
./run_transparent_singularity.sh --container qsmxt_{{ site.software_version }}_{{ site.build_date }}.simg
source activate_qsmxt_{{ site.software_version }}_{{ site.build_date }}.simg.sh