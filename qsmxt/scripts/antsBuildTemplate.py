# -*- coding: utf-8 -*-
#################################################################################
# Program:   Build Template Parallel
# Language:  Python
##
# Authors:  Jessica Forbes, Grace Murray, and Hans Johnson, University of Iowa
##
# This software is distributed WITHOUT ANY WARRANTY; without even
# the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR
# PURPOSE.
##
#################################################################################
from __future__ import print_function
from builtins import map
from builtins import zip
from builtins import range

from nipype.pipeline import engine as pe
from nipype.interfaces import utility as util
from nipype.interfaces.utility import Function

from nipype.interfaces.ants import (ANTS, WarpImageMultiTransform, AverageImages,
                                    MultiplyImages, AverageAffineTransform)


def GetFirstListElement(this_list):
    return this_list[0]


def MakeTransformListWithGradientWarps(averageAffineTranform,
                                       gradientStepWarp):
    return [
        averageAffineTranform, gradientStepWarp, gradientStepWarp,
        gradientStepWarp, gradientStepWarp
    ]


def RenestDeformedPassiveImages(deformedPassiveImages,
                                flattened_image_nametypes):
    import os
    """ Now make a list of lists of images where the outter list is per image type,
    and the inner list is the same size as the number of subjects to be averaged.
    In this case, the first element will be a list of all the deformed T2's, and
    the second element will be a list of all deformed POSTERIOR_AIR,  etc..
    """
    all_images_size = len(deformedPassiveImages)
    image_dictionary_of_lists = dict()
    nested_imagetype_list = list()
    outputAverageImageName_list = list()
    image_type_list = list()
    # make empty_list, this is not efficient, but it works
    for name in flattened_image_nametypes:
        image_dictionary_of_lists[name] = list()
    for index in range(0, all_images_size):
        curr_name = flattened_image_nametypes[index]
        curr_file = deformedPassiveImages[index]
        image_dictionary_of_lists[curr_name].append(curr_file)
    for image_type, image_list in list(image_dictionary_of_lists.items()):
        nested_imagetype_list.append(image_list)
        outputAverageImageName_list.append('AVG_' + image_type + '.nii.gz')
        image_type_list.append('WARP_AVG_' + image_type)
    print("\n" * 10)
    print("HACK: ", nested_imagetype_list)
    print("HACK: ", outputAverageImageName_list)
    print("HACK: ", image_type_list)
    return nested_imagetype_list, outputAverageImageName_list, image_type_list


# Utility Function
# This will make a list of list pairs for defining the concatenation of transforms
# wp=['wp1.nii','wp2.nii','wp3.nii']
# af=['af1.mat','af2.mat','af3.mat']
# ll=map(list,zip(af,wp))
# ll
# #[['af1.mat', 'wp1.nii'], ['af2.mat', 'wp2.nii'], ['af3.mat', 'wp3.nii']]


def MakeListsOfTransformLists(warpTransformList, AffineTransformList):
    return list(map(list, list(zip(warpTransformList, AffineTransformList))))


# Flatten and return equal length transform and images lists.


