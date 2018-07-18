#!/usr/bin/env python3

import nipype_interface_tgv_qsm as tgv
# import nipype_interface_fsl_tgv as tgv
# import nipype.interfaces.fsl as fsl
# import nipype.interfaces.minc as minc
# from nipype.interfaces.minc.testdata import minc2Dfile
#
# minc.Math.help()
# maths = minc.Math(input_files=[minc2Dfile], scale=(3.0, 2))
# print(maths.cmdline)
#
# fsl.ImageMaths.help()
# maths = fsl.ImageMaths(in_file='test_phase.nii', op_string='-add 5', out_file='test_phase_add5.nii')
# print(maths.cmdline)
# maths.run()

tgv.QSMappingInterface.help()
qsm = tgv.QSMappingInterface(file_mask='test_mask.nii', file_phase='test_phase.nii', TE=0.004, b0=7, num_threads=9)
print(qsm.cmdline)
qsm.run()
