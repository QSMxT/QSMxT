#!/usr/bin/env python3
"""
Helper functions to create complete mock DICOM DataFrames for testing.
"""

import pandas as pd
import datetime
from typing import List, Dict, Any, Optional


def create_complete_dicom_dataframe(
    patient_id: str = "patient001",
    patient_name: str = "Test Patient",
    study_date: str = "20230101",
    acquisition: str = "gre",
    series_description: str = "GRE_Sequence",
    image_type: List[str] = None,
    num_echoes: int = 1,
    echo_times: List[float] = None,
    inversion_times: List[float] = None,
    num_instances: int = 64,
    series_time: str = "120000",
    acquisition_time: str = "120000",
    acquisition_date: str = None,
    series_date: str = None,
    coil_info: str = None,
    series_instance_uid: str = None,
    run_number: int = 1,
    dicom_path: str = "/mock/path/dicom.dcm"
) -> pd.DataFrame:
    """
    Create a complete mock DICOM DataFrame with all fields expected by dicom_convert functions.
    
    Parameters:
    - patient_id: Patient identifier
    - patient_name: Patient name
    - study_date: Study date (YYYYMMDD format)
    - acquisition: Acquisition name
    - series_description: Series description
    - image_type: List of image type indicators (e.g., ['M'], ['P'], ['REAL'], ['IMAGINARY'])
    - num_echoes: Number of echoes in the sequence
    - echo_times: List of echo times (if None, generates based on num_echoes)
    - inversion_times: List of inversion times (optional)
    - num_instances: Number of DICOM instances
    - series_time: Series acquisition time (HHMMSS format)
    - acquisition_time: Acquisition time (HHMMSS format)
    - acquisition_date: Acquisition date (defaults to study_date)
    - series_date: Series date (defaults to study_date)
    - coil_info: Coil information string (e.g., "C32")
    - series_instance_uid: Series instance UID
    - run_number: Run number
    - dicom_path: Path to DICOM file
    
    Returns:
    - Complete pandas DataFrame ready for dicom_convert functions
    """
    
    if image_type is None:
        image_type = ['M']  # Default to magnitude
    
    if echo_times is None:
        if num_echoes == 1:
            echo_times = [5.0]  # Single echo time in ms  
        else:
            echo_times = [5.0 + i * 5.0 for i in range(num_echoes)]  # Multiple echoes
    
    if acquisition_date is None:
        acquisition_date = study_date
    
    if series_date is None:
        series_date = study_date
    
    if series_instance_uid is None:
        series_instance_uid = f"1.2.3.4.5.{hash(series_description) % 1000000}"
    
    # Create base data for all echoes/instances
    rows = []
    for echo_idx in range(num_echoes):
        for instance_idx in range(num_instances):
            row = {
                # Patient information
                'PatientID': patient_id,
                'PatientName': patient_name,
                
                # Study information
                'StudyDate': study_date,
                'AcquisitionDate': acquisition_date,
                'SeriesDate': series_date,
                
                # Series information
                'Acquisition': acquisition,
                'SeriesDescription': series_description,
                'SeriesTime': series_time,
                'AcquisitionTime': acquisition_time,
                'SeriesInstanceUID': series_instance_uid,
                'RunNumber': run_number,
                
                # Image information - convert list to tuple for pandas groupby compatibility
                'ImageType': tuple(image_type) if isinstance(image_type, list) else image_type,
                'EchoTime': echo_times[echo_idx] if echo_idx < len(echo_times) else echo_times[0],
                'InstanceNumber': instance_idx + 1,
                
                # File information
                'DICOM_Path': f"{dicom_path}_{echo_idx:02d}_{instance_idx:03d}.dcm",
                
                # Optional fields
                'Count': num_instances,  # Will be set correctly by groupby operations
            }
            
            # Add inversion times if provided
            if inversion_times:
                if echo_idx < len(inversion_times):
                    row['InversionTime'] = inversion_times[echo_idx]
                else:
                    row['InversionTime'] = None
            
            # Add coil information if provided
            if coil_info:
                row['(0051,100F)'] = coil_info
            
            rows.append(row)
    
    return pd.DataFrame(rows)