def FlattenTransformAndImagesList(ListOfPassiveImagesDictionaries,
                                  transformation_series):
    import sys
    import os
    import re
    import glob

    print("HACK:  DEBUG: ListOfPassiveImagesDictionaries\n{lpi}\n".format(
        lpi=ListOfPassiveImagesDictionaries))

    def extract_bids_entities_from_qsm_path(qsm_path):
        """Extract BIDS entities from QSM output path."""
        # Extract subject and session from path
        subject_match = re.search(r'/sub-([^/]+)/', qsm_path)
        session_match = re.search(r'/ses-([^/]+)/', qsm_path)

        subject = f"sub-{subject_match.group(1)}" if subject_match else None
        session = f"ses-{session_match.group(1)}" if session_match else None

        # Extract entities from the QSM directory name
        qsm_dir_match = re.search(r'/qsm_([^/]+)/', qsm_path)
        if not qsm_dir_match:
            return None

        qsm_dir = qsm_dir_match.group(1)

        # Parse entities from the QSM directory name
        acq = re.search(r'acq-([^_]+)', qsm_dir).group(1) if 'acq-' in qsm_dir else None
        rec = re.search(r'rec-([^_]+)', qsm_dir).group(1) if 'rec-' in qsm_dir else None
        inv = re.search(r'inv-([^_]+)', qsm_dir).group(1) if 'inv-' in qsm_dir else None
        run = re.search(r'run-([^_]+)', qsm_dir).group(1) if 'run-' in qsm_dir else None

        # If entities not found in directory name, try extracting from the filename
        filename = os.path.basename(qsm_path)
        if not acq:
            acq_match = re.search(r'acq-([^_]+)', filename)
            if acq_match:
                acq = acq_match.group(1)
        if not rec:
            rec_match = re.search(r'rec-([^_]+)', filename)
            if rec_match:
                rec = rec_match.group(1)
        if not inv:
            inv_match = re.search(r'inv-([^_]+)', filename)
            if inv_match:
                inv = inv_match.group(1)
        if not run:
            run_match = re.search(r'run-([^_]+)', filename)
            if run_match:
                run = run_match.group(1)

        # Extract suffix - it's always the last underscore-separated part of qsm_dir
        suffix = qsm_dir.split('_')[-1]

        return {
            'subject': subject,
            'session': session,
            'acq': acq,
            'rec': rec,
            'inv': inv,
            'suffix': suffix,
            'run': run
        }

    def infer_bids_dir_from_qsm_path(qsm_path):
        """Infer BIDS directory from QSM output path."""
        # QSM path looks like: /path/to/bids/derivatives/workflow/qsmxt-workflow/sub-X/ses-Y/qsm_...
        # We need to find the BIDS root directory

        # Look for the pattern that indicates we're in derivatives
        derivatives_match = re.search(r'(.+)/derivatives/', qsm_path)
        if derivatives_match:
            return derivatives_match.group(1)

        # Fallback: look for the parent directory of sub-* directories
        subject_match = re.search(r'(.+)/sub-[^/]+/', qsm_path)
        if subject_match:
            # Go up to find the actual BIDS directory
            potential_bids = subject_match.group(1)
            # If it contains 'derivatives', go up further
            while 'derivatives' in potential_bids or 'workflow' in potential_bids:
                potential_bids = os.path.dirname(potential_bids)
            return potential_bids

        return None

    def find_corresponding_magnitude_file(bids_dir, entities):
        """Find magnitude file that matches the BIDS entities from QSM."""
        subject = entities['subject']
        session = entities['session']
        acq = entities['acq']
        rec = entities['rec']
        inv = entities['inv']
        suffix = entities['suffix']
        run = entities['run']

        # Build the search path
        base_path = os.path.join(bids_dir, subject)
        if session:
            base_path = os.path.join(base_path, session)
        search_path = os.path.join(base_path, "anat")

        # Build required entity substrings to filter by
        required_entities = []
        if acq:
            required_entities.append(f"_acq-{acq}_")
        if rec:
            required_entities.append(f"_rec-{rec}_")
        if run:
            required_entities.append(f"_run-{run}_")
        if inv:
            required_entities.append(f"_inv-{inv}_")

        # Get all files matching the suffix
        all_files = glob.glob(os.path.join(search_path, f"*_{suffix}.nii*"))

        # Filter to magnitude files (part-mag or no part entity)
        mag_files = [f for f in all_files if '_part-mag_' in f or '_part-' not in f]

        # Filter files that contain all required entities
        matching_files = []
        for filepath in mag_files:
            filename = os.path.basename(filepath)
            if all(entity in filename for entity in required_entities):
                matching_files.append(filepath)

        if matching_files:
            return sorted(matching_files)[0]

        return None

    def build_magnitude_qsm_pairs(qsm_files, bids_dir):
        """Build list of (magnitude_file, qsm_file) pairs that match exactly."""
        pairs = []
        failed_matches = []

        for qsm_file in qsm_files:
            # Extract BIDS entities from QSM path
            entities = extract_bids_entities_from_qsm_path(qsm_file)

            if not entities:
                failed_matches.append((qsm_file, "Could not parse BIDS entities"))
                continue

            # Find corresponding magnitude file
            mag_file = find_corresponding_magnitude_file(bids_dir, entities)

            if mag_file:
                pairs.append((mag_file, qsm_file, entities))
            else:
                failed_matches.append((qsm_file, f"No magnitude file found for entities: {entities}"))

        return pairs, failed_matches

    # Enhanced BIDS entity matching logic
    print("=== ENHANCED FLATTEN FUNCTION ===")
    print(f"Input: {len(ListOfPassiveImagesDictionaries)} QSM files, {len(transformation_series)} transforms")

    # Extract QSM file paths
    qsm_files = [qsm_dict['QSM'] for qsm_dict in ListOfPassiveImagesDictionaries]

    # Infer BIDS directory from the first QSM file path
    if qsm_files:
        bids_dir = infer_bids_dir_from_qsm_path(qsm_files[0])
        print(f"Inferred BIDS directory: {bids_dir}")
    else:
        print("ERROR: No QSM files provided")
        sys.exit(-1)

    if not bids_dir:
        print("ERROR: Could not infer BIDS directory from QSM paths")
        sys.exit(-1)

    # Build magnitude-QSM pairs using BIDS entity matching
    pairs, failed_matches = build_magnitude_qsm_pairs(qsm_files, bids_dir)

    print(f"Successfully matched: {len(pairs)} QSM files to magnitude files")
    print(f"Failed to match: {len(failed_matches)} QSM files")

    if failed_matches:
        print("Failed matches:")
        for qsm_file, reason in failed_matches:
            print(f"  {os.path.basename(qsm_file)}: {reason}")

    # Check if we have enough transforms for the matched pairs
    num_transforms = len(transformation_series)
    num_matched = len(pairs)

    if num_matched > num_transforms:
        print(f"WARNING: {num_matched} matched pairs but only {num_transforms} transforms")
        print(f"Using first {num_transforms} matched pairs")
        pairs = pairs[:num_transforms]
    elif num_matched < num_transforms:
        print(f"WARNING: {num_matched} matched pairs but {num_transforms} transforms")
        print(f"Using first {num_matched} transforms")
        transformation_series = transformation_series[:num_matched]

    # Build the flattened lists using only matched pairs
    flattened_images = []
    flattened_image_nametypes = []
    flattened_transforms = []

    for i, (mag_file, qsm_file, entities) in enumerate(pairs):
        # Use the QSM file for template building
        flattened_images.append(qsm_file)
        flattened_image_nametypes.append('QSM')

        # Use corresponding transform
        if i < len(transformation_series):
            flattened_transforms.append(transformation_series[i])

    print(f"Final output: {len(flattened_images)} images, {len(flattened_transforms)} transforms")
    print("Successfully processed with BIDS entity matching!")

    print("HACK: flattened images    {0}\n".format(flattened_images))
    print("HACK: flattened nametypes {0}\n".format(flattened_image_nametypes))
    print("HACK: flattened txfms     {0}\n".format(flattened_transforms))

    return flattened_images, flattened_transforms, flattened_image_nametypes


