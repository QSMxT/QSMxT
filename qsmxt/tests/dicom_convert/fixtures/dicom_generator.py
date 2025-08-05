#!/usr/bin/env python3
"""
DICOM generator utility for creating synthetic DICOM files for testing.
Uses pydicom to create valid DICOM files with proper headers.
"""

import os
import tempfile
from datetime import datetime
from typing import List, Dict, Optional, Tuple
import numpy as np

try:
    import pydicom
    from pydicom.dataset import Dataset, FileDataset
    from pydicom.uid import generate_uid
except ImportError:
    raise ImportError("pydicom is required for DICOM generation. Install with: pip install pydicom")


def create_dicom_file(
    filename: str,
    patient_id: str = "TEST001",
    patient_name: str = "Test^Patient",
    study_date: str = None,
    series_description: str = "Test Series",
    image_type: List[str] = None,
    echo_time: float = 5.0,
    inversion_time: Optional[float] = None,
    series_number: int = 1,
    instance_number: int = 1,
    rows: int = 64,
    cols: int = 64,
    manufacturer: str = "SIEMENS",
    pixel_data: Optional[np.ndarray] = None,
    series_instance_uid: Optional[str] = None,
    study_instance_uid: Optional[str] = None,
    sop_instance_uid: Optional[str] = None,
    coil_string: Optional[str] = None,
) -> str:
    """
    Create a synthetic DICOM file with specified parameters.
    
    Parameters:
    - filename: Output filename
    - patient_id: Patient identifier
    - patient_name: Patient name in DICOM format (Last^First)
    - study_date: Study date in YYYYMMDD format
    - series_description: Description of the series
    - image_type: List of image type strings (e.g., ['ORIGINAL', 'PRIMARY', 'M'])
    - echo_time: Echo time in milliseconds
    - inversion_time: Inversion time in milliseconds (optional)
    - series_number: Series number
    - instance_number: Instance number within the series
    - rows/cols: Image dimensions
    - manufacturer: Equipment manufacturer
    - pixel_data: Optional numpy array of pixel data
    - series_instance_uid: Series UID (generated if not provided)
    - study_instance_uid: Study UID (generated if not provided)
    - sop_instance_uid: SOP Instance UID (generated if not provided)
    - coil_string: Coil information (for multi-coil data)
    
    Returns:
    - Path to created DICOM file
    """
    
    # Create file meta information
    file_meta = pydicom.dataset.FileMetaDataset()
    file_meta.MediaStorageSOPClassUID = pydicom.uid.MRImageStorage
    file_meta.MediaStorageSOPInstanceUID = sop_instance_uid or generate_uid()
    file_meta.TransferSyntaxUID = pydicom.uid.ExplicitVRLittleEndian
    file_meta.ImplementationClassUID = generate_uid()
    
    # Create the FileDataset instance
    ds = FileDataset(filename, {}, file_meta=file_meta, preamble=b"\0" * 128)
    
    # Set patient information
    ds.PatientName = patient_name
    ds.PatientID = patient_id
    
    # Set study information
    if study_date is None:
        study_date = datetime.now().strftime("%Y%m%d")
    ds.StudyDate = study_date
    ds.StudyTime = datetime.now().strftime("%H%M%S")
    ds.StudyInstanceUID = study_instance_uid or generate_uid()
    
    # Set series information
    ds.SeriesDate = study_date
    ds.SeriesTime = datetime.now().strftime("%H%M%S")
    ds.SeriesDescription = series_description
    ds.SeriesNumber = series_number
    ds.SeriesInstanceUID = series_instance_uid or generate_uid()
    
    # Set image information
    ds.InstanceNumber = instance_number
    ds.SOPClassUID = pydicom.uid.MRImageStorage
    ds.SOPInstanceUID = file_meta.MediaStorageSOPInstanceUID
    
    if image_type is None:
        image_type = ['ORIGINAL', 'PRIMARY', 'M', 'ND']
    ds.ImageType = image_type
    
    # Set acquisition parameters
    ds.EchoTime = echo_time
    if inversion_time is not None:
        ds.InversionTime = inversion_time
    
    # Set equipment information
    ds.Manufacturer = manufacturer
    ds.InstitutionName = "Test Institution"
    
    # Set image pixel data
    ds.Rows = rows
    ds.Columns = cols
    ds.SamplesPerPixel = 1
    ds.PhotometricInterpretation = "MONOCHROME2"
    ds.BitsAllocated = 16
    ds.BitsStored = 16
    ds.HighBit = 15
    ds.PixelRepresentation = 0
    
    if pixel_data is None:
        # Create synthetic pixel data
        pixel_data = np.random.randint(0, 1000, size=(rows, cols), dtype=np.uint16)
    ds.PixelData = pixel_data.tobytes()
    
    # Add manufacturer-specific tags
    if manufacturer.upper() == "SIEMENS":
        # Add Siemens-specific tags
        if coil_string:
            ds[0x0051, 0x100f] = pydicom.DataElement(0x0051100f, 'LO', coil_string)
    elif manufacturer.upper() == "GE":
        # Add GE-specific tags
        pass
    elif manufacturer.upper() == "PHILIPS":
        # Add Philips-specific tags
        pass
    
    # Save the DICOM file
    ds.save_as(filename, write_like_original=False)
    
    return filename


