# Builds the **Tune build** for OpenBench SPSA workloads, whose engine entry has
# "path": "openbench/tune". OpenBench runs `make -j EXE=<name> [EVALFILE=<file>]` in this
# directory and takes the binary from here.
#
#   make EXE=mudskipper-tune              Tune build named mudskipper-tune, in this directory
#   make EXE=mudskipper-tune CPU=native   tuned for this machine
#   make EXE=mudskipper-tune EVALFILE=f   embedding network file f instead of the default
#
# The root Makefile does the build, so the network is fetched and CPU= applies as in every
# other build. OpenBench passes CC= or CXX= only for C and C++ engines; both are accepted
# and unused.

EXE ?= mudskipper-tune
ROOT := ../..

# The root Makefile runs from the repository root, so paths given here are made absolute
# and passed on its command line, which wins over the relative ones it would otherwise
# inherit through MAKEFLAGS. CPU= is inherited unchanged.
PATHS := EXE=$(abspath $(EXE))
ifdef EVALFILE
    PATHS += EVALFILE=$(abspath $(EVALFILE))
endif

.PHONY: default

default:
	$(MAKE) -C $(ROOT) $(PATHS) FEATURES=tune
