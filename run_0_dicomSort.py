#!/bin/python

# Adapted from Alex Weston
# Digital Innovation Lab, Mayo Clinic
# https://gist.github.com/alex-weston-13/4dae048b423f1b4cb9828734a4ec8b83
import argparse
import os
import pydicom # pydicom is using the gdcm package for decompression

def clean_text(string):
    # clean and standardize text descriptions, which makes searching files easier
    forbidden_symbols = ["*", ".", ",", "\"", "\\", "/", "|", "[", "]", ":", ";", " "]
    for symbol in forbidden_symbols:
        string = string.replace(symbol, "_") # replace everything with an underscore
    return string.lower()  

def dicomsort(src, dst, use_patient_name):
    os.makedirs(dst, exist_ok=True)
    extension = '.IMA'
    print('reading file list...')
    unsortedList = []
    for root, dirs, files in os.walk(src):
        for file in files:
            if file[-4:] in ['.ima', '.IMA']: # exclude non-dicoms, good for messy folders
                unsortedList.append(os.path.join(root, file))
            elif file[-4:] in ['.dcm', '.DCM']:
                extension = '.dcm'
                unsortedList.append(os.path.join(root, file))


    print('%s files found.' % len(unsortedList))
        
    for dicom_loc in unsortedList:
        # read the file
        ds = pydicom.read_file(dicom_loc, force=True)
    
        # get patient, study, and series information
        patientName = clean_text(str(ds.get("PatientName", "NA")))
        patientID = clean_text(ds.get("PatientID", "NA"))
        studyDate = clean_text(ds.get("StudyDate", "NA"))
        studyDescription = clean_text(ds.get("StudyDescription", "NA"))
        seriesDescription = clean_text(ds.get("SeriesDescription", "NA"))
    
        # generate new, standardized file name
        modality = ds.get("Modality","NA")
        studyInstanceUID = ds.get("StudyInstanceUID","NA")
        seriesInstanceUID = ds.get("SeriesInstanceUID","NA")
        instanceNumber = str(ds.get("InstanceNumber","0"))
        fileName = modality + "." + seriesInstanceUID + "." + instanceNumber + extension

        subj_name = patientName if use_patient_name else patientID
        
        # uncompress files (using the gdcm package)
        try:
            ds.decompress()
        except:
            print('an instance in file %s - %s - %s - %s" could not be decompressed. exiting.' % (subj_name, studyDate, studyDescription, seriesDescription ))
    
        # save files to a 3-tier nested folder structure
        subjName_date = f"sub-{subj_name}_{studyDate}"

        if not os.path.exists(os.path.join(dst, subjName_date, seriesDescription)):
            os.makedirs(os.path.join(dst, subjName_date, seriesDescription), exist_ok=True)
            print('Saving out file: %s - %s.' % (subjName_date, seriesDescription))
        
        ds.save_as(os.path.join(dst, subjName_date, seriesDescription, fileName))

        if os.path.exists(os.path.join(dst, subjName_date, seriesDescription, fileName)):
            os.remove(dicom_loc)

    print('done.')

if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="QSMxT DICOM to BIDS converter",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        'src',
        help='folder containing DICOM files'
    )

    parser.add_argument(
        'dst',
        default=None,
        const=None,
        nargs='?',
        help='output folder to contain sorted DICOMs'
    )

    parser.add_argument(
        '--use_patient_name',
        action='store_true',
        help='use patient name rather than ID for subject folders'
    )

    args = parser.parse_args()
    dicomsort(args.src, args.dst if args.dst is not None else args.src, args.use_patient_name)
    