def create_dicom_series(
    output_dir: str,
    series_type: str = "magnitude",
    num_slices: int = 10,
    num_echoes: int = 1,
    patient_id: str = "TEST001",
    series_number: int = 1,
    manufacturer: str = "SIEMENS",
    coil_string: Optional[str] = None,
) -> List[str]:
    """
    Create a complete DICOM series with multiple slices.
    
    Parameters:
    - output_dir: Directory to save DICOM files
    - series_type: Type of series ('magnitude', 'phase', 'real', 'imaginary', 't1w')
    - num_slices: Number of slices in the series
    - num_echoes: Number of echoes (for multi-echo sequences)
    - patient_id: Patient identifier
    - series_number: Series number
    - manufacturer: Equipment manufacturer
    - coil_string: Coil information
    
    Returns:
    - List of created DICOM file paths
    """
    
    os.makedirs(output_dir, exist_ok=True)
    created_files = []
    
    # Determine image type based on series type
    image_type_map = {
        'magnitude': ['ORIGINAL', 'PRIMARY', 'M', 'ND'],
        'phase': ['ORIGINAL', 'PRIMARY', 'P', 'ND'],
        'real': ['ORIGINAL', 'PRIMARY', 'REAL'],
        'imaginary': ['ORIGINAL', 'PRIMARY', 'IMAGINARY'],
        't1w': ['ORIGINAL', 'PRIMARY', 'M', 'ND', 'NORM', 'UNI'],
    }
    
    image_type = image_type_map.get(series_type, ['ORIGINAL', 'PRIMARY'])
    series_description = f"{series_type.upper()} Series"
    
    # Generate UIDs for the series
    study_uid = generate_uid()
    series_uid = generate_uid()
    
    instance_counter = 1
    
    for echo_idx in range(num_echoes):
        echo_time = 5.0 + (echo_idx * 5.0)  # 5ms, 10ms, 15ms, etc.
        
        for slice_idx in range(num_slices):
            filename = os.path.join(
                output_dir,
                f"IM_{series_number:04d}_{instance_counter:04d}.dcm"
            )
            
            create_dicom_file(
                filename=filename,
                patient_id=patient_id,
                series_description=series_description,
                image_type=image_type,
                echo_time=echo_time,
                series_number=series_number,
                instance_number=instance_counter,
                manufacturer=manufacturer,
                series_instance_uid=series_uid,
                study_instance_uid=study_uid,
                coil_string=coil_string,
            )
            
            created_files.append(filename)
            instance_counter += 1
    
    return created_files


def create_magnitude_phase_pair(
    output_dir: str,
    num_slices: int = 10,
    num_echoes: int = 1,
    patient_id: str = "TEST001",
    manufacturer: str = "SIEMENS",
) -> Dict[str, List[str]]:
    """
    Create matched magnitude and phase DICOM series.
    
    Returns:
    - Dictionary with 'magnitude' and 'phase' keys containing file lists
    """
    mag_files = create_dicom_series(
        output_dir=output_dir,
        series_type="magnitude",
        num_slices=num_slices,
        num_echoes=num_echoes,
        patient_id=patient_id,
        series_number=1,
        manufacturer=manufacturer,
    )
    
    phase_files = create_dicom_series(
        output_dir=output_dir,
        series_type="phase",
        num_slices=num_slices,
        num_echoes=num_echoes,
        patient_id=patient_id,
        series_number=2,
        manufacturer=manufacturer,
    )
    
    return {"magnitude": mag_files, "phase": phase_files}


