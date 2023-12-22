#!/usr/bin/env julia

using QuantitativeSusceptibilityMappingTGV
using MriResearchTools
using ArgParse
using Pkg

# Function to load the required GPU package, installing it if necessary
function ensure_package_loaded(pkg_name)
    try
        eval(Meta.parse("using $pkg_name"))
    catch
        Pkg.add(pkg_name)
        eval(Meta.parse("using $pkg_name"))
    end
end

# Function to parse command line arguments
function parse_cli_args()
    s = ArgParseSettings()
    @add_arg_table! s begin
        "--phase"
            help = "input - phase filename"
            required = true
        "--mask"
            help = "input - mask filename"
            required = true
        "--erosions"
            help = "input - erosions"
            default = "3"
        "--TE"
            help = "input - echo time (s)"
            required = true
        "--b0-str"
            help = "input - magnetic field strength"
            default = "3.0"
        "--alphas"
            help = "input - manual regularization alphas"
            default = nothing
        "--iterations"
            help = "input - number of iterations"
            default = nothing
        "--regularization"
            help = "input - regularization factor"
            default = "2.0"
        "--output"
            help = "output - qsm filename"
            default = "chi.nii"
        "--gpu"
            help = "input - GPU type (CUDA, AMDGPU, oneAPI, or Metal)"
            default = nothing
    end
    return parse_args(ARGS, s)
end

function main(args)
    phase = readphase(args["phase"])
    mask = niread(args["mask"]) .!= 0

    vsz = header(phase).pixdim[2:4]
    erosions::Int = eval(Meta.parse(args["erosions"]))
    TE::Float64 = eval(Meta.parse(args["TE"]))
    B0::Float64 = eval(Meta.parse(args["b0-str"]))
    regularization::Float64 = eval(Meta.parse(args["regularization"]))

    # Parsing alpha and iterations values
    alpha = args["alphas"] !== nothing ? Tuple(map(x -> parse(Float64, String(x)), split(replace(args["alphas"], ['[', ']'] => "")))) : nothing
    iterations = args["iterations"] !== nothing ? parse(Int, args["iterations"]) : nothing

    # Handling GPU package loading
    gpu = nothing
    if args["gpu"] !== nothing
        ensure_package_loaded(args["gpu"])
        gpu = eval(Meta.parse(args["gpu"]))
    end

    # Building the set of named arguments dynamically
    kwargs = Dict(
        :TE => TE,
        :fieldstrength => B0,
        :erosions => erosions,
        :laplacian => get_laplace_phase3,
        :regularization => regularization
    )

    # Adding optional arguments if present
    if alpha !== nothing
        kwargs[:alpha] = alpha
    end
    if iterations !== nothing
        kwargs[:iterations] = iterations
    end

    # Using splatting to pass named arguments to the function
    @time chi = if gpu !== nothing
                    qsm_tgv(phase, mask, vsz; gpu=gpu, pairs(kwargs)...)
                else
                    qsm_tgv(phase, mask, vsz; pairs(kwargs)...)
                end

    # Saving the output
    savenii(chi, args["output"]; header = header(phase))
end

# Script entry point
args = parse_cli_args()
main(args)
