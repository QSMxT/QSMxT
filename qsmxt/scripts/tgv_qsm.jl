#!/usr/bin/env julia
import Pkg
try
    using QuantitativeSusceptibilityMappingTGV, MriResearchTools, ArgParse
catch
    Pkg.add(["QuantitativeSusceptibilityMappingTGV", "MriResearchTools", "ArgParse"])
    using QuantitativeSusceptibilityMappingTGV, MriResearchTools, ArgParse
end

function try_load_or_install(pkg_name::String)
    try
        @eval using $(Symbol(pkg_name))
        return @eval $(Symbol(pkg_name))
    catch
        @info "Package $pkg_name not found. Attempting to install..."
        try
            Pkg.add(pkg_name)
            @eval using $(Symbol(pkg_name))
            return @eval $(Symbol(pkg_name))
        catch e
            @warn "Failed to install or load $pkg_name: $e. Falling back to CPU."
            return nothing
        end
    end
end

function load_gpu_module(gpu_type)
    if gpu_type === nothing
        return nothing
    end
    gpu_lower = lowercase(gpu_type)
    if gpu_lower == "cuda"
        return try_load_or_install("CUDA")
    elseif gpu_lower == "amdgpu"
        return try_load_or_install("AMDGPU")
    elseif gpu_lower == "oneapi"
        return try_load_or_install("oneAPI")
    elseif gpu_lower == "metal"
        return try_load_or_install("Metal")
    else
        @warn "Unknown GPU type: $gpu_type. Falling back to CPU."
        return nothing
    end
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
        "--b0-dir"
            help = "input - magnetic field direction for oblique acquisitions"
            default = "[0,0,1]"
        "--alphas"
            help = "input - manual regularization alphas"
            default = nothing
        "--iterations"
            help = "input - number of iterations"
            default = nothing
        "--regularization"
            help = "input - regularization factor"
            default = "2.0"
        "--gpu"
            help = "GPU backend: cuda, amdgpu, oneapi, metal (default: CPU)"
            default = nothing
        "--output"
            help = "output - qsm filename"
            default = "chi.nii"
    end
    return parse_args(ARGS, s)
end

function main(args)
    # Load GPU module if specified
    gpu_module = load_gpu_module(args["gpu"])

    phase = readphase(args["phase"])
    mask = niread(args["mask"]) .!= 0

    vsz = header(phase).pixdim[2:4]
    erosions::Int = eval(Meta.parse(args["erosions"]))
    TE::Float64 = eval(Meta.parse(args["TE"]))
    B0::Float64 = eval(Meta.parse(args["b0-str"]))
    regularization::Float64 = eval(Meta.parse(args["regularization"]))

    # Parse B0 direction for oblique acquisitions
    b0_dir = collect(Float64, eval(Meta.parse(args["b0-dir"])))

    # Parsing alpha and iterations values
    alpha = args["alphas"] !== nothing ? Tuple(map(x -> parse(Float64, String(x)), split(replace(args["alphas"], ['[', ']'] => "")))) : nothing
    iterations = args["iterations"] !== nothing ? parse(Int, args["iterations"]) : nothing

    # Build the set of named arguments dynamically
    kwargs = Dict(
        :TE => TE,
        :fieldstrength => B0,
        :erosions => erosions,
        :laplacian => get_laplace_phase3,
        :regularization => regularization,
        :B0_dir => b0_dir
    )
    if alpha !== nothing
        kwargs[:alpha] = alpha
    end
    if iterations !== nothing
        kwargs[:iterations] = iterations
    end
    if gpu_module !== nothing
        kwargs[:gpu] = gpu_module
    end

    # Use splatting to pass the named arguments to the function
    @time chi = qsm_tgv(phase, mask, vsz; pairs(kwargs)...)

    savenii(chi, args["output"]; header = header(phase))
end

# Entry point
args = parse_cli_args()
main(args)
