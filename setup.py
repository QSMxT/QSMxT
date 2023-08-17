from setuptools import setup, find_packages

setup(
    name='qsmxt',
    version='4.0.0',
    packages=find_packages(),
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
        'packaging',
        'scikit-sparse @ git+https://github.com/scikit-sparse/scikit-sparse.git@v0.4.9'
    ],
    entry_points={
        'console_scripts': [
            'qsmxt = qsmxt.cli.qsmxt:main',
            'dicom-convert = qsmxt.cli.dicom_convert:main',
            'dicom-sort = qsmxt.cli.dicom_sort:main',
            'nifti-convert = qsmxt.cli.nifti_convert:main',
        ],
    },
    # other metadata
)
