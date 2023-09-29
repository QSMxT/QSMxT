git clone https://github.com/NeuroDesk/transparent-singularity qsmxt_${CONTAINER_VERSION}_${BUILD_DATE}
  cd qsmxt_${CONTAINER_VERSION}_${BUILD_DATE}
  ./run_transparent_singularity.sh --container qsmxt_${CONTAINER_VERSION}_${BUILD_DATE}.simg
  source activate_qsmxt_${CONTAINER_VERSION}_${BUILD_DATE}.simg.sh