# Builds the **Tune build** for OpenBench SPSA workloads, whose engine entry has
# "path": "openbench/tune". ../build.mk describes the build.
#
#   make EXE=mudskipper-tune              Tune build named mudskipper-tune, tuned for this
#                                         machine, in this directory
#   make EXE=mudskipper-tune CPU=         portable binary instead
#   make EXE=mudskipper-tune EVALFILE=f   embedding network file f instead of the default

EXE ?= mudskipper-tune
FEATURES := tune

include ../build.mk