def ANTSTemplateBuildSingleIterationWF(iterationPhasePrefix=''):
    """

    Inputs::

           inputspec.images :
           inputspec.fixed_image :
           inputspec.ListOfPassiveImagesDictionaries :

    Outputs::

           outputspec.template :
           outputspec.transforms_list :
           outputspec.passive_deformed_templates :
    """

    TemplateBuildSingleIterationWF = pe.Workflow(
        name='ANTSTemplateBuildSingleIterationWF_' +
        str(str(iterationPhasePrefix)))

    inputSpec = pe.Node(
        interface=util.IdentityInterface(fields=[
            'images', 'fixed_image', 'ListOfPassiveImagesDictionaries'
        ]),
        run_without_submitting=True,
        name='inputspec')
    # HACK: TODO: Need to move all local functions to a common untility file, or at the top of the file so that
    # they do not change due to re-indenting.  Otherwise re-indenting for flow control will trigger
    # their hash to change.
    # HACK: TODO: REMOVE 'transforms_list' it is not used.  That will change all the hashes
    # HACK: TODO: Need to run all python files through the code beutifiers.  It has gotten pretty ugly.
    outputSpec = pe.Node(
        interface=util.IdentityInterface(fields=[
            'template', 'transforms_list', 'passive_deformed_templates', 'flattened_transforms', 'wimtPassivedeformed'
        ]),
        run_without_submitting=True,
        name='outputspec')

    # NOTE MAP NODE! warp each of the original images to the provided fixed_image as the template
    BeginANTS = pe.MapNode(
        interface=ANTS(), name='BeginANTS', iterfield=['moving_image'])
    BeginANTS.inputs.dimension = 3
    BeginANTS.inputs.output_transform_prefix = str(
        iterationPhasePrefix) + '_tfm'
    BeginANTS.inputs.metric = ['CC']
    BeginANTS.inputs.metric_weight = [1.0]
    BeginANTS.inputs.radius = [5]
    BeginANTS.inputs.transformation_model = 'SyN'
    BeginANTS.inputs.gradient_step_length = 0.25
    BeginANTS.inputs.number_of_iterations = [50, 35, 15]
    BeginANTS.inputs.number_of_affine_iterations = [
        10000, 10000, 10000, 10000, 10000
    ]
    BeginANTS.inputs.use_histogram_matching = True
    BeginANTS.inputs.mi_option = [32, 16000]
    BeginANTS.inputs.regularization = 'Gauss'
    BeginANTS.inputs.regularization_gradient_field_sigma = 3
    BeginANTS.inputs.regularization_deformation_field_sigma = 0
    TemplateBuildSingleIterationWF.connect(inputSpec, 'images', BeginANTS,
                                           'moving_image')
    TemplateBuildSingleIterationWF.connect(inputSpec, 'fixed_image', BeginANTS,
                                           'fixed_image')

    MakeTransformsLists = pe.Node(
        interface=util.Function(
            function=MakeListsOfTransformLists,
            input_names=['warpTransformList', 'AffineTransformList'],
            output_names=['out']),
        run_without_submitting=True,
        name='MakeTransformsLists')
    MakeTransformsLists.interface.ignore_exception = True
    TemplateBuildSingleIterationWF.connect(
        BeginANTS, 'warp_transform', MakeTransformsLists, 'warpTransformList')
    TemplateBuildSingleIterationWF.connect(BeginANTS, 'affine_transform',
                                           MakeTransformsLists,
                                           'AffineTransformList')

    # Now warp all the input_images images
    wimtdeformed = pe.MapNode(
        interface=WarpImageMultiTransform(),
        iterfield=['transformation_series', 'input_image'],
        name='wimtdeformed')
    TemplateBuildSingleIterationWF.connect(inputSpec, 'images', wimtdeformed,
                                           'input_image')
    TemplateBuildSingleIterationWF.connect(
        MakeTransformsLists, 'out', wimtdeformed, 'transformation_series')

    # Shape Update Next =====
    # Now  Average All input_images deformed images together to create an updated template average
    AvgDeformedImages = pe.Node(
        interface=AverageImages(), name='AvgDeformedImages')
    AvgDeformedImages.inputs.dimension = 3
    AvgDeformedImages.inputs.output_average_image = str(
        iterationPhasePrefix) + '.nii.gz'
    AvgDeformedImages.inputs.normalize = True
    TemplateBuildSingleIterationWF.connect(wimtdeformed, "output_image",
                                           AvgDeformedImages, 'images')

    # Now average all affine transforms together
    AvgAffineTransform = pe.Node(
        interface=AverageAffineTransform(), name='AvgAffineTransform')
    AvgAffineTransform.inputs.dimension = 3
    AvgAffineTransform.inputs.output_affine_transform = 'Avererage_' + str(
        iterationPhasePrefix) + '_Affine.mat'
    TemplateBuildSingleIterationWF.connect(BeginANTS, 'affine_transform',
                                           AvgAffineTransform, 'transforms')

    # Now average the warp fields togther
    AvgWarpImages = pe.Node(interface=AverageImages(), name='AvgWarpImages')
    AvgWarpImages.inputs.dimension = 3
    AvgWarpImages.inputs.output_average_image = str(
        iterationPhasePrefix) + 'warp.nii.gz'
    AvgWarpImages.inputs.normalize = True
    TemplateBuildSingleIterationWF.connect(BeginANTS, 'warp_transform',
                                           AvgWarpImages, 'images')

    # Now average the images together
    # TODO:  For now GradientStep is set to 0.25 as a hard coded default value.
    GradientStep = 0.25
    GradientStepWarpImage = pe.Node(
        interface=MultiplyImages(), name='GradientStepWarpImage')
    GradientStepWarpImage.inputs.dimension = 3
    GradientStepWarpImage.inputs.second_input = -1.0 * GradientStep
    GradientStepWarpImage.inputs.output_product_image = 'GradientStep0.25_' + str(
        iterationPhasePrefix) + '_warp.nii.gz'
    TemplateBuildSingleIterationWF.connect(
        AvgWarpImages, 'output_average_image', GradientStepWarpImage,
        'first_input')

    # Now create the new template shape based on the average of all deformed images
    UpdateTemplateShape = pe.Node(
        interface=WarpImageMultiTransform(), name='UpdateTemplateShape')
    UpdateTemplateShape.inputs.invert_affine = [1]
    TemplateBuildSingleIterationWF.connect(
        AvgDeformedImages, 'output_average_image', UpdateTemplateShape,
        'reference_image')
    TemplateBuildSingleIterationWF.connect(
        AvgAffineTransform, 'affine_transform', UpdateTemplateShape,
        'transformation_series')
    TemplateBuildSingleIterationWF.connect(GradientStepWarpImage,
                                           'output_product_image',
                                           UpdateTemplateShape, 'input_image')

    ApplyInvAverageAndFourTimesGradientStepWarpImage = pe.Node(
        interface=util.Function(
            function=MakeTransformListWithGradientWarps,
            input_names=['averageAffineTranform', 'gradientStepWarp'],
            output_names=['TransformListWithGradientWarps']),
        run_without_submitting=True,
        name='MakeTransformListWithGradientWarps')
    ApplyInvAverageAndFourTimesGradientStepWarpImage.interface.ignore_exception = True

    TemplateBuildSingleIterationWF.connect(
        AvgAffineTransform, 'affine_transform',
        ApplyInvAverageAndFourTimesGradientStepWarpImage,
        'averageAffineTranform')
    TemplateBuildSingleIterationWF.connect(
        UpdateTemplateShape, 'output_image',
        ApplyInvAverageAndFourTimesGradientStepWarpImage, 'gradientStepWarp')

    ReshapeAverageImageWithShapeUpdate = pe.Node(
        interface=WarpImageMultiTransform(),
        name='ReshapeAverageImageWithShapeUpdate')
    ReshapeAverageImageWithShapeUpdate.inputs.invert_affine = [1]
    ReshapeAverageImageWithShapeUpdate.inputs.out_postfix = '_Reshaped'
    TemplateBuildSingleIterationWF.connect(
        AvgDeformedImages, 'output_average_image',
        ReshapeAverageImageWithShapeUpdate, 'input_image')
    TemplateBuildSingleIterationWF.connect(
        AvgDeformedImages, 'output_average_image',
        ReshapeAverageImageWithShapeUpdate, 'reference_image')
    TemplateBuildSingleIterationWF.connect(
        ApplyInvAverageAndFourTimesGradientStepWarpImage,
        'TransformListWithGradientWarps', ReshapeAverageImageWithShapeUpdate,
        'transformation_series')
    TemplateBuildSingleIterationWF.connect(ReshapeAverageImageWithShapeUpdate,
                                           'output_image', outputSpec,
                                           'template')

    ######
    ######
    # Process all the passive deformed images in a way similar to the main image used for registration
    ######
    ######
    ######
    ##############################################
    # Now warp all the ListOfPassiveImagesDictionaries images
    FlattenTransformAndImagesListNode = pe.Node(
        Function(
            function=FlattenTransformAndImagesList,
            input_names=[
                'ListOfPassiveImagesDictionaries', 'transformation_series'
            ],
            output_names=[
                'flattened_images', 'flattened_transforms',
                'flattened_image_nametypes'
            ]),
        run_without_submitting=True,
        name="99_FlattenTransformAndImagesList")
    TemplateBuildSingleIterationWF.connect(
        inputSpec, 'ListOfPassiveImagesDictionaries',
        FlattenTransformAndImagesListNode, 'ListOfPassiveImagesDictionaries')
    TemplateBuildSingleIterationWF.connect(MakeTransformsLists, 'out',
                                           FlattenTransformAndImagesListNode,
                                           'transformation_series')
    wimtPassivedeformed = pe.MapNode(
        interface=WarpImageMultiTransform(),
        iterfield=['transformation_series', 'input_image'],
        name='wimtPassivedeformed')
    TemplateBuildSingleIterationWF.connect(
        AvgDeformedImages, 'output_average_image', wimtPassivedeformed,
        'reference_image')
    TemplateBuildSingleIterationWF.connect(FlattenTransformAndImagesListNode,
                                           'flattened_images',
                                           wimtPassivedeformed, 'input_image')
    TemplateBuildSingleIterationWF.connect(
        FlattenTransformAndImagesListNode, 'flattened_transforms',
        wimtPassivedeformed, 'transformation_series')

    TemplateBuildSingleIterationWF.connect(FlattenTransformAndImagesListNode, 'flattened_transforms',
                                           outputSpec, 'flattened_transforms')

    RenestDeformedPassiveImagesNode = pe.Node(
        Function(
            function=RenestDeformedPassiveImages,
            input_names=['deformedPassiveImages', 'flattened_image_nametypes'],
            output_names=[
                'nested_imagetype_list', 'outputAverageImageName_list',
                'image_type_list'
            ]),
        run_without_submitting=True,
        name="99_RenestDeformedPassiveImages")
    TemplateBuildSingleIterationWF.connect(wimtPassivedeformed, 'output_image',
                                           RenestDeformedPassiveImagesNode,
                                           'deformedPassiveImages')
    
    TemplateBuildSingleIterationWF.connect(wimtPassivedeformed, 'output_image',
                                           outputSpec, 'wimtPassivedeformed')

    TemplateBuildSingleIterationWF.connect(
        FlattenTransformAndImagesListNode, 'flattened_image_nametypes',
        RenestDeformedPassiveImagesNode, 'flattened_image_nametypes')
    # Now  Average All passive input_images deformed images together to create an updated template average
    AvgDeformedPassiveImages = pe.MapNode(
        interface=AverageImages(),
        iterfield=['images', 'output_average_image'],
        name='AvgDeformedPassiveImages')
    AvgDeformedPassiveImages.inputs.dimension = 3
    AvgDeformedPassiveImages.inputs.normalize = False
    TemplateBuildSingleIterationWF.connect(RenestDeformedPassiveImagesNode,
                                           "nested_imagetype_list",
                                           AvgDeformedPassiveImages, 'images')
    TemplateBuildSingleIterationWF.connect(
        RenestDeformedPassiveImagesNode, "outputAverageImageName_list",
        AvgDeformedPassiveImages, 'output_average_image')

    # -- TODO:  Now neeed to reshape all the passive images as well
    ReshapeAveragePassiveImageWithShapeUpdate = pe.MapNode(
        interface=WarpImageMultiTransform(),
        iterfield=['input_image', 'reference_image', 'out_postfix'],
        name='ReshapeAveragePassiveImageWithShapeUpdate')
    ReshapeAveragePassiveImageWithShapeUpdate.inputs.invert_affine = [1]
    TemplateBuildSingleIterationWF.connect(
        RenestDeformedPassiveImagesNode, "image_type_list",
        ReshapeAveragePassiveImageWithShapeUpdate, 'out_postfix')
    TemplateBuildSingleIterationWF.connect(
        AvgDeformedPassiveImages, 'output_average_image',
        ReshapeAveragePassiveImageWithShapeUpdate, 'input_image')
    TemplateBuildSingleIterationWF.connect(
        AvgDeformedPassiveImages, 'output_average_image',
        ReshapeAveragePassiveImageWithShapeUpdate, 'reference_image')
    TemplateBuildSingleIterationWF.connect(
        ApplyInvAverageAndFourTimesGradientStepWarpImage,
        'TransformListWithGradientWarps',
        ReshapeAveragePassiveImageWithShapeUpdate, 'transformation_series')
    TemplateBuildSingleIterationWF.connect(
        ReshapeAveragePassiveImageWithShapeUpdate, 'output_image', outputSpec,
        'passive_deformed_templates')

    return TemplateBuildSingleIterationWF