def create_mag_phase_pair(
    patient_id: str = "patient001",
    acquisition: str = "gre",
    echo_times: List[float] = None,
    **kwargs
) -> pd.DataFrame:
    """Create a magnitude/phase pair for testing."""
    if echo_times is None:
        echo_times = [5.0]
    
    # Create magnitude data
    mag_df = create_complete_dicom_dataframe(
        patient_id=patient_id,
        acquisition=acquisition,
        series_description=f"{acquisition}_MAG",
        image_type=['M'],
        echo_times=echo_times,
        **kwargs
    )
    
    # Create phase data
    phase_df = create_complete_dicom_dataframe(
        patient_id=patient_id,
        acquisition=acquisition,
        series_description=f"{acquisition}_PHASE", 
        image_type=['P'],
        echo_times=echo_times,
        series_instance_uid=f"1.2.3.4.5.{hash(f'{acquisition}_PHASE') % 1000000}",
        dicom_path="/mock/path/phase_dicom.dcm",
        **kwargs
    )
    
    return pd.concat([mag_df, phase_df], ignore_index=True)


def create_real_imag_pair(
    patient_id: str = "patient001",
    acquisition: str = "gre",
    echo_times: List[float] = None,
    **kwargs
) -> pd.DataFrame:
    """Create a real/imaginary pair for testing."""
    if echo_times is None:
        echo_times = [5.0]
    
    # Create real data
    real_df = create_complete_dicom_dataframe(
        patient_id=patient_id,
        acquisition=acquisition,
        series_description=f"{acquisition}_REAL",
        image_type=['REAL'],
        echo_times=echo_times,
        **kwargs
    )
    
    # Create imaginary data
    imag_df = create_complete_dicom_dataframe(
        patient_id=patient_id,
        acquisition=acquisition,
        series_description=f"{acquisition}_IMAGINARY",
        image_type=['IMAGINARY'],
        echo_times=echo_times,
        series_instance_uid=f"1.2.3.4.5.{hash(f'{acquisition}_IMAGINARY') % 1000000}",
        dicom_path="/mock/path/imag_dicom.dcm",
        **kwargs
    )
    
    return pd.concat([real_df, imag_df], ignore_index=True)


def create_t1w_data(
    patient_id: str = "patient001",
    acquisition: str = "mprage",
    **kwargs
) -> pd.DataFrame:
    """Create T1w anatomical data for testing."""
    return create_complete_dicom_dataframe(
        patient_id=patient_id,
        acquisition=acquisition,
        series_description="T1_MPRAGE",
        image_type=['UNI', 'M'],  # UNI indicates T1w
        num_instances=192,  # Typical T1w volume size
        **kwargs
    )


def create_multiecho_data(
    patient_id: str = "patient001",
    acquisition: str = "gre",
    num_echoes: int = 3,
    **kwargs
) -> pd.DataFrame:
    """Create multi-echo magnitude/phase data for testing."""
    echo_times = [5.0 + i * 5.0 for i in range(num_echoes)]  # 5ms, 10ms, 15ms, etc.
    
    return create_mag_phase_pair(
        patient_id=patient_id,
        acquisition=acquisition,
        echo_times=echo_times,
        num_echoes=num_echoes,
        **kwargs
    )


def create_multicoil_data(
    patient_id: str = "patient001",
    acquisition: str = "gre", 
    num_coils: int = 4,
    **kwargs
) -> pd.DataFrame:
    """Create multi-coil data for testing."""
    dfs = []
    
    for coil_idx in range(1, num_coils + 1):
        coil_df = create_mag_phase_pair(
            patient_id=patient_id,
            acquisition=acquisition,
            coil_info=f"C{coil_idx:02d}",
            series_instance_uid=f"1.2.3.4.5.{coil_idx}",
            dicom_path=f"/mock/path/coil_{coil_idx:02d}_dicom.dcm",
            **kwargs
        )
        dfs.append(coil_df)
    
    return pd.concat(dfs, ignore_index=True)


# Convenience functions for common test scenarios
def create_minimal_valid_dataframe() -> pd.DataFrame:
    """Create the minimal valid DataFrame for basic testing."""
    return create_complete_dicom_dataframe()


def create_complex_test_scenario() -> pd.DataFrame:
    """Create a complex test scenario with multiple acquisitions and types."""
    dfs = []
    
    # T1w anatomical
    dfs.append(create_t1w_data(acquisition="mprage"))
    
    # Single-echo GRE
    dfs.append(create_mag_phase_pair(acquisition="gre_single"))
    
    # Multi-echo GRE  
    dfs.append(create_multiecho_data(acquisition="gre_multi", num_echoes=3))
    
    # Real/Imag data
    dfs.append(create_real_imag_pair(acquisition="gre_complex"))
    
    return pd.concat(dfs, ignore_index=True)