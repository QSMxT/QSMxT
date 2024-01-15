#On Nectar: ubuntu 22.04 with docker installed
sudo docker pull vnmd/qsmxt_2.1.0:20230615
sudo apt update
sudo apt install unzip
sudo apt install python3-pip
sudo pip3 install osfclient
osf -p ru43c clone /storage/tmp
unzip /storage/tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_M_5_QSM_p2_1mmIso_TE20.zip -d /storage/tmp/dicoms
unzip /storage/tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub1/GR_P_6_QSM_p2_1mmIso_TE20.zip -d /storage/tmp/dicoms
unzip /storage/tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_M_5_QSM_p2_1mmIso_TE20.zip -d /storage/tmp/dicoms
unzip /storage/tmp/osfstorage/GRE_2subj_1mm_TE20ms/sub2/GR_P_6_QSM_p2_1mmIso_TE20.zip -d /storage/tmp/dicoms

#Then snapshot image and get id from image catelogue (e.g. 4adcec13-2291-4bff-987f-d87f98d24379)