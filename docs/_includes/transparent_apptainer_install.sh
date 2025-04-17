git clone https://github.com/astewartau/transparent-apptainer qsmxt_${PROD_CONTAINER_VERSION}_${PROD_CONTAINER_DATE}
cd qsmxt_${PROD_CONTAINER_VERSION}_${PROD_CONTAINER_DATE}
./run_transparent_apptainer.sh ${apptainer_opts} --container qsmxt_${PROD_CONTAINER_VERSION}_${PROD_CONTAINER_DATE}.simg
source activate_qsmxt_${PROD_CONTAINER_VERSION}_${PROD_CONTAINER_DATE}.simg.sh