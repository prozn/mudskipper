# The build every OpenBench makefile shares. OpenBench runs `make -j EXE=<name>
# [EVALFILE=<file>]` in an engine entry's build path and takes the binary from there. Each
# makefile under openbench/ sets FEATURES and includes this file, so that its build runs
# through the root Makefile: the network is fetched, or EVALFILE embedded, as in every
# other build. OpenBench passes CC= or CXX= only for C and C++ engines; both are accepted
# and unused.

ROOT := ../..

# OpenBench never passes CPU=, so the default is a binary tuned for the machine that builds
# it, which is always the machine that runs it. A portable binary searches slower, so it
# would play shallower games at the same time control. `CPU=` gives a portable build.
CPU ?= native

# The root Makefile runs from the repository root, so paths given here are made absolute
# and passed on its command line, which wins over the relative ones it would otherwise
# inherit through MAKEFLAGS.
PATHS := EXE=$(abspath $(EXE))
ifdef EVALFILE
    PATHS += EVALFILE=$(abspath $(EVALFILE))
endif

.PHONY: default

default:
	$(MAKE) -C $(ROOT) $(PATHS) CPU=$(CPU) FEATURES=$(FEATURES)
