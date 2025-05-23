import os
import nibabel as nib
import numpy as np
import nilearn.image
import warnings
from qsmxt.scripts.qsmxt_functions import extend_fname
from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, File, traits

def resample_to_axial(mag_nii=None, pha_nii=None, mask_nii=None):
    # calculate base affine
    nii = mag_nii or pha_nii or mask_nii
    voxel_size = np.array(nii.header.get_zooms())
    resolution = np.array(nii.header.get_data_shape())
    origin = np.array(voxel_size * resolution / 2)
    base_affine = np.eye(4)
    np.fill_diagonal(base_affine, voxel_size * np.sign(np.diag(nii.affine))[:3])
    base_affine[3,3] = 1
    base_affine[:3,3] = origin * -np.sign(np.diag(nii.affine)[:3])
    base_affine = base_affine[:3,:3]

    mag_rot_nii = None
    pha_rot_nii = None
    mask_rot_nii = None

    # compute real and imaginary components from magnitude and phase
    if mag_nii and pha_nii:
        pha = pha_nii.get_fdata()
        mag = mag_nii.get_fdata()
        real = mag * np.cos(pha)
        imag = mag * np.sin(pha)
        cplx_header = mag_nii.header.copy()
        cplx_header.set_data_dtype(np.float32)
        real_nii = nib.Nifti1Image(real, affine=pha_nii.affine, header=cplx_header)
        imag_nii = nib.Nifti1Image(imag, affine=pha_nii.affine, header=cplx_header)

        # resample real and imaginary to base affine
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            real_rot_nii = nilearn.image.resample_img(real_nii, target_affine=base_affine, target_shape=None, interpolation='continuous')
            imag_rot_nii = nilearn.image.resample_img(imag_nii, target_affine=base_affine, target_shape=None, interpolation='continuous')
            mask_rot_nii = nilearn.image.resample_img(mask_nii, target_affine=base_affine, target_shape=None, interpolation='nearest') if mask_nii else None

        # convert real and imaginary to magnitude and phase
        real_rot = real_rot_nii.get_fdata()
        imag_rot = imag_rot_nii.get_fdata()
        mag_rot = np.array(np.round(np.hypot(real_rot, imag_rot, dtype=mag.dtype), 0), dtype=mag.dtype)
        pha_rot = np.arctan2(imag_rot, real_rot, dtype=np.float32)

        # add noise to zero values
        mask = pha_rot == 0
        if mask.sum() / mask.size >= 0.1:
            np.random.seed()
            noise = np.random.uniform(-np.pi, np.pi, pha_rot.shape)
            pha_rot[mask] = noise[mask]

        # create nifti objects
        mag_rot_nii = nib.Nifti1Image(mag_rot, affine=real_rot_nii.affine, header=mag_nii.header)
        pha_rot_nii = nib.Nifti1Image(pha_rot, affine=real_rot_nii.affine, header=pha_nii.header)
    elif mag_nii:
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            mag_rot_nii = nilearn.image.resample_img(mag_nii, target_affine=base_affine, target_shape=None, interpolation='continuous')
    if mask_nii:
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            mask_rot_nii = nilearn.image.resample_img(mask_nii, target_affine=base_affine, target_shape=None, interpolation='nearest')

    return mag_rot_nii, pha_rot_nii, mask_rot_nii

def resample_files(mag_file=None, pha_file=None, mask_file=None, obliquity_threshold=None):
    # load data
    mag_nii = nib.load(mag_file) if mag_file else None
    pha_nii = nib.load(pha_file) if pha_file else None
    mask_nii = nib.load(mask_file) if mask_file else None        

    # check obliquity
    nii = mag_nii or pha_nii or mask_nii
    obliquity = np.rad2deg(nib.affines.obliquity(nii.affine))
    obliquity_norm = np.linalg.norm(obliquity)
    if obliquity_threshold and obliquity_norm < obliquity_threshold:
        return mag_file, pha_file, mask_file

    # resample
    mag_rot_nii, pha_rot_nii, mask_rot_nii = resample_to_axial(mag_nii, pha_nii, mask_nii)
    
    # save results
    mag_resampled_fname = None
    pha_resampled_fname = None
    mask_resampled_fname = None
    if mag_rot_nii:
        mag_resampled_fname = extend_fname(mag_file, "_resampled", out_dir=os.getcwd())
        nib.save(mag_rot_nii, mag_resampled_fname)
    if pha_rot_nii:
        pha_resampled_fname = extend_fname(pha_file, "_resampled", out_dir=os.getcwd())
        nib.save(pha_rot_nii, pha_resampled_fname)
    if mask_rot_nii:
        mask_resampled_fname = extend_fname(mask_file, "_resampled", out_dir=os.getcwd())
        nib.save(mask_rot_nii, mask_resampled_fname)

    return mag_resampled_fname, pha_resampled_fname, mask_resampled_fname


def resample_like(in_file, in_like, interpolation='continuous'):
    in_nii = nib.load(in_file)
    in_like_nii = nib.load(in_like)
    if np.array_equal(in_nii.affine, in_like_nii.affine):
        return in_file
    in_nii_resampled = nilearn.image.resample_img(in_nii, target_affine=in_like_nii.affine, target_shape=np.array(in_like_nii.header.get_data_shape()), interpolation=interpolation)
    in_resampled_fname = extend_fname(in_file, "_resampled", out_dir=os.getcwd())
    nib.save(in_nii_resampled, in_resampled_fname)
    return in_resampled_fname


class AxialSamplingInputSpec(BaseInterfaceInputSpec):
    magnitude = File(mandatory=False, exists=True)
    phase = File(mandatory=False, exists=True)
    mask = File(mandatory=False, exists=True)
    obliquity_threshold = traits.Float(mandatory=False)


class AxialSamplingOutputSpec(TraitedSpec):
    magnitude = File(exists=False)
    phase = File(exists=False)
    mask = File(mandatory=False)


class AxialSamplingInterface(SimpleInterface):
    input_spec = AxialSamplingInputSpec
    output_spec = AxialSamplingOutputSpec

    def _run_interface(self, runtime):
        magnitude, phase, mask = resample_files(
            mag_file=self.inputs.magnitude,
            pha_file=self.inputs.phase,
            mask_file=self.inputs.mask,
            obliquity_threshold=self.inputs.obliquity_threshold
        )
        if magnitude: self._results['magnitude'] = magnitude
        if phase: self._results['phase'] = phase
        if mask: self._results['mask'] = mask
        
        return runtime


class ResampleLikeInputSpec(BaseInterfaceInputSpec):
    in_file = File(mandatory=True, exists=True)
    in_like = File(mandatory=True, exists=True)


class ResampleLikeOutputSpec(TraitedSpec):
    out_file = File(exists=True)


class ResampleLikeInterface(SimpleInterface):
    input_spec = ResampleLikeInputSpec
    output_spec = ResampleLikeOutputSpec

    def _run_interface(self, runtime):
        out_file = resample_like(self.inputs.in_file, self.inputs.in_like)
        self._results['out_file'] = out_file
        return runtime

