#!/usr/bin/env python3
"""Plot an SPSA tune's Search parameters against games played, from a track_spsa.py log.

OpenBench keeps only each parameter's current value, so a tune's history exists only in
the log that OpenBench's Scripts/spsa/track_spsa.py appends to. Each log line is a JSON
array of a timestamp, the tune's info and its digest as CSV text. This script draws one
small chart per parameter, its y axis spanning the parameter's full tuning range with a
dashed line at the start value, and writes them to one SVG file for the tune's PR. It
also prints how far each parameter moved over the last quarter of the games, in units of
its C_end, so a parameter still drifting at the end stands out.

Snapshots after the tune finished are ignored. Needs only the standard library.

    python3 openbench/tune/plot_spsa.py spsa.3.log group-a.svg
"""

import argparse
import csv
import io
import json
import sys
from dataclasses import dataclass, field
from html import escape

# Light and dark colours, so the SVG reads in either theme when GitHub shows it.
STYLE = (
    ".bg{fill:#fcfcfb}.t{fill:#0b0b0b}.s{fill:#52514e}.g{stroke:#e4e3df}"
    ".st{stroke:#52514e}.ln{stroke:#2a78d6}.dot{fill:#2a78d6;stroke:#fcfcfb}"
    "@media (prefers-color-scheme:dark){.bg{fill:#1a1a19}.t{fill:#f0efec}"
    ".s{fill:#c3c2b7}.g{stroke:#2e2e2c}.st{stroke:#c3c2b7}.ln{stroke:#3987e5}"
    ".dot{fill:#3987e5;stroke:#1a1a19}}"
)

COLUMNS = 3
PANEL_WIDTH, PANEL_HEIGHT = 300, 240
MARGIN_LEFT, MARGIN_TOP = 44, 40
PLOT_WIDTH, PLOT_HEIGHT = 236, 150
HEADER = 64


@dataclass
class Parameter:
    """One Search parameter's tuning range and its value at each snapshot."""

    start: float
    low: float
    high: float
    c_end: float
    points: list[tuple[int, float]] = field(default_factory=list)


def read_log(path: str) -> tuple[dict[str, Parameter], dict]:
    """Reads the snapshots up to the first one of a finished tune.

    Returns the parameters in digest order and the info of the last snapshot read.
    """
    params: dict[str, Parameter] = {}
    info: dict = {}
    with open(path, encoding="utf-8") as log:
        for line in log:
            if not line.strip():
                continue
            _, info, digest = json.loads(line)
            games = info["games"]
            for row in csv.DictReader(io.StringIO(digest)):
                param = params.setdefault(
                    row["Name"],
                    Parameter(
                        float(row["Start"]),
                        float(row["Min"]),
                        float(row["Max"]),
                        float(row["C_end"]),
                    ),
                )
                if not param.points or param.points[-1][0] != games:
                    param.points.append((games, float(row["Curr"])))
            if info["finished"]:
                break
    if not params:
        sys.exit(f"{path} has no snapshots")
    return params, info


def print_drift(params: dict[str, Parameter], total: int) -> None:
    """Prints each parameter's start and final values and its drift over the last quarter."""
    print(f"{'Parameter':30} {'Start':>7} {'Final':>7} {'Last 25%':>9} {'In C_end':>9}")
    for name, param in params.items():
        final = param.points[-1][1]
        at_three_quarters = min(param.points, key=lambda p: abs(p[0] - 0.75 * total))[1]
        drift = final - at_three_quarters
        print(
            f"{name:30} {param.start:7.0f} {final:7.1f} {drift:+9.1f} "
            f"{drift / param.c_end:+9.2f}"
        )


