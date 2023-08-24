from setuptools import setup, find_packages

setup(
    name='qsmxt',
    long_description="QSMxT is an end-to-end software toolbox for Quantitative Susceptibility Mapping",
    version='4.0.1',
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
        'qsm-forward==0.15',
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
