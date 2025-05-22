#!/usr/bin/env python3
import nibabel as nib
import numpy as np
import os
import hashlib

from nipype.interfaces.base import SimpleInterface, BaseInterfaceInputSpec, TraitedSpec, traits, File
from qsmxt.scripts.qsmxt_functions import extend_fname

def frequency_to_normalized(frequency_path, B0, scale_factor=1, out_path=None):
    # use scale_factor=1e6 for microradians (needed for nextqsm)
    # use scale_factor=1e6/(2*np.pi) for ppm (needed for most other algorithms)
    
    # load ΔB (Hz)
    frequency_nii = nib.load(frequency_path)
    ΔB = frequency_nii.get_fdata()
    
    # gyromagnetic ratio in Hz/T
    γ = 42.58*10**6

    # calculate normalized phase
    Φ_norm = (2*np.pi * ΔB) / (γ * B0) * scale_factor

    # save result
    out_path = out_path or extend_fname(frequency_path, f"_normalized", out_dir=os.getcwd())
    nib.save(img=nib.Nifti1Image(dataobj=Φ_norm, header=frequency_nii.header, affine=frequency_nii.affine), filename=out_path)
    return out_path


class FreqToNormalizedInputSpec(BaseInterfaceInputSpec):
    frequency = File(mandatory=True, exists=True)
    B0 = traits.Float(mandatory=True)
    scale_factor = traits.Float(mandatory=False, default_value=1)
    

class FreqToNormalizedOutputSpec(TraitedSpec):
    phase_normalized = File(mandatory=True, exists=True)


class FreqToNormalizedInterface(SimpleInterface):
    input_spec = FreqToNormalizedInputSpec
    output_spec = FreqToNormalizedOutputSpec

    def _run_interface(self, runtime):
        self._results['phase_normalized'] = frequency_to_normalized(
            frequency_path=self.inputs.frequency,
            B0=self.inputs.B0,
            scale_factor=self.inputs.scale_factor
        )
        return runtime


def frequency_to_phase(frequency_path, TE, wraps=False, out_path=None):
    # load ΔB (Hz)
    frequency_nii = nib.load(frequency_path)
    ΔB = frequency_nii.get_fdata()
    
    # phase accumulation at TE (rads)
    Φ_acc = 2*np.pi * ΔB * TE

    # add wraps if requested
    if wraps: Φ_acc = (Φ_acc + np.pi) % (2 * np.pi) - np.pi
    
    # save results
    out_path = out_path or extend_fname(frequency_path, f"_phase-TE{int(TE*1000):0>3}", out_dir=os.getcwd())
    nib.save(img=nib.Nifti1Image(dataobj=Φ_acc, header=frequency_nii.header, affine=frequency_nii.affine), filename=out_path)
    return out_path


class FreqToPhaseInputSpec(BaseInterfaceInputSpec):
    frequency = File(mandatory=True, exists=True)
    TE = traits.Float(mandatory=True)
    wraps = traits.Bool(default_value=False)
    

class FreqToPhaseOutputSpec(TraitedSpec):
    phase = File(mandatory=True, exists=True)


class FreqToPhaseInterface(SimpleInterface):
    input_spec = FreqToPhaseInputSpec
    output_spec = FreqToPhaseOutputSpec

    def _run_interface(self, runtime):
        self._results['phase'] = frequency_to_phase(
            frequency_path=self.inputs.frequency,
            TE=self.inputs.TE,
            wraps=self.inputs.wraps
        )
        return runtime


