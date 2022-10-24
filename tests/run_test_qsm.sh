#!/usr/bin/env bash
set -e 
git clone https://github.com/QSMxT/QSMxT.git /tmp/QSMxT

container=`cat /tmp/QSMxT/README.md | grep -m 1 vnmd/qsmxt | cut -d ' ' -f 6`
echo "[DEBUG] This is the container I extracted from the readme: $container"
sudo docker pull $container

sudo docker run -v /tmp:/tmp $container pytest /tmp/QSMxT/scripts/run_test_qsm.py

