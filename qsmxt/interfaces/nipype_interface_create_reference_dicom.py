#!/usr/bin/env python3

import json
import os
import re
import tempfile
from datetime import datetime
import pydicom
from pydicom.dataset import Dataset, FileDataset
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, traits, File

class CreateReferenceDicomInputSpec(BaseInterfaceInputSpec):
    source_json = File(
        mandatory=True, 
        exists=True, 
        desc="Source JSON sidecar file with metadata"
    )
    subject_id = traits.Str(
        mandatory=True,
        desc="BIDS subject ID (e.g., 'sub-001')"
    )
    session_id = traits.Str(
        mandatory=False,
        desc="BIDS session ID (e.g., 'ses-20240403')"
    )
    image_type_suffix = traits.List(
        traits.Str(),
        mandatory=False,
        desc="Additional ImageType values to append (e.g., ['QSM'], ['SWI'])"
    )
    series_description_suffix = traits.Str(
        mandatory=False,
        desc="Suffix to append to SeriesDescription (e.g., '_QSM')"
    )

class CreateReferenceDicomOutputSpec(TraitedSpec):
    reference_dicom = File(
        exists=True,
        desc="Output reference DICOM file"
    )

class CreateReferenceDicomInterface(SimpleInterface):
    input_spec = CreateReferenceDicomInputSpec
    output_spec = CreateReferenceDicomOutputSpec
    
    def _parse_session_date(self, session_id):
        """Extract date from BIDS session ID if it matches YYYYMMDD format."""
        if not session_id:
            return None
            
        # Try to extract 8-digit date from session ID
        match = re.search(r'(\d{8})', session_id)
        if match:
            date_str = match.group(1)
            # Validate it's a real date
            try:
                datetime.strptime(date_str, '%Y%m%d')
                return date_str
            except ValueError:
                pass
        return None
    
    def _get_date_from_json(self, json_data, field_names):
        """Get the first available date from JSON using fallback field names."""
        for field in field_names:
            if field in json_data:
                date_val = json_data[field]
                # Handle different date formats
                if isinstance(date_val, str):
                    # Already in DICOM format (YYYYMMDD)
                    if len(date_val) == 8 and date_val.isdigit():
                        return date_val
                    # ISO format (YYYY-MM-DD)
                    elif '-' in date_val:
                        try:
                            dt = datetime.strptime(date_val[:10], '%Y-%m-%d')
                            return dt.strftime('%Y%m%d')
                        except ValueError:
                            pass
        return None
    
    def _get_time_from_json(self, json_data, field_names):
        """Get the first available time from JSON using fallback field names."""
        for field in field_names:
            if field in json_data:
                time_val = str(json_data[field])
                # Remove any decimal seconds and format as HHMMSS
                if '.' in time_val:
                    time_val = time_val.split('.')[0]
                # Remove colons if present
                time_val = time_val.replace(':', '')
                # Ensure it's 6 digits
                if len(time_val) >= 6:
                    return time_val[:6]
        return None

    def _run_interface(self, runtime):
        # Read the source JSON file
        with open(self.inputs.source_json, 'r', encoding='utf-8') as f:
            json_data = json.load(f)
        
        # Create a minimal DICOM dataset
        file_meta = Dataset()
        file_meta.MediaStorageSOPClassUID = '1.2.840.10008.5.1.4.1.1.4'  # MR Image Storage
        file_meta.MediaStorageSOPInstanceUID = pydicom.uid.generate_uid()
        file_meta.ImplementationClassUID = pydicom.uid.generate_uid()
        file_meta.TransferSyntaxUID = pydicom.uid.ExplicitVRLittleEndian
        
        # Create the FileDataset
        ds = FileDataset(None, {}, file_meta=file_meta, preamble=b"\0" * 128)
        
        # Patient Information - Use BIDS IDs
        ds.PatientName = self.inputs.subject_id
        ds.PatientID = self.inputs.subject_id
        
        # Try to get patient info from JSON if available
        if 'PatientSex' in json_data:
            ds.PatientSex = json_data['PatientSex']
        else:
            ds.PatientSex = ''
            
        if 'PatientBirthDate' in json_data:
            ds.PatientBirthDate = json_data['PatientBirthDate']
        else:
            ds.PatientBirthDate = ''
        
        # Date handling with smart fallbacks
        study_date = None
        series_date = None
        
        # First try to parse date from session ID
        if self.inputs.session_id:
            session_date = self._parse_session_date(self.inputs.session_id)
            if session_date:
                study_date = session_date
                series_date = session_date
        
        # Fallback to JSON dates if needed
        if not study_date:
            study_date = self._get_date_from_json(
                json_data, 
                ['StudyDate', 'AcquisitionDate', 'SeriesDate']
            )
        if not series_date:
            series_date = self._get_date_from_json(
                json_data,
                ['SeriesDate', 'AcquisitionDate', 'StudyDate']
            )
        
        # Set dates (use empty string if not found)
        ds.StudyDate = study_date or ''
        ds.SeriesDate = series_date or ''
        ds.AcquisitionDate = series_date or ''
        
        # Time fields from JSON
        ds.StudyTime = self._get_time_from_json(
            json_data,
            ['StudyTime', 'AcquisitionTime', 'SeriesTime']
        ) or ''
        ds.SeriesTime = self._get_time_from_json(
            json_data,
            ['SeriesTime', 'AcquisitionTime', 'StudyTime']
        ) or ''
        ds.AcquisitionTime = self._get_time_from_json(
            json_data,
            ['AcquisitionTime', 'SeriesTime', 'StudyTime']
        ) or ''
        
        # Study/Series Information
        ds.StudyInstanceUID = pydicom.uid.generate_uid()
        ds.SeriesInstanceUID = pydicom.uid.generate_uid()
        ds.SOPInstanceUID = pydicom.uid.generate_uid()
        ds.SOPClassUID = '1.2.840.10008.5.1.4.1.1.4'  # MR Image Storage
        
        # Copy relevant fields from JSON using correct DICOM tag names
        field_mapping = {
            'Modality': 'MR',  # Default to MR
            'Manufacturer': '',
            'DeviceSerialNumber': '',
            'StationName': '',
            'InstitutionName': '',
            'InstitutionalDepartmentName': '',
            'InstitutionAddress': '',
            'MagneticFieldStrength': None,
            'ImagingFrequency': None,
            'EchoTime': None,
            'RepetitionTime': None,
            'FlipAngle': None,
            'SliceThickness': None,
            'PixelSpacing': None,
            'SeriesNumber': 1,
            'InstanceNumber': 1,
            'StudyDescription': '',
            'SeriesDescription': '',
            'ProtocolName': '',
            'BodyPartExamined': 'BRAIN',
            'PatientPosition': '',
            'MRAcquisitionType': '',
            'ScanningSequence': '',
            'SequenceVariant': '',
            'ScanOptions': '',
            'SequenceName': '',
            'SoftwareVersions': '',
            'ReceiveCoilName': '',
            'EchoTrainLength': None,
            'PixelBandwidth': None,
            'SAR': None,
        }
        
        # Handle fields that need name conversion from JSON to DICOM
        json_to_dicom_mapping = {
            'ProcedureStepDescription': 'PerformedProcedureStepDescription',
            'PercentPhaseFOV': 'PercentPhaseFieldOfView', 
            'PhaseEncodingDirection': 'InPlanePhaseEncodingDirection',
            'PhaseEncodingSteps': 'NumberOfPhaseEncodingSteps',
            'AcquisitionMatrixPE': 'AcquisitionMatrix',
            'PercentSampling': 'PercentSampling',  # This is correct
        }
        
        # Handle ManufacturersModelName -> ManufacturerModelName conversion
        if 'ManufacturersModelName' in json_data:
            ds.ManufacturerModelName = json_data['ManufacturersModelName']
        elif 'ManufacturerModelName' in json_data:
            ds.ManufacturerModelName = json_data['ManufacturerModelName']
        
        # Handle ContentDate (should be same as AcquisitionDate)
        if 'ContentDate' in json_data:
            ds.ContentDate = json_data['ContentDate']
        elif ds.AcquisitionDate:
            ds.ContentDate = ds.AcquisitionDate
        
        # Handle ContentTime (should be same as AcquisitionTime)
        if 'ContentTime' in json_data:
            ds.ContentTime = json_data['ContentTime']
        elif ds.AcquisitionTime:
            ds.ContentTime = ds.AcquisitionTime
        
        # Copy standard fields with proper data type handling
        for dicom_field, default_value in field_mapping.items():
            if dicom_field in json_data:
                value = json_data[dicom_field]
                # Handle float values for fields that expect strings
                if dicom_field in ['PartialFourier', 'PhaseResolution'] and isinstance(value, float):
                    value = str(value)
                setattr(ds, dicom_field, value)
            elif default_value is not None:
                setattr(ds, dicom_field, default_value)
        
        # Handle fields with name conversion
        for json_field, dicom_field in json_to_dicom_mapping.items():
            if json_field in json_data:
                value = json_data[json_field]
                # Convert to string if needed for certain fields
                if dicom_field == 'InPlanePhaseEncodingDirection' and value in ['i', 'j', 'k']:
                    # Map phase encoding direction to DICOM values
                    direction_map = {'i': 'ROW', 'j': 'COL', 'k': 'SLC'}
                    value = direction_map.get(value, str(value))
                elif isinstance(value, (int, float)) and dicom_field in ['NumberOfPhaseEncodingSteps']:
                    value = int(value)
                setattr(ds, dicom_field, value)
        
        # Handle ImageOrientationPatient specially (it's an array)
        if 'ImageOrientationPatientDICOM' in json_data:
            ds.ImageOrientationPatient = json_data['ImageOrientationPatientDICOM']
        
        # Handle some additional multi-echo specific fields if present
        if 'EchoTime1' in json_data:
            # This might be useful for multi-echo sequences
            ds.EchoTime = json_data['EchoTime1']  # Use first echo time as primary
        elif 'EchoTime2' in json_data and not hasattr(ds, 'EchoTime'):
            ds.EchoTime = json_data['EchoTime2']
        
        # Note: Vendor-specific fields like ShimSetting, RefLinesPE, CoilCombinationMethod
        # are skipped as they require private DICOM tags and can cause compatibility issues
        
        # Handle ImageType specially
        image_type = []
        if 'ImageType' in json_data:
            if isinstance(json_data['ImageType'], list):
                image_type = json_data['ImageType'].copy()
            else:
                image_type = [str(json_data['ImageType'])]
        
        # Add processing-specific ImageType values
        if self.inputs.image_type_suffix:
            # Mark as derived/secondary
            if 'ORIGINAL' in image_type:
                image_type.remove('ORIGINAL')
            if 'PRIMARY' in image_type:
                image_type.remove('PRIMARY')
            if 'DERIVED' not in image_type:
                image_type.insert(0, 'DERIVED')
            if 'SECONDARY' not in image_type:
                image_type.insert(1, 'SECONDARY')
            
            # Add specific processing type
            for suffix in self.inputs.image_type_suffix:
                if suffix not in image_type:
                    image_type.append(suffix)
        
        ds.ImageType = image_type
        
        # Update SeriesDescription with suffix if provided
        if self.inputs.series_description_suffix and hasattr(ds, 'SeriesDescription'):
            ds.SeriesDescription = str(ds.SeriesDescription) + self.inputs.series_description_suffix
        
        # Set required DICOM fields for a valid image
        ds.Rows = 1
        ds.Columns = 1
        ds.BitsAllocated = 16
        ds.BitsStored = 16
        ds.HighBit = 15
        ds.PixelRepresentation = 1  # Signed
        ds.SamplesPerPixel = 1
        ds.PhotometricInterpretation = 'MONOCHROME2'
        
        # Add a minimal pixel array (1x1)
        import numpy as np
        ds.PixelData = np.zeros((1, 1), dtype=np.int16).tobytes()
        
        # Save the reference DICOM to a temporary file
        output_dir = os.path.join(tempfile.gettempdir(), 'qsmxt_reference_dicoms')
        os.makedirs(output_dir, exist_ok=True)
        
        timestamp = datetime.now().strftime('%Y%m%d_%H%M%S')
        output_file = os.path.join(output_dir, f'ref_dicom_{self.inputs.subject_id}_{timestamp}.dcm')
        
        ds.save_as(output_file, write_like_original=False)
        
        self._results['reference_dicom'] = output_file
        return runtime