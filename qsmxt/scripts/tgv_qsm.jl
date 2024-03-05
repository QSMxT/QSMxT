#!/usr/bin/env julia
import Pkg
try
    using QuantitativeSusceptibilityMappingTGV, MriResearchTools, ArgParse
catch
    Pkg.add(["QuantitativeSusceptibilityMappingTGV", "MriResearchTools", "ArgParse"])
    using QuantitativeSusceptibilityMappingTGV, MriResearchTools, ArgParse
end

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

    # Build the set of named arguments dynamically
    kwargs = Dict(
        :TE => TE,
        :fieldstrength => B0,
        :erosions => erosions,
        :laplacian => get_laplace_phase3,
        :regularization => regularization
    )
    if alpha !== nothing
        kwargs[:alpha] = alpha
    end
    if iterations !== nothing
        kwargs[:iterations] = iterations
    end

    # Use splatting to pass the named arguments to the function
    @time chi = qsm_tgv(phase, mask, vsz; pairs(kwargs)...)

    savenii(chi, args["output"]; header = header(phase))
end

# Entry point
args = parse_cli_args()
main(args)
