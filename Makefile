# Builds a release binary named by EXE, the way OpenBench and SPRT scripts expect.
#
#   make                          portable binary named mudskipper
#   make EXE=dev-abc123           portable binary named dev-abc123
#   make EXE=dev-abc123 CPU=native  binary tuned for this machine (use for SPRT)
#   make FEATURES=datagen         binary with the given Cargo features (comma-separated)
#   make net                      fetch the network named in network.txt (ADR-0004)
#
# The build embeds networks/default.bin, which `make` first fetches with `make net`.
# Set EVALFILE to embed another network file instead; nothing is then fetched.

EXE ?= mudskipper
CPU ?=
FEATURES ?=

ifeq ($(OS),Windows_NT)
    OUT := $(patsubst %.exe,%,$(EXE)).exe
else
    OUT := $(EXE)
endif

ifneq ($(CPU),)
    CPU_FLAGS := -C target-cpu=$(CPU)
endif

ifneq ($(FEATURES),)
    FEATURE_FLAGS := --features $(FEATURES)
endif

# The default network, named in network.txt and published as a release asset of the
# networks repository.
NET := $(strip $(shell tr -d "\r" < network.txt))
NETWORKS_URL := https://github.com/prozn/mudskipper-networks/releases/download

ifdef EVALFILE
    export EVALFILE := $(abspath $(EVALFILE))
else
    NET_PREREQ := net
endif

.PHONY: default net clean

default: $(NET_PREREQ)
	cargo rustc --release --bin mudskipper $(FEATURE_FLAGS) -- $(CPU_FLAGS) --emit link=$(OUT)

# Copies only when the file differs, so that an unchanged network does not rebuild.
net: networks/$(NET).bin
	cmp -s $< networks/default.bin || cp $< networks/default.bin

networks/$(NET).bin:
	mkdir -p networks
	curl -fsSL --retry 3 -o $@.part $(NETWORKS_URL)/$(NET)/$(NET).bin
	mv $@.part $@

clean:
	cargo clean
	rm -f $(OUT)
