# Builds the release build for OpenBench SPRT workloads, whose engine entry has
# "path": "openbench/sprt". ../build.mk describes the build.
#
#   make EXE=mudskipper-dev              release build named mudskipper-dev, tuned for this
#                                        machine, in this directory
#   make EXE=mudskipper-dev CPU=         portable binary instead
#   make EXE=mudskipper-dev EVALFILE=f   embedding network file f instead of the default

EXE ?= mudskipper
FEATURES :=

include ../build.mk
