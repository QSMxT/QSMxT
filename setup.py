import os
from setuptools import setup, find_packages

def read_version_from_config():
    setup_file_path = os.path.abspath(__file__)
    setup_dir = os.path.dirname(setup_file_path)
    config_path = os.path.join(setup_dir, 'docs', '_config.yml')
    REQUIRED_VERSION_TYPE = os.environ.get('REQUIRED_VERSION_TYPE') or 'DEPLOY_PACKAGE_VERSION'
    with open(config_path, 'r') as f:
        for line in f:
            if line.startswith(REQUIRED_VERSION_TYPE):
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
        'psutil==6.1.0',
        'datetime==5.5',
        'networkx==2.8.8',
        'numpy==1.24.3',
        'h5py==3.11.0',
        'nibabel==5.2.1',
        'nilearn==0.10.4',
        'traits==6.3.2',
        'nipype==1.8.6',
        'scipy==1.10.1',
        'scikit-image==0.21.0',
        'pydicom==2.4.4',
        'seaborn==0.13.2',
        'webdavclient3==3.14.6',
        'images-upload-cli==1.1.3',
        'qsm-forward==0.22',
        'osfclient==0.0.5',
        'niflow-nipype1-workflows==0.0.5',
        'tensorflow==2.13.1',
        'packaging==24.1',
        'nextqsm==1.0.4',
        'matplotlib==3.7.5',
        'pandas==2.0.3',
        'dicompare==0.1.31'
    ],
    extras_require={
        'dev': [
            'pytest>=7.0.0',
            'pytest-mock>=3.10.0',
            'pytest-cov>=4.0.0',
            'pytest-xdist>=3.0.0',
            'black>=22.0.0',
            'isort>=5.10.0',
            'flake8>=5.0.0',
        ],
        'test': [
            'pytest>=7.0.0',
            'pytest-mock>=3.10.0',
            'pytest-cov>=4.0.0',
            'pytest-xdist>=3.0.0',
        ]
    },
    entry_points={
        'console_scripts': [
            'qsmxt = qsmxt.cli.main:main',
            'dicom-convert = qsmxt.cli.dicom_convert:main',
            'nifti-convert = qsmxt.cli.nifti_convert:main',
            'get-qsmxt-dir = qsmxt.scripts.qsmxt_functions:get_qsmxt_dir'
        ],
    },
)