def phase_to_normalized(phase_path, B0, TE, scale_factor=1, out_path=None):
    # use scale_factor=1e6 for microradians (needed for nextqsm)
    # use scale_factor=1e6/(2*np.pi) for ??? (needed for rts, tv, etc.)

    # load phase
    phase_nii = nib.load(phase_path)
    Φ_acc = phase_nii.get_fdata()

    # gyromagnetic ratio in Hz/T
    γ = 42.58*10**6

    # calculate normalized phase
    Φ_norm = Φ_acc / (TE * γ * B0) * scale_factor

    # save result
    out_path = out_path or extend_fname(phase_path, f"_normalized", out_dir=os.getcwd())
    nib.save(img=nib.Nifti1Image(dataobj=Φ_norm, header=phase_nii.header, affine=phase_nii.affine), filename=out_path)

    return out_path


class PhaseToNormalizedInputSpec(BaseInterfaceInputSpec):
    phase = File(mandatory=True, exists=True)
    B0 = traits.Float(mandatory=False, default_value=3)
    TE = traits.Float(mandatory=True)
    scale_factor = traits.Float(mandatory=False, default_value=1)
    

class PhaseToNormalizedOutputSpec(TraitedSpec):
    phase_normalized = File(mandatory=True, exists=True)


class PhaseToNormalizedInterface(SimpleInterface):
    input_spec = PhaseToNormalizedInputSpec
    output_spec = PhaseToNormalizedOutputSpec

    def _run_interface(self, runtime):
        self._results['phase_normalized'] = phase_to_normalized(
            phase_path=self.inputs.phase,
            B0=self.inputs.B0,
            TE=self.inputs.TE,
            scale_factor=self.inputs.scale_factor
        )
        return runtime

def seed_from_filename(filename):
    hash_obj = hashlib.md5(filename.encode('utf-8'))
    hash_num = int(hash_obj.hexdigest(), 16)
    seed = hash_num % (2**32)  # Make seed 32-bit integer.
    return seed

def scale_to_pi(phase_path, phase_scaled_path=None):
    # load input phase
    phase_nii = nib.load(phase_path)
    Φ_acc_wrapped = phase_nii.get_fdata()

    # replace nans with 0
    changed = False
    if np.isnan(Φ_acc_wrapped).any():
        changed = True
        Φ_acc_wrapped[np.isnan(Φ_acc_wrapped)] = 0

    # if a significant portion is close to 0 or pi, replace with random values
    mask = np.logical_or((abs(abs(Φ_acc_wrapped) - np.pi)) < 1e-3, Φ_acc_wrapped == 0)
    if mask.sum() / mask.size >= 0.1:
        seed_value = seed_from_filename(phase_path)
        np.random.seed(seed_value)
        noise = np.random.uniform(-np.pi, np.pi, Φ_acc_wrapped.shape)
        Φ_acc_wrapped[mask] = noise[mask]
        changed = True
    
    # return the original if it is already scaled correctly
    if not changed and (np.round(np.min(Φ_acc_wrapped), 2)*-1) == np.round(np.max(Φ_acc_wrapped), 2) == 3.14:
        return phase_path
    
    # scale to -pi,+pi
    Φ_acc_wrapped_scaled = np.array(np.interp(Φ_acc_wrapped, (Φ_acc_wrapped.min(), Φ_acc_wrapped.max()), (-np.pi, +np.pi)), dtype=Φ_acc_wrapped.dtype)

    # save result
    phase_scaled_path = phase_scaled_path or extend_fname(phase_path, "_scaled", out_dir=os.getcwd())
    nib.save(nib.Nifti1Image(dataobj=Φ_acc_wrapped_scaled, header=phase_nii.header, affine=phase_nii.affine), phase_scaled_path)
    return phase_scaled_path


class ScalePhaseInputSpec(BaseInterfaceInputSpec):
    phase = File(mandatory=True, exists=True)
    

class ScalePhaseOutputSpec(TraitedSpec):
    phase = File(mandatory=True, exists=True)


class ScalePhaseInterface(SimpleInterface):
    input_spec = ScalePhaseInputSpec
    output_spec = ScalePhaseOutputSpec

    def _run_interface(self, runtime):
        self._results['phase'] = scale_to_pi(self.inputs.phase)
        return runtime


