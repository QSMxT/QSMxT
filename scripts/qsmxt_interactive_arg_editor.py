#!/usr/bin/env python3

import os
import sys

def get_user_input(prompt, options=None, default=None, type_=str):
    while True:
        user_in = input(prompt)
        if user_in == "":
            return default
        if options is not None and user_in not in options:
            continue
        if type_ == bool:
            if user_in.lower().strip() in ['y', 'yes', 'on', 'true', 'enabled']:
                return True
            elif user_in.lower().strip() in ['n', 'no', 'off', 'false', 'disabled']:
                return False
            else:
                continue
        else:
            try:
                user_in = type_(user_in)
            except ValueError:
                continue
        return user_in

def get_list_input(prompt, default=None, list_type=float, list_len=None):
    while True:
        user_in = input(prompt)
        if user_in == "":
            return default
        try:
            user_in = [list_type(val) for val in user_in.split(" ")]
        except ValueError:
            continue
        if list_len is not None and not (list_len[0] <= len(user_in) <= list_len[1]):
            continue
        return user_in

def interactive_arg_editor(args):
    print("\n=== QSMxT: Select Settings Interactively ===")

    default_args = {
        'gre' : {
            'combine_phase' : False,
            'qsm_algorithm' : 'rts',
            'unwrapping_algorithm' : 'romeo',
            'bf_algorithm' : 'pdf',
            'masking_algorithm' : 'threshold',
            'two_pass' : True,
            'masking_input' : 'phase',
            'threshold_algorithm' : 'otsu',
            'threshold_algorithm_factor' : [1.7, 1.0],
            'filling_algorithm' : 'both',
            'inhomogeneity_correction' : False,
            'mask_erosions' : [3, 0],
        },
        'epi' : {
            'combine_phase' : False,
            'qsm_algorithm' : 'rts',
            'unwrapping_algorithm' : 'romeo',
            'bf_algorithm' : 'pdf',
            'masking_algorithm' : 'threshold',
            'two_pass' : True,
            'masking_input' : 'phase',
            'threshold_algorithm' : 'otsu',
            'threshold_algorithm_factor' : [1.7, 1.0],
            'filling_algorithm' : 'both',
            'inhomogeneity_correction' : True,
            'mask_erosions' : [3, 0],
            'add_bet' : True
        },
        'bet' : {
            'masking_algorithm' : 'bet'
        },
        'fast' : {
            'combine_phase' : False,
            'qsm_algorithm' : 'rts',
            'unwrapping_algorithm' : 'romeo',
            'bf_algorithm' : 'vsharp',
            'masking_algorithm' : 'bet',
            'mask_erosions' : [3],
        },
        'body' : {
            'combine_phase' : False,
            'qsm_algorithm' : 'tgv',
            'masking_algorithm' : 'threshold',
            'two_pass' : True,
            'masking_input' : 'phase',
            'threshold_value' : [0.25],
            'filling_algorithm' : 'both',
            'mask_erosions' : [3, 0],
        },
        'nextqsm' : {
            'combine_phase' : False,
            'qsm_algorithm' : 'nextqsm',
            'masking_algorithm' : 'bet-firstecho',
            'mask_erosions' : [3]
        }
    }

    def overwrite_args(args, new_args):
        for key, value in new_args.items():
            args[key] = value
        return args
    if args.premade: args = overwrite_args(args, default_args[args.premade])

    if not len(sys.argv) > 3 and not (len(sys.argv) == 5 and '--premade' in sys.argv):
        print("\n=== Premade pipelines ===")
        print("gre: Applies suggested settings for 3D-GRE images")
        print("epi: Applies suggested settings for 3D-EPI images (assumes human brain)")
        print("bet: Applies a traditional BET-masking approach (artefact reduction unavailable)")
        print("fast: Applies a set of fast algorithms")
        print("body: Applies suggested settings for non-brain applications") # ...
        print("nextqsm: Applies suggested settings for running the NeXtQSM algorithm (assumes human brain)")

        args.premade = get_user_input(
            prompt=f"\nSelect premade pipeline (enter for default - {args.premade}): ",
            options=['gre', 'epi', 'bet', 'fast', 'body']
        )

        if args.premade: args = overwrite_args(args, default_args[args.premade])

    while True:
        os.system('clear')
        print("== Summary ==")
        print(f"\nPaths and patterns:")
        print(f" - Input BIDS directory: {args.bids_dir}")
        print(f" - Output QSM directory: {args.output_dir}")
        print(f" - Subject pattern: {args.subject_pattern}")
        print(f" - Session pattern: {args.session_pattern}")
        print(f" - Magnitude pattern: {args.magnitude_pattern}")
        print(f" - Phase pattern: {args.phase_pattern}")
        print(f" - Mask pattern {args.mask_pattern}")
        print(f" - Subjects: {'Process all subjects' if not args.subjects else args.subjects}")
        print(f" - Sessions: {'Process all sessions' if not args.sessions else args.sessions}")
        print(f" - Echoes: {'Process all echoes' if not args.num_echoes else args.num_echoes}")

        print("\nExecution settings:")
        if args.slurm or args.pbs:
            if args.slurm: print(f" - Execution type: HPC (SLURM with account string {args.slurm})")
            else: print(f" - Execution type: HPC (PBS graph with account string {args.pbs})")
        else:
            print(f" - Execution type: MultiProc (n_procs={args.n_procs})")
        print(f" - Debug mode: {args.debug}")
        print(f" - Dry run: {args.dry}")
        
        print("\n(1) Masking:")
        print(f" - Use existing masks if available: {'Yes' if args.use_existing_masks else 'No'}")
        if args.masking_algorithm == 'threshold':
            print(f" - Masking algorithm: threshold ({args.masking_input}-based{('; inhomogeneity-corrected)' if args.masking_input == 'magnitude' and args.inhomogeneity_correction else ')')}")
            print(f"   - Two-pass artefact reduction: {'Enabled' if args.two_pass else 'Disabled'}")
            if len(args.threshold_value) >= 2 and all(args.threshold_value) and args.two_pass:
                if int(args.threshold_value[0]) == float(args.threshold_value[0]) and int(args.threshold_value[1]) == float(args.threshold_value[1]):
                    print(f"   - Threshold: {int(args.threshold_value[0])}, {int(args.threshold_value[1])} (hardcoded voxel intensities)")
                else:
                    print(f"   - Threshold: {float(args.threshold_value[0])}%, {float(args.threshold_value[1])}% (hardcoded percentiles of the signal histogram)")
            elif len(args.threshold_value) == 1 and all(args.threshold_value):
                if int(args.threshold_value[0]) == float(args.threshold_value[0]):
                    print(f"   - Threshold: {int(args.threshold_value[0])} (hardcoded voxel intensity)")
                else:
                    print(f"   - Threshold: {float(args.threshold_value[0])}% (hardcoded percentile of per-echo histogram)")
            else:
                print(f"   - Threshold algorithm: {args.threshold_algorithm}", end="")
                if len(args.threshold_algorithm_factor) >= 2 and args.two_pass:
                    print(f" (x{args.threshold_algorithm_factor[0]} for single-pass; x{args.threshold_algorithm_factor[1]} for two-pass)")
                elif len(args.threshold_algorithm_factor) == 1:
                    print(f" (x{args.threshold_algorithm_factor[0]})")
                else:
                    print()
            print(f"   - Hole-filling algorithm: {'morphological+gaussian' if args.filling_algorithm == 'both' else args.filling_algorithm}{'+bet' if args.add_bet else ''}{f' (bet fractional intensity = {args.bet_fractional_intensity})' if args.add_bet else ''}")
            if args.two_pass and len(args.mask_erosions) == 2:
                print(f"   - Erosions: {args.mask_erosions[0]} erosions for single-pass; {args.mask_erosions[1]} erosions for two-pass")
        else:
            print(f" - Masking algorithm: {args.masking_algorithm}{f' (fractional intensity = {args.bet_fractional_intensity})' if 'bet' in args.masking_algorithm else ''}")
            print(f"   - Erosions: {args.mask_erosions[0]}")
        
        print("\n(2) Phase processing:")
        print(f" - Axial resampling: " + (f"Enabled (obliquity threshold = {args.obliquity_threshold})" if args.obliquity_threshold else " Disabled"))
        print(f" - Multi-echo combination: " + ("B0 mapping (using ROMEO)" if args.combine_phase else "Susceptibility averaging"))
        if args.qsm_algorithm in ['rts']:
            print(f" - Phase unwrapping: {args.unwrapping_algorithm}")
            print(f" - Background field removal: {args.bf_algorithm}")
        print(f" - Dipole inversion: {args.qsm_algorithm}")
        
        user_in = get_user_input(
            prompt="\nEnter a number to customize; enter 'run' to run: ",
        )
        if user_in == 'run': break
        if not user_in: continue
        else:
            try:
                user_in = int(user_in)
            except ValueError:
                continue
        
        if 1 <= user_in <= 2:
            if user_in == 1: # MASKING
                os.system('clear')
                print("=== MASKING ===")

                print("\n== Existing masks ==")
                args.use_existing_masks = get_user_input(
                    prompt=f"Use existing masks if available [default: {'yes' if args.use_existing_masks else 'no'}]: ",
                    options=['yes', 'no'],
                    type_=bool
                )
                args.mask_pattern = get_user_input(
                    prompt=f"Enter mask file pattern [default: {args.mask_pattern}]: ",
                    default=args.mask_pattern
                )
                
                print("\n== Masking algorithm ==")
                print("threshold: ")
                print("     - required for the two-pass artefact reduction algorithm (https://doi.org/10.1002/mrm.29048)")
                print("     - required for applications other than in vivo human brain")
                print("     - more robust to severe pathology")
                print("bet: Applies the Brain Extraction Tool (standalone version)")
                print("     - the standard in most QSM pipelines")
                print("     - robust in healthy human brains")
                print("     - Paper: https://doi.org/10.1002/hbm.10062")
                print("     - Code: https://github.com/liangfu/bet2")
                print("bet-firstecho: Applies BET to the first-echo magnitude only")
                print("     - This setting is the same as BET for single-echo acquisitions or if multi-echo images are combined using B0 mapping")
                print("\nNOTE: Even if you are using premade masks, a masking method is required as a backup.\n")
                args.masking_algorithm = get_user_input(
                    prompt=f"Select masking algorithm [default - {args.masking_algorithm}]: ",
                    options=['bet', 'bet-firstecho', 'threshold'],
                    default=args.masking_algorithm
                )

                if 'bet' in args.masking_algorithm:
                    args.bet_fractional_intensity = get_user_input(
                        prompt=f"\nBET fractional intensity [default - {args.bet_fractional_intensity}]: ",
                        default=args.bet_fractional_intensity,
                        type_=float
                    )

                if args.masking_algorithm == 'threshold':
                    print("\n== Threshold input ==")
                    print("Select the input to be used in the thresholding algorithm.\n")
                    print("magnitude: use the MRI signal magnitude")
                    print("  - standard approach")
                    print("  - requires magnitude images")
                    print("phase: use a phase quality map")
                    print("  - phase quality map produced by ROMEO (https://doi.org/10.1002/mrm.28563)")
                    print("  - measured between 0 and 100")
                    print("  - some evidence that phase-based masks are more reliable near the brain boundary (https://doi.org/10.1002/mrm.29368)")

                    args.masking_input = get_user_input(
                        prompt=f"\nSelect threshold input [default - {args.masking_input}]: ",
                        options=['magnitude', 'phase'], default=args.masking_input
                    )

                    if args.masking_input == 'magnitude':
                        args.inhomogeneity_correction = get_user_input(
                            prompt=f"\nApply inhomogeneity correction to magnitude [default: {'yes' if args.inhomogeneity_correction else 'no'}]: ",
                            options=['yes', 'no'],
                            default=args.inhomogeneity_correction,
                            type_=bool
                        )

                    print("\n== Two-pass Artefact Reduction ==")
                    print("Select whether to use the two-pass artefact reduction algorithm (https://doi.org/10.1002/mrm.29048).\n")
                    print("  - reduces artefacts, particularly near strong susceptibility sources")
                    print("  - sometimes requires tweaking of the mask to maintain accuracy in high-susceptibility regions")
                    print("  - single-pass results will still be included in the output")
                    print("  - doubles the runtime of the pipeline")
                    args.two_pass = get_user_input(
                        f"\nSelect on or off [default - {'on' if args.two_pass else 'off'}]: ",
                        options=['on', 'off'],
                        type_=bool,
                        default=args.two_pass
                    )
                    if len(args.threshold_value) == 2 and not args.two_pass:
                        args.threshold_value = [args.threshold_value[0]]

                    print("\n== Threshold value ==")
                    print("Select an algorithm to automate threshold selection, or enter a custom threshold.\n")
                    print("otsu: Automate threshold selection using the Otsu algorithm (https://doi.org/10.1109/TSMC.1979.4310076)")
                    print("gaussian: Automate threshold selection using a Gaussian algorithm (https://doi.org/10.1016/j.compbiomed.2012.01.004)")
                    print("\nHardcoded threshold:")
                    print(" - Use an integer to indicate an absolute signal intensity")
                    print(" - Use a floating-point value from 0-1 to indicate a percentile of the per-echo signal histogram")
                    if args.two_pass: print(" - Use two values to specify different thresholds for each pass in two-pass QSM")
                    while True:
                        user_in = input(f"\nSelect threshold algorithm or value [default - {args.threshold_value if args.threshold_value != [None, None] else args.threshold_algorithm if args.threshold_algorithm else 'otsu'}]: ")
                        if user_in == "":
                            break
                        elif user_in in ['otsu', 'gaussian']:
                            args.threshold_algorithm = user_in
                            break
                        else:
                            try:
                                user_in = [float(val) for val in user_in.split(" ")]
                            except ValueError:
                                continue
                            if not (1 <= len(user_in) <= 2):
                                continue
                            if all(val == int(val) for val in user_in):
                                args.threshold_value = [int(val) for val in user_in]
                            else:
                                args.threshold_value = user_in
                            break

                    if args.threshold_value != [None, None]: args.threshold_algorithm = [None, None]
                    if args.threshold_value == [None, None] and not args.threshold_algorithm:
                        args.threshold_algorithm = 'otsu'

                    if args.threshold_algorithm in ['otsu', 'gaussian']:
                        args.threshold_value = [None, None]
                        print("\n== Threshold algorithm factors ==")
                        print("The threshold algorithm can be tweaked by multiplying it by some factor.")
                        args.threshold_algorithm_factor = get_list_input(
                            prompt=f"\nEnter threshold algorithm factor(s) (space-separated) [default - {str(args.threshold_algorithm_factor)}]: ",
                            list_type=float,
                            default=args.threshold_algorithm_factor,
                            list_len=(1, 2)
                        )
                        
                    print("\n== Hole-filling algorithm ==")
                    print("Threshold-based masking requires an algorithm to fill holes in the mask.\n")
                    print("gaussian:")
                    print(" - applies the scipy gaussian_filter function")
                    print(" - may fill some unwanted regions (e.g. connecting skull to brain)")
                    print("morphological:")
                    print(" - applies the scipy binary_fill_holes function")
                    print("both:")
                    print(" - applies both methods (gaussian followed by morphological)")
                    args.filling_algorithm = get_user_input(
                        prompt=f"\nSelect hole-filling algorithm: [default - {args.filling_algorithm}]: ",
                        options=['gaussian', 'morphological', 'both'],
                        default=args.filling_algorithm
                    )
                    args.add_bet = get_user_input(
                        prompt=f"\nInclude a BET mask in the hole-filling operation (yes or no) [default - {'yes' if args.add_bet else 'no'}]: ",
                        options=['yes', 'no'],
                        default=args.add_bet,
                        type_=bool
                    )
                    if args.add_bet:
                        args.bet_fractional_intensity = get_user_input(
                            prompt=f"\nBET fractional intensity [default - {args.bet_fractional_intensity}]: ",
                            default=args.bet_fractional_intensity,
                            type_=float
                        )
            if user_in == 2: # PHASE PROCESSING
                os.system('clear')
                print("== Resample to axial ==")
                print("This step will perform axial resampling for oblique acquisitions.")
                args.obliquity_threshold = get_user_input(
                    prompt=f"\nEnter an obliquity threshold to cause resampling or -1 for none [default - {args.obliquity_threshold}]: ",
                    default=args.obliquity_threshold,
                    type_=float
                )

                print("== Combine phase ==")
                print("This step will combine multi-echo phase data by generating a field map using ROMEO.")
                args.combine_phase = get_user_input(
                    prompt=f"\nCombine multi-echo phase data [default - {'yes' if args.combine_phase else 'no'}]: ",
                    default=args.combine_phase,
                    type_=bool
                )

                print("\n== QSM Algorithm ==")
                print("rts: Rapid Two-Step (10.1016/j.neuroimage.2017.11.018)")
                print("   - Compatible with two-pass artefact reduction algorithm")
                print("   - Fast runtime")
                print("tgv: Total Generalized Variation (10.1016/j.neuroimage.2015.02.041)")
                print("   - Combined unwrapping, background field removal and dipole inversion")
                print("   - Most stable with custom masks")
                print("   - Long runtime")
                print("   - Compatible with two-pass artefact reduction algorithm")
                print("nextqsm: NeXtQSM (10.1016/j.media.2022.102700)")
                print('   - Uses deep learning to solve the background field removal and dipole inversion steps')
                print('   - High memory requirements (>=12gb recommended)')
                args.qsm_algorithm = get_user_input(
                    prompt=f"\nSelect QSM algorithm [default - {args.qsm_algorithm}]: ",
                    options=['rts', 'tgv', 'nextqsm'],
                    default=args.qsm_algorithm
                )

                if args.qsm_algorithm in ['rts', 'nextqsm']:
                    print("\n== Unwrapping algorithm ==")
                    print("romeo: (https://doi.org/10.1002/mrm.28563)")
                    print(" - quantitative")
                    print("laplacian: (https://doi.org/10.1364/OL.28.001194; https://doi.org/10.1002/nbm.3064)")
                    print(" - non-quantitative")
                    print(" - popular for its numerical simplicity")
                    while True:
                        user_in = input(f"\nSelect unwrapping algorithm [default - {args.unwrapping_algorithm}]: ")
                        if user_in == "": break
                        elif user_in in ['romeo', 'laplacian']:
                            args.unwrapping_algorithm = user_in
                            break

                if args.qsm_algorithm in ['rts']:
                    print("\n== Background field removal ==")
                    print("vsharp: V-SHARP algorithm (https://doi.org/10.1002/mrm.23000)")
                    print(" - fast")
                    print(" - involves a mask erosion step that impacts the next steps")
                    print(" - less reliable with threshold-based masks")
                    print(" - not compatible with artefact reduction algorithm")
                    print("pdf: Projection onto Dipole Fields algorithm (https://doi.org/10.1002/nbm.1670)")
                    print(" - slower")
                    print(" - more accurate")
                    print(" - does not require an additional erosion step")
                    while True:
                        user_in = input(f"\nSelect background field removal algorithm [default - {args.bf_algorithm}]: ")
                        if user_in == "": break
                        elif user_in in ['vsharp', 'pdf']:
                            args.bf_algorithm = user_in
                            break

    return args

