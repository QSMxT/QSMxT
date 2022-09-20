#!/usr/bin/env julia
import Pkg
Pkg.activate("/neurodesktop-storage/qsmxt") # TODO remove when push to docker
using RomeoApp

unwrapping_main(ARGS)
