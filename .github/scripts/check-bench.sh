#!/usr/bin/env bash
# Checks the Bench signature: runs `bench` and compares its node count with the
# `Bench: <nodes>` line of a commit message.
#
#   check-bench.sh <engine> [commit]    commit defaults to HEAD
#
# Exits 0 on a match and 1 on a mismatch or a missing `Bench:` line.
set -euo pipefail

engine=${1:?usage: check-bench.sh <engine> [commit]}
commit=${2:-HEAD}

# The last `Bench:` line wins, so a squash-merge that keeps earlier commit messages
# still checks the final one.
expected=$(git log -1 --format=%B "$commit" | tr -d '\r' |
    sed -n 's/^Bench: *\([0-9][0-9]*\) *$/\1/p' | tail -n 1)

actual=$("$engine" bench | tr -d '\r' | tail -n 1 |
    sed -n 's/^\([0-9][0-9]*\) nodes [0-9][0-9]* nps$/\1/p')

if [[ -z $actual ]]; then
    echo "::error::bench did not end with '<nodes> nodes <nps> nps'"
    exit 1
fi

if [[ -z $expected ]]; then
    echo "::error::commit $commit has no 'Bench: <nodes>' line; bench reports $actual"
    echo "Add this line to the commit message:"
    echo "Bench: $actual"
    exit 1
fi

if [[ $expected != "$actual" ]]; then
    echo "::error::Bench signature mismatch: commit says $expected, bench reports $actual"
    echo "expected: $expected"
    echo "actual:   $actual"
    exit 1
fi

echo "Bench signature matches: $actual"