def create_real_imaginary_pair(
    output_dir: str,
    num_slices: int = 10,
    num_echoes: int = 1,
    patient_id: str = "TEST001",
    manufacturer: str = "SIEMENS",
) -> Dict[str, List[str]]:
    """
    Create matched real and imaginary DICOM series.
    
    Returns:
    - Dictionary with 'real' and 'imaginary' keys containing file lists
    """
    real_files = create_dicom_series(
        output_dir=output_dir,
        series_type="real",
        num_slices=num_slices,
        num_echoes=num_echoes,
        patient_id=patient_id,
        series_number=3,
        manufacturer=manufacturer,
    )
    
    imag_files = create_dicom_series(
        output_dir=output_dir,
        series_type="imaginary",
        num_slices=num_slices,
        num_echoes=num_echoes,
        patient_id=patient_id,
        series_number=4,
        manufacturer=manufacturer,
    )
    
    return {"real": real_files, "imaginary": imag_files}


def create_multicoil_data(
    output_dir: str,
    num_coils: int = 4,
    num_slices: int = 10,
    patient_id: str = "TEST001",
    manufacturer: str = "SIEMENS",
) -> Dict[str, List[str]]:
    """
    Create multi-coil DICOM data.
    
    Returns:
    - Dictionary with coil names as keys containing file lists
    """
    coil_data = {}
    
    for coil_idx in range(1, num_coils + 1):
        coil_string = f"C{coil_idx:02d}"
        coil_dir = os.path.join(output_dir, f"coil_{coil_idx:02d}")
        
        # Create magnitude and phase for each coil
        coil_files = create_magnitude_phase_pair(
            output_dir=coil_dir,
            num_slices=num_slices,
            patient_id=patient_id,
            manufacturer=manufacturer,
        )
        
        # Add coil information to each file
        for mag_file in coil_files["magnitude"]:
            ds = pydicom.dcmread(mag_file)
            ds[0x0051, 0x100f] = pydicom.DataElement(0x0051100f, 'LO', coil_string)
            ds.save_as(mag_file)
        
        for phase_file in coil_files["phase"]:
            ds = pydicom.dcmread(phase_file)
            ds[0x0051, 0x100f] = pydicom.DataElement(0x0051100f, 'LO', coil_string)
            ds.save_as(phase_file)
        
        coil_data[coil_string] = coil_files
    
    return coil_data


def create_test_dicom_directory(
    base_dir: str,
    include_t1w: bool = True,
    include_multiecho: bool = True,
    include_multicoil: bool = False,
    manufacturer: str = "SIEMENS",
) -> Dict[str, any]:
    """
    Create a complete test DICOM directory structure.
    
    Parameters:
    - base_dir: Base directory for DICOM data
    - include_t1w: Include T1-weighted anatomical data
    - include_multiecho: Include multi-echo GRE data
    - include_multicoil: Include multi-coil data
    - manufacturer: Equipment manufacturer
    
    Returns:
    - Dictionary describing created data structure
    """
    os.makedirs(base_dir, exist_ok=True)
    created_data = {}
    
    # Create basic magnitude/phase pair
    basic_dir = os.path.join(base_dir, "basic_gre")
    created_data["basic_gre"] = create_magnitude_phase_pair(
        output_dir=basic_dir,
        num_slices=10,
        manufacturer=manufacturer,
    )
    
    # Create T1w data if requested
    if include_t1w:
        t1w_dir = os.path.join(base_dir, "t1w")
        created_data["t1w"] = create_dicom_series(
            output_dir=t1w_dir,
            series_type="t1w",
            num_slices=192,  # Typical T1w has more slices
            series_number=10,
            manufacturer=manufacturer,
        )
    
    # Create multi-echo data if requested
    if include_multiecho:
        multiecho_dir = os.path.join(base_dir, "multiecho_gre")
        created_data["multiecho_gre"] = create_magnitude_phase_pair(
            output_dir=multiecho_dir,
            num_slices=10,
            num_echoes=3,  # 3 echoes
            manufacturer=manufacturer,
        )
    
    # Create multi-coil data if requested
    if include_multicoil:
        multicoil_dir = os.path.join(base_dir, "multicoil")
        created_data["multicoil"] = create_multicoil_data(
            output_dir=multicoil_dir,
            num_coils=4,
            num_slices=10,
            manufacturer=manufacturer,
        )
    
    return created_data


# Example usage for testing
if __name__ == "__main__":
    with tempfile.TemporaryDirectory() as tmpdir:
        print(f"Creating test DICOM data in: {tmpdir}")
        
        # Create a complete test dataset
        data = create_test_dicom_directory(
            base_dir=tmpdir,
            include_t1w=True,
            include_multiecho=True,
            include_multicoil=True,
        )
        
        print("\nCreated data structure:")
        for key, value in data.items():
            print(f"\n{key}:")
            if isinstance(value, dict):
                for subkey, files in value.items():
                    print(f"  {subkey}: {len(files)} files")
            else:
                print(f"  {len(value)} files")