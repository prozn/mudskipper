# Builds a release binary named by EXE, the way OpenBench and SPRT scripts expect.
#
#   make                          portable binary named mudskipper
#   make EXE=dev-abc123           portable binary named dev-abc123
#   make EXE=dev-abc123 CPU=native  binary tuned for this machine (use for SPRT)
#   make FEATURES=datagen         binary with the given Cargo features (comma-separated)
#   make net                      fetch the network named in network.txt (ADR-0004)
#   make networks/<name>.bin      fetch any other published network
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

# The network recipes use only make's own functions and curl, which Windows 10 and later
# ship with, so that they run from cmd.exe and PowerShell as well as from sh. Reading
# network.txt needs GNU make 4.0 or later.

# The default network, named in network.txt and published as a release asset of the
# networks repository.
NET := $(strip $(file <network.txt))
ifeq ($(NET),)
    $(error cannot read the network name from network.txt (GNU make 4.0 or later is needed))
endif
NETWORKS_URL := https://github.com/prozn/mudskipper-networks/releases/download
CURL := curl -fsSL --retry 3 --create-dirs

# The name of the network in networks/default.bin, which `make net` records beside it.
FETCHED := $(if $(wildcard networks/default.name),$(strip $(file <networks/default.name)))

# include_bytes! reads a relative path from the source file's directory, so EVALFILE is
# made absolute. `override` because a variable set on the command line wins otherwise.
ifdef EVALFILE
    override EVALFILE := $(abspath $(EVALFILE))
    export EVALFILE
else
    NET_PREREQ := net
endif

# A download that fails part way leaves no file behind to be taken as complete.
.DELETE_ON_ERROR:

.PHONY: default net clean FORCE

default: $(NET_PREREQ)
	cargo rustc --release --bin mudskipper $(FEATURE_FLAGS) -- $(CPU_FLAGS) --emit link=$(OUT)

# Fetches networks/default.bin only when it is missing or holds another network, so that
# an unchanged network does not rebuild. The name is recorded once the download succeeds.
net: networks/default.bin
	$(file >networks/default.name,$(NET))

ifneq ($(FETCHED),$(NET))
networks/default.bin: FORCE
endif
networks/default.bin:
	$(CURL) -o $@ $(NETWORKS_URL)/$(NET)/$(NET).bin

# Any other published network, by name: `make networks/<name>.bin`.
networks/%.bin:
	$(CURL) -o $@ $(NETWORKS_URL)/$*/$*.bin

FORCE:

clean:
	cargo clean
	rm -f $(OUT)
