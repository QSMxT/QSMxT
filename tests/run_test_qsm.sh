#!/usr/bin/env bash
set -e 

cp -r . /tmp/QSMxT
# git clone https://github.com/QSMxT/QSMxT.git /tmp/QSMxT

container=`cat /tmp/QSMxT/README.md | grep -m 1 vnmd/qsmxt | cut -d ' ' -f 6`
echo "[DEBUG] this is the container I extracted from the readme: $container"

sudo docker pull $container

out_singlepass1='/tmp/02_qsm_output/qsm_final/sub-170705134431std1312211075243167001_ses-1_run-01_part-phase_T2starw_scaled_qsm-filled_000_average.nii'
out_singlepass2='/tmp/02_qsm_output/qsm_final/sub-170706160506std1312211075243167001_ses-1_run-01_part-phase_T2starw_scaled_qsm-filled_000_average.nii'
out_twopass1='/tmp/02_qsm_output/qsm_final/sub-170705134431std1312211075243167001_ses-1_run-01_part-phase_T2starw_scaled_qsm_000_twopass_average.nii'
out_twopass2='/tmp/02_qsm_output/qsm_final/sub-170706160506std1312211075243167001_ses-1_run-01_part-phase_T2starw_scaled_qsm_000_twopass_average.nii'
out_betaverage1='/tmp/02_qsm_output/qsm_final/sub-170705134431std1312211075243167001_ses-1_run-01_part-phase_T2starw_scaled_qsm_000_average.nii'
out_betaverage2='/tmp/02_qsm_output/qsm_final/sub-170706160506std1312211075243167001_ses-1_run-01_part-phase_T2starw_scaled_qsm_000_average.nii'
pip install osfclient
osf -p ru43c clone /tmp
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_M_5_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_P_6_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_M_5_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms
unzip /tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_P_6_QSM_p2_1mmIso_TE20.zip -d /tmp/dicoms

echo "[DEBUG] starting run_0_dicomSort.py"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_0_dicomSort.py /tmp/dicoms /tmp/00_dicom

echo "[DEBUG] starting run_1_dicomConvert.py"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_1_dicomConvert.py /tmp/00_dicom /tmp/01_bids --auto_yes

echo "[DEBUG] starting run_2_qsm.py normal"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2
[ -f $out_twopass1 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_twopass1 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_twopass2 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_twopass2 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_2_qsm.py --single_pass"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --single_pass
[ -f $out_singlepass1 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass1 -R -S`
#echo $min_max_std
#std=`echo $min_max_std | cut -d ' ' -f 3`; echo $std
#max=`echo $min_max_std | cut -d ' ' -f 2`; echo $max
#min=`echo $min_max_std | cut -d ' ' -f 1`; echo $min
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_singlepass2 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass2 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
sudo rm -rf /tmp/02_qsm_output


echo "[DEBUG] Testing individual features (+single_pass):"

echo "[DEBUG] starting run_2_qsm.py --inhomogeneity_correction --single_pass"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --inhomogeneity_correction --single_pass
[ -f $out_singlepass1 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass1 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_singlepass2 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass2 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f /tmp/02_qsm_output/workflow_qsm/sub-170705134431std1312211075243167001/ses-1/run-01/mriresearchtools_correct-inhomogeneity/mapflow/_mriresearchtools_correct-inhomogeneity0/result__mriresearchtools_correct-inhomogeneity0.pklz ] && echo "[DEBUG]. Test OK." || exit 1
sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_2_qsm.py --add_bet --single_pass"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --add_bet --single_pass
[ -f $out_singlepass1 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass1 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_singlepass2 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass2 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f /tmp/02_qsm_output/workflow_qsm/sub-170705134431std1312211075243167001/ses-1/run-01/fsl-bet/mapflow/_fsl-bet0/result__fsl-bet0.pklz ] && echo "[DEBUG]. Test OK." || exit 1
sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_2_qsm.py --extra_fill_strength 2 --single_pass"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --extra_fill_strength 2 --single_pass
[ -f $out_singlepass1 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass1 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_singlepass2 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass2 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_2_qsm.py --bet_fractional_intensity 0.4 --single_pass"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --bet_fractional_intensity 0.4 --single_pass
[ -f $out_singlepass1 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass1 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_singlepass2 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass2 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_2_qsm.py --threshold 20 --single_pass"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --threshold 20 --single_pass
[ -f $out_singlepass1 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass1 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_singlepass2 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass2 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_2_qsm.py --masking magnitude-based --single_pass"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --masking magnitude-based --single_pass
[ -f $out_singlepass1 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass1 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_singlepass2 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass2 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting `run_2`_qsm.py --masking phase-based --single_pass"
# sudo docker run -it -v /tmp:/tmp $container
# sudo docker run -it -v /tmp:/tmp vnmd/qsmxt_1.1.8:20211216
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --masking phase-based --single_pass
[ -f $out_singlepass1 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass1 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_singlepass2 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass2 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_2_qsm.py --masking bet --single_pass"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --masking bet --single_pass
[ -f $out_betaverage1 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_betaverage1 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_betaverage2 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_betaverage2 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
sudo rm -rf /tmp/02_qsm_output

echo "[DEBUG] starting run_2_qsm.py --num_echoes 1 --single_pass"
sudo docker run -v /tmp:/tmp $container python3 /tmp/QSMxT/run_2_qsm.py /tmp/01_bids /tmp/02_qsm_output --n_procs 2 --qsm_iterations 2 --num_echoes 1 --single_pass
[ -f $out_singlepass1 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass1 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi

[ -f $out_singlepass2 ] && echo "[DEBUG]. Test OK." || exit 1
#min_max_std=`sudo docker run -v /tmp:/tmp $container fslstats $out_singlepass2 -R -S`
#std=`echo $min_max_std | cut -d ' ' -f 3`
#max=`echo $min_max_std | cut -d ' ' -f 2`
#min=`echo $min_max_std | cut -d ' ' -f 1`
#if [ 1 -eq "$(echo "${std} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${max} > 0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
#if [ 1 -eq "$(echo "${min} < -0.0001" | bc)" ]; then echo "[DEBUG]. Test OK."; else echo "NOT OK" && exit 1; fi
sudo rm -rf /tmp/02_qsm_output

