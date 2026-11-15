# Builds a release binary named by EXE, the way OpenBench and SPRT scripts expect.
#
#   make                          portable binary named mudskipper
#   make EXE=dev-abc123           portable binary named dev-abc123
#   make EXE=dev-abc123 CPU=native  binary tuned for this machine (use for SPRT)

EXE ?= mudskipper
CPU ?=

ifeq ($(OS),Windows_NT)
    OUT := $(patsubst %.exe,%,$(EXE)).exe
else
    OUT := $(EXE)
endif

ifneq ($(CPU),)
    CPU_FLAGS := -C target-cpu=$(CPU)
endif

.PHONY: default clean

default:
	cargo rustc --release --bin mudskipper -- $(CPU_FLAGS) --emit link=$(OUT)

clean:
	cargo clean
	rm -f $(OUT)
