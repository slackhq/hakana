#!/usr/bin/env python3
"""Benchmark hakana format on a source directory without changing its files.

Use --baseline with a saved pre-change hakana binary to alternate timing runs and
compare every formatted file byte-for-byte. Output comparisons use temporary
copies with the source project's .hhconfig. --profile records a samply profile
separately from the timing runs. Build release binaries before benchmarking.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import shutil
import statistics
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
EXTENSIONS = {'.hack', '.php', '.hh', '.hhi'}


def invoke(command: list[str]) -> subprocess.CompletedProcess:
    result = subprocess.run(command, capture_output=True)
    # hackfmt-compatible check/in-place modes return zero even on parse errors.
    if result.returncode or result.stderr:
        raise RuntimeError(
            f'{command[0]} exited {result.returncode}:\n'
            + result.stderr.decode(errors='replace')
        )
    return result


def compare_outputs(programs: dict[str, Path], paths: list[Path],
                    directory: Path, config_root: Path) -> None:
    with tempfile.TemporaryDirectory(prefix='hakana-format-benchmark-') as temp:
        for name, program in programs.items():
            copies = []
            for path in paths:
                copy = Path(temp) / name / path.relative_to(directory)
                copy.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, copy)
                copies.append(str(copy))
            invoke([str(program), 'format', '--root', str(config_root),
                    '--in-place', *copies])
        different = [
            path.relative_to(directory) for path in paths
            if (Path(temp) / 'baseline' / path.relative_to(directory)).read_bytes()
            != (Path(temp) / 'current' / path.relative_to(directory)).read_bytes()
        ]
        if different:
            raise RuntimeError('Output changed:\n' + '\n'.join(map(str, different)))
        print(f'All {len(paths)} formatted outputs match byte-for-byte.', flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('--program', type=Path, default=ROOT / 'target/release/hakana')
    parser.add_argument('--baseline', type=Path)
    parser.add_argument('--root', type=Path, help='Directory containing .hhconfig')
    parser.add_argument('--runs', type=int, default=5)
    parser.add_argument('--json', type=Path, help='Write timing results as JSON')
    parser.add_argument('--profile', type=Path, help='Save a samply profile (.json.gz)')
    args = parser.parse_args()
    directory = args.directory.resolve()
    if not directory.is_dir() or args.runs < 1:
        parser.error('An existing source directory and at least one run are required')
    paths = sorted(p for p in directory.rglob('*')
                   if p.is_file() and p.suffix in EXTENSIONS)
    if not paths:
        parser.error('No Hack source files found')
    config_root = args.root.resolve() if args.root else next(
        (p for p in paths[0].parents if (p / '.hhconfig').is_file()), Path.cwd()
    )
    programs = {}
    if args.baseline:
        programs['baseline'] = args.baseline.resolve()
    programs['current'] = args.program.resolve()
    commands = {
        name: [str(program), 'format', '--root', str(config_root),
               '--check-formatting', *map(str, paths)]
        for name, program in programs.items()
    }
    size = sum(p.stat().st_size for p in paths)
    print(f'{len(paths)} files, {size:,} bytes; config root: {config_root}', flush=True)
    # Warm each binary and the filesystem cache before the measured runs.
    expected_checks = {name: invoke(cmd).stdout for name, cmd in commands.items()}
    times: dict[str, list[float]] = {name: [] for name in programs}
    for run in range(args.runs):
        order = list(commands)
        if run % 2:
            order.reverse()
        for name in order:
            start = time.perf_counter()
            result = invoke(commands[name])
            elapsed = time.perf_counter() - start
            if result.stdout != expected_checks[name]:
                raise RuntimeError(f'{name}: check output changed between runs')
            times[name].append(elapsed)
            print(f'{name} run {run + 1}: {elapsed:.3f}s', flush=True)
    medians = {name: statistics.median(values) for name, values in times.items()}
    for name, median in medians.items():
        print(f'{name} median: {median:.3f}s ({size / median / 1_000_000:.2f} MB/s)')
    if args.baseline:
        print(f'Speedup: {medians["baseline"] / medians["current"]:.2f}x', flush=True)
        compare_outputs(programs, paths, directory, config_root)
    if args.json:
        args.json.write_text(json.dumps({
            'directory': str(directory), 'files': len(paths), 'bytes': size,
            'config_root': str(config_root),
            'programs': {name: str(path) for name, path in programs.items()},
            'seconds': times, 'median_seconds': medians,
        }, indent=2) + '\n')
    if args.profile:
        subprocess.run(
            ['samply', 'record', '--save-only', '--unstable-presymbolicate',
             '--main-thread-only', '-o', str(args.profile),
             '--', *commands['current']],
            check=True,
        )


if __name__ == '__main__':
    main()
