#!/usr/bin/env julia

using QuantitativeSusceptibilityMappingTGV
using MriResearchTools
using ArgParse

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
    alpha::Tuple{Float64,Float64} = args["alphas"] !== nothing ? eval(Meta.parse(args["alphas"])) : get_default_alpha(regularization)
    iterations::Int = args["iterations"] !== nothing ? eval(Meta.parse(args["iterations"])) : get_default_iterations(vsz, step_size)

    println(vsz)
    println(erosions)
    println(TE)
    println(B0)
    println(regularization)
    println(alpha)
    println(iterations)

    @time chi = qsm_tgv(
        phase, 
        mask, 
        vsz;
        TE = TE,
        fieldstrength = B0,
        alpha = alpha,
        iterations = iterations,
        erosions = erosions,
        laplacian = get_laplace_phase3
    )

    savenii(chi, args["output"]; header = header(phase))
end

# Entry point
args = parse_cli_args()
main(args)
