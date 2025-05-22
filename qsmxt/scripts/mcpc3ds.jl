#!/usr/bin/env julia
# run_mcpc3ds.jl

using ArgParse
import MriResearchTools

function parse_command_line()
    s = ArgParseSettings()
    @add_arg_table! s begin
        "--mag"
            help     = "Magnitude files (one NIfTI per echo), each with shape X×Y×Z×coil"
            required = true
            nargs    = '+'
        "--phase"
            help     = "Phase files (one NIfTI per echo), each with shape X×Y×Z×coil"
            required = true
            nargs    = '+'
        "--TEs"
            help     = "Echo times in seconds, comma-separated (e.g. \"0.00942,0.0197\")"
            required = true
        "--outprefix"
            help     = "Output file prefix (no extension)"
            required = true
    end
    return parse_args(ARGS, s)
end

function main()
    # Parse arguments
    args   = parse_command_line()
    mags   = args["mag"]
    phases = args["phase"]
    TEs    = parse.(Float64, split(args["TEs"], r"\s*,\s*"))
    outpre = args["outprefix"]

    @assert length(mags) == length(phases) == length(TEs) "Counts of --mag, --phase and --TEs must match."

    # Load the raw NIfTI volumes so we can later extract their header
    raw_mags   = [ MriResearchTools.readmag(f)   for f in mags   ]
    raw_phases = [ MriResearchTools.readphase(f) for f in phases ]

    # Extract their array data (assumed shape X×Y×Z×coil)
    mag_imgs   = raw_mags
    phase_imgs = raw_phases

    # Stack into 5-D: X×Y×Z×necho×ncoil
    mag5_list   = [ reshape(m, size(m,1), size(m,2), size(m,3), 1, size(m,4))
                    for m in mag_imgs ]
    phase5_list = [ reshape(p, size(p,1), size(p,2), size(p,3), 1, size(p,4))
                    for p in phase_imgs ]
    mag5   = cat(mag5_list...;   dims=4)
    phase5 = cat(phase5_list...; dims=4)

    # Run the coil-combination algorithm
    combined = MriResearchTools.mcpc3ds(phase5, mag5; TEs=TEs)

    # Extract the combined magnitude and phase (4-D: X×Y×Z×necho)
    mag_c   = MriResearchTools.getmag(combined)
    phase_c = MriResearchTools.getangle(combined)

    # Write outputs next to the first magnitude input, re-using its header
    outdir = dirname(mags[1])
    hdr    = MriResearchTools.header(raw_mags[1])
    MriResearchTools.savenii(mag_c,   outpre * "_mag",   outdir, hdr)
    MriResearchTools.savenii(phase_c, outpre * "_phase", outdir, hdr)
end

main()
