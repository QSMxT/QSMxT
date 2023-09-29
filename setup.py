import os
from setuptools import setup, find_packages

def read_version_from_config():
    setup_file_path = os.path.abspath(__file__)
    setup_dir = os.path.dirname(setup_file_path)
    config_path = os.path.join(setup_dir, 'docs', '_config.yml')

    with open(config_path, 'r') as f:
        for line in f:
            if line.startswith('PACKAGE_VERSION:'):
                return line.split(":")[1].strip()

    raise ValueError('QSMxT version not found in docs/_config.yml!')

setup(
    name='qsmxt',
    long_description="QSMxT is an end-to-end software toolbox for Quantitative Susceptibility Mapping",
    version=read_version_from_config(),
    packages=find_packages(),
    package_dir={'qsmxt': 'qsmxt'},
    package_data={
        'qsmxt': ['aseg_labels.csv', 'qsm_pipelines.json', 'scripts/*.jl', 'scripts/*.py']
    },
    install_requires=[
        'psutil',
        'datetime',
        'networkx==2.8.8',
        'numpy',
        'h5py',
        'nibabel',
        'nilearn',
        'traits',
        'nipype',
        'scipy',
        'scikit-image',
        'pydicom',
        'pytest',
        'seaborn',
        'webdavclient3',
        'images-upload-cli',
        'qsm-forward==0.16',
        'osfclient',
        'niflow-nipype1-workflows',
        'tensorflow',
        'packaging'
    ],
    entry_points={
        'console_scripts': [
            'qsmxt = qsmxt.cli.main:main',
            'dicom-convert = qsmxt.cli.dicom_convert:main',
            'dicom-sort = qsmxt.cli.dicom_sort:main',
            'nifti-convert = qsmxt.cli.nifti_convert:main',
            'get-qsmxt-dir = qsmxt.scripts.qsmxt_functions:get_qsmxt_dir'
        ],
    },
)
