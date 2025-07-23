"""Synthetic data generators for testing."""

import numpy as np
from typing import Tuple, Optional


def generate_brain_mask(shape: Tuple[int, int, int] = (64, 64, 32)) -> np.ndarray:
    """Generate realistic brain mask."""
    x, y, z = np.meshgrid(np.linspace(-1, 1, shape[0]),
                          np.linspace(-1, 1, shape[1]),
                          np.linspace(-1, 1, shape[2]))
    
    # Ellipsoid brain shape
    mask = (x**2/0.8**2 + y**2/0.8**2 + z**2/0.6**2) < 1
    return mask.astype(bool)


def generate_qsm_data(shape: Tuple[int, int, int] = (64, 64, 32),
                     susceptibility_range: Tuple[float, float] = (-0.2, 0.2)) -> np.ndarray:
    """Generate realistic QSM susceptibility values."""
    brain_mask = generate_brain_mask(shape)
    
    data = np.zeros(shape)
    
    # Different brain regions with different susceptibilities
    # Gray matter: slightly positive
    # White matter: slightly negative
    # Veins: highly positive (iron-rich)
    
    brain_indices = np.where(brain_mask)
    n_brain = len(brain_indices[0])
    
    # Most brain tissue
    data[brain_mask] = np.random.uniform(susceptibility_range[0]*0.5, 
                                       susceptibility_range[1]*0.5, n_brain)
    
    # Add some high-susceptibility regions (veins)
    n_veins = n_brain // 50
    vein_indices = np.random.choice(n_brain, n_veins, replace=False)
    for i in vein_indices:
        idx = (brain_indices[0][i], brain_indices[1][i], brain_indices[2][i])
        data[idx] = np.random.uniform(susceptibility_range[1]*0.8, 
                                    susceptibility_range[1])
    
    return data.astype(np.float32)


def generate_frequency_data(shape: Tuple[int, int, int] = (64, 64, 32),
                          B0: float = 3.0) -> np.ndarray:
    """Generate frequency data from susceptibility."""
    # This would implement forward modeling
    # For testing purposes, create realistic frequency patterns
    susceptibility = generate_qsm_data(shape)
    
    # Simplified dipole convolution (actual implementation would use FFT)
    # For testing, just scale susceptibility to realistic frequency values
    γ = 42.58e6  # Hz/T
    frequency = susceptibility * γ * B0 / (2*np.pi) * 1e6  # Convert to Hz
    
    return frequency.astype(np.float32)


def generate_phase_data_with_wrapping(shape: Tuple[int, int, int] = (64, 64, 32),
                                    TE: float = 0.02,  # 20ms
                                    B0: float = 3.0) -> np.ndarray:
    """Generate phase data with realistic wrapping patterns."""
    # Start with frequency data
    frequency = generate_frequency_data(shape, B0)
    
    # Convert to phase using TE
    phase = 2 * np.pi * frequency * TE
    
    # Add phase wrapping
    phase = (phase + np.pi) % (2*np.pi) - np.pi
    
    return phase.astype(np.float32)


def generate_magnitude_data(shape: Tuple[int, int, int] = (64, 64, 32),
                          snr: float = 20.0) -> np.ndarray:
    """Generate magnitude data with realistic signal characteristics."""
    brain_mask = generate_brain_mask(shape)
    
    data = np.zeros(shape)
    
    # Brain tissue has higher signal
    data[brain_mask] = np.random.normal(1000, 100, np.sum(brain_mask))
    # Background has low signal
    data[~brain_mask] = np.random.normal(50, 20, np.sum(~brain_mask))
    
    # Add noise based on SNR
    noise_std = np.mean(data[brain_mask]) / snr
    noise = np.random.normal(0, noise_std, shape)
    data = data + noise
    
    # Ensure non-negative (magnitude data)
    data = np.abs(data)
    
    return data.astype(np.float32)


def generate_bimodal_histogram_data(n_samples: int = 10000,
                                  mode1_params: Tuple[float, float] = (20, 5),
                                  mode2_params: Tuple[float, float] = (100, 15),
                                  mode1_weight: float = 0.6) -> np.ndarray:
    """Generate bimodal distribution for testing histogram-based algorithms."""
    n_mode1 = int(n_samples * mode1_weight)
    n_mode2 = n_samples - n_mode1
    
    mode1_data = np.random.normal(mode1_params[0], mode1_params[1], n_mode1)
    mode2_data = np.random.normal(mode2_params[0], mode2_params[1], n_mode2)
    
    return np.concatenate([mode1_data, mode2_data])