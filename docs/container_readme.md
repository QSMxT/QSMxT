----------------------------------
## qsmxt/toolVersion ##

A full QSM processing pipeline from DICOM to segmentation to evaluation of results. 

To sort DICOMs, use:
    $ dicom-sort YOUR_DICOM_DIR dicoms-sorted

To convert sorted DICOMs to BIDS, use:
    $ dicom-convert dicoms-sorted bids

To run QSMxT, use:
    $ qsmxt bids qsm

For full documentation, see https://qsmxt.github.io/

To run applications outside of this container: ml qsmxt/toolVersion

----------------------------------
