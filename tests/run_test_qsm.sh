#!/usr/bin/env bash

cp -r . /tmp/QSMxT
# git clone https://github.com/QSMxT/QSMxT.git /tmp/QSMxT

container=`cat /tmp/QSMxT/README.md | grep -m 1 vnmd/qsmxt | cut -d ' ' -f 6`
echo "[DEBUG] this is the container I extracted from the readme: $container"

sudo docker pull $container

out_singlepass1='/tmp/02_qsm_output/qsm_final/_run_run-1/sub-170705134431std1312211075243167001_ses-1_acq-qsmPH00_run-1_phase_scaled_qsm-filled_000_average.nii'
out_singlepass2='/tmp/02_qsm_output/qsm_final/_run_run-1/sub-170706160506std1312211075243167001_ses-1_acq-qsmPH00_run-1_phase_scaled_qsm-filled_000_average.nii'
out_twopass1='/tmp/02_qsm_output/qsm_final/_run_run-1/sub-170705134431std1312211075243167001_ses-1_acq-qsmPH00_run-1_phase_scaled_qsm_000_composite_average.nii'
out_twopass2='/tmp/02_qsm_output/qsm_final/_run_run-1/sub-170706160506std1312211075243167001_ses-1_acq-qsmPH00_run-1_phase_scaled_qsm_000_composite_average.nii'
out_betaverage1='/tmp/02_qsm_output/qsm_final/_run_run-1/sub-170705134431std1312211075243167001_ses-1_acq-qsmPH00_run-1_phase_scaled_qsm_000_average.nii'
out_betaverage2='/tmp/02_qsm_output/qsm_final/_run_run-1/sub-170706160506std1312211075243167001_ses-1_acq-qsmPH00_run-1_phase_scaled_qsm_000_average.nii'

pip install osfclient
osf -p ru43c clone /tmp
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_M_5_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_P_6_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_M_5_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_P_6_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms

echo "[DEBUG] starting run_0_dicomSort.py"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_0_dicomSort.py /tmp/dicoms /tmp/00_dicom

echo "[DEBUG] starting run_1_dicomToBids.py"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_1_dicomToBids.py /tmp/00_dicom /tmp/01_bids

echo "[DEBUG] starting run_2_qsm.py normal"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2
[ -f $out_singlepass1 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass1 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_singlepass2 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass2 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
sudo rm -rf /tmp/02_qsm_output


echo "[DEBUG] Testing individual features:"

echo "[DEBUG] starting run_2_qsm.py --inhomogeneity_correction"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --inhomogeneity_correction 
[ -f $out_singlepass1 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass1 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_singlepass2 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass2 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f /tmp/02_qsm_output/workflow_qsm/sub-170705134431std1312211075243167001/ses-1/_run_run-1/correct_inhomogeneity/mapflow/_correct_inhomogeneity0/result__correct_inhomogeneity0.pklz ] && echo "[DEBUG]. Test OK." || exit 1
sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_2_qsm.py --no_resampling"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --no_resampling 
[ -f $out_singlepass1 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass1 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_singlepass2 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass2 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_2_qsm.py --add_bet"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --add_bet 
[ -f $out_singlepass1 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass1 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_singlepass2 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass2 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f /tmp/02_qsm_output/workflow_qsm/sub-170705134431std1312211075243167001/ses-1/_run_run-1/fsl_bet/mapflow/_fsl_bet0/result__fsl_bet0.pklz ] && echo "[DEBUG]. Test OK." || exit 1
sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_2_qsm.py --extra_fill_strength 2"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --extra_fill_strength 2
[ -f $out_singlepass1 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass1 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_singlepass2 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass2 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_2_qsm.py --bet_fractional_intensity 0.4"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --bet_fractional_intensity 0.4 
[ -f $out_singlepass1 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass1 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_singlepass2 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass2 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_2_qsm.py --threshold 20"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --threshold 20 
[ -f $out_singlepass1 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass1 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_singlepass2 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass2 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_2_qsm.py --two_pass"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --two_pass 
[ -f $out_twopass1 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_twopass1 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_twopass2 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_twopass2 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_2_qsm.py --masking magnitude-based"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --masking magnitude-based 
[ -f $out_singlepass1 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass1 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_singlepass2 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass2 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_2_qsm.py --masking phase-based"
# sudo docker run -it -v /tmp:/tmp $container
# sudo docker run -it -v /tmp:/tmp vnmd/qsmxt_1.1.8:20211216
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --masking phase-based
[ -f $out_singlepass1 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass1 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_singlepass2 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass2 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_2_qsm.py --masking bet"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --masking bet 
[ -f $out_betaverage1 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_betaverage1 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_betaverage2 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_betaverage2 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_2_qsm.py --num_echoes 1"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --num_echoes 1 
[ -f $out_singlepass1 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass1 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_singlepass2 ] && echo "[DEBUG]. Test OK." || exit 1
min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass2 -R -S`
std=`echo $min_max_std | cut -d ' ' -f 26`
max=`echo $min_max_std | cut -d ' ' -f 25`
min=`echo $min_max_std | cut -d ' ' -f 24`
if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
sudo rm -rf /tmp/02_qsm_output






echo "[DEBUG] Testing combination of features:"



echo "[DEBUG] starting run_2_qsm.py --two_pass --masking magnitude-based"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --two_pass --masking magnitude-based 
[ -f $out_twopass1 ] && echo "[DEBUG]. Test OK." || exit 1
[ -f $out_twopass2 ] && echo "[DEBUG]. Test OK." || exit 1
[ -f /tmp/02_qsm_output/workflow_qsm/sub-170705134431std1312211075243167001/ses-1/_run_run-1/magnitude_mask/mapflow/_magnitude_mask0/result__magnitude_mask0.pklz ] && echo "[DEBUG]. Test OK." || exit 1
sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_2_qsm.py --two_pass --masking phase-based --extra_fill_strength 2"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --two_pass --masking phase-based --extra_fill_strength 2
[ -f $out_twopass1 ] && echo "[DEBUG]. Test OK." || exit 1
[ -f $out_twopass2 ] && echo "[DEBUG]. Test OK." || exit 1
sudo rm -rf /tmp/02_qsm_output