def panel(name: str, param: Parameter, total: int, left: float, top: float) -> list[str]:
    """Draws one parameter's chart with its top-left corner at (left, top)."""

    def x(games: int) -> float:
        return left + MARGIN_LEFT + games / total * PLOT_WIDTH

    def y(value: float) -> float:
        return top + MARGIN_TOP + (param.high - value) / (param.high - param.low) * PLOT_HEIGHT

    final_games, final = param.points[-1]
    plot_left, plot_right = left + MARGIN_LEFT, left + MARGIN_LEFT + PLOT_WIDTH
    out = [
        f'<text class="t" x="{plot_left}" y="{top + 16}" font-size="13" '
        f'font-weight="600">{escape(name)}</text>',
        f'<text class="s" x="{plot_left}" y="{top + 32}" font-size="12">'
        f"{param.start:g} → {final:.1f}</text>",
    ]
    for value in (param.low, (param.low + param.high) / 2, param.high):
        out.append(
            f'<line class="g" x1="{plot_left}" x2="{plot_right}" y1="{y(value):.1f}" '
            f'y2="{y(value):.1f}" stroke-width="1"/>'
        )
        out.append(
            f'<text class="s" x="{plot_left - 6}" y="{y(value) + 4:.1f}" font-size="11" '
            f'text-anchor="end">{value:g}</text>'
        )
    out.append(
        f'<line class="st" x1="{plot_left}" x2="{plot_right}" y1="{y(param.start):.1f}" '
        f'y2="{y(param.start):.1f}" stroke-width="1" stroke-dasharray="4 3"/>'
    )
    for games in (0, total // 2, total):
        out.append(
            f'<text class="s" x="{x(games):.1f}" y="{top + MARGIN_TOP + PLOT_HEIGHT + 16}" '
            f'font-size="11" text-anchor="middle">{games / 1000:.0f}k</text>'
        )
    points = " ".join(f"{x(games):.1f},{y(value):.1f}" for games, value in param.points)
    out.append(
        f'<polyline class="ln" points="{points}" fill="none" stroke-width="2" '
        f'stroke-linejoin="round"/>'
    )
    out.append(
        f'<circle class="dot" cx="{x(final_games):.1f}" cy="{y(final):.1f}" r="4" '
        f'stroke-width="2"><title>{escape(name)}: {final:.2f} at {final_games:,} games'
        f"</title></circle>"
    )
    return out


def render(params: dict[str, Parameter], info: dict, total: int) -> str:
    """Draws every parameter's chart, three to a row, under a title naming the tune."""
    rows = -(-len(params) // COLUMNS)
    width = 20 + COLUMNS * (PANEL_WIDTH + 10)
    height = HEADER + rows * (PANEL_HEIGHT + 40)
    title = (
        f"SPSA tune {info.get('id', '?')}, {info.get('dev_time_control', '?')}: "
        "value against games played"
    )
    out = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" '
        f'width="{width}" height="{height}" '
        'font-family="system-ui,-apple-system,Segoe UI,sans-serif">',
        f"<style>{STYLE}</style>",
        f'<rect class="bg" width="{width}" height="{height}"/>',
        f'<text class="t" x="20" y="26" font-size="16" font-weight="600">{escape(title)}</text>',
        '<text class="s" x="20" y="44" font-size="12">Y axis spans each parameter\'s full '
        f"tuning range; dashed line is the start value. {total:,} games.</text>",
    ]
    for index, (name, param) in enumerate(params.items()):
        left = 20 + (index % COLUMNS) * (PANEL_WIDTH + 10)
        top = HEADER + (index // COLUMNS) * (PANEL_HEIGHT + 40)
        out += panel(name, param, total, left, top)
    out.append("</svg>")
    return "\n".join(out) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("log", help="the spsa.<tune id>.log file track_spsa.py wrote")
    parser.add_argument("output", help="the SVG file to write")
    args = parser.parse_args()

    params, info = read_log(args.log)
    total = max(param.points[-1][0] for param in params.values())
    print_drift(params, total)
    with open(args.output, "w", encoding="utf-8") as out:
        out.write(render(params, info, total))


if __name__ == "__main__":
    main()
