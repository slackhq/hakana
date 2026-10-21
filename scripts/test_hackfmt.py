#!/usr/bin/env python3
"""Run Hakana against HHVM's original hackfmt fixtures, without copying them.

Build with `cargo build --release --bin hakana`, then run this script. A separate
formatter binary can be tested with --program PATH --standalone. Comparisons
preserve indentation, unlike HHVM's general-purpose verify.py. Its comparison
allows a terminal newline and, through filter_ocaml_stacktrace, one extra trailing
blank line. We retain that compatibility for CLI fixtures; OCaml range-unit
assertions are byte-for-byte. No expectations are generated or updated.
--filter selects case names; the default runs all.
"""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
import difflib
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
UPSTREAM = ROOT / 'third-party/hhvm/hphp/hack/test'

@dataclass
class Case:
    name: str
    cwd: Path
    args: list[str]
    expected: bytes
    stdin: bytes | None = None
    idempotence: bool = False
    json_lines: bool = False
    status: int = 0
    exact: bool = False


def ocaml_string(s: str) -> str:
    # The upstream range tests use ordinary strings and String.make expressions.
    s = s.strip()
    if s.startswith('(') and s.endswith(')'):
        s = s[1:-1]
    tokens = re.finditer(r'"((?:[^"\\]|\\.)*)"|String\.make\s+(\d+)\s+\'(.)\'', s, re.S)
    result = ''
    for m in tokens:
        if m.group(1) is not None:
            result += re.sub(r'\\([nrt"\\])', lambda x: {'n':'\n','r':'\r','t':'\t','"':'"','\\':'\\'}[x[1]], m[1])
        else:
            result += m[3] * int(m[2])
    return result


def cases() -> list[Case]:
    base = UPSTREAM / 'hackfmt'
    result = []
    for suite in ['tests', 'range_tests', 'check_formatting']:
        for path in sorted((base / suite).rglob('*.php')):
            flags = path.with_suffix('.flags')
            args = ['--test', path.name]
            if suite == 'check_formatting':
                args += ['--check-formatting']
            if flags.exists():
                args += shlex.split(flags.read_text())
            expected = Path(str(path) + '.exp').read_bytes()
            result.append(Case(str(path.relative_to(UPSTREAM)), path.parent, args, expected,
                               idempotence=suite == 'tests',
                               status=3 if expected.startswith(b'Invalid argument:') else 0))
    for suite in ['difftests', 'difftests-megarepo']:
        for path in sorted((base / suite).glob('*.diff')):
            exp = Path(str(path) + '.exp')
            root = 'root' if suite == 'difftests' else 'megarepo/www'
            expected = exp.read_bytes() if exp.exists() else b''
            result.append(Case(str(path.relative_to(UPSTREAM)), path.parent,
                ['--test', '--diff-dry-run', '--root', root], expected, path.read_bytes(),
                status=4 if expected.startswith(b'Invalid diff:') else 0))
    for suite in ['linttool', 'linttool_with_severity']:
        for path in sorted((base / suite).glob('*.txt.exp')):
            args = ['--linttool', path.name[:-4], 'passed-by-file-name.php']
            if suite.endswith('severity'):
                args += ['--linttool-severity', 'advice']
            result.append(Case(str(path.relative_to(UPSTREAM)), path.parent, args,
                               path.read_bytes(), json_lines=True))
    for path in sorted((UPSTREAM / 'enum_class_label/format').glob('*.php')):
        result.append(Case(str(path.relative_to(UPSTREAM)), path.parent,
            ['--test', path.name], Path(str(path) + '.exp').read_bytes(), idempotence=True))
    for path in sorted((base / 'ide_format').glob('*.php')):
        result.append(Case(str(path.relative_to(UPSTREAM)), path.parent,
            ['--test-ide', path.name], Path(str(path) + '.exp').read_bytes()))
    source = (base / 'hackfmt_range_unit_test.ml').read_text()
    source = re.sub(r'\(\*.*?\*\)', '', source, flags=re.S)
    sum_source = re.search(r'let sum = ("(?:[^"\\]|\\.)*")', source, re.S)[1]
    pattern = r'"([^"]+)"\s*>:: assert_range_formats_to\s*~src:(.*?)\s*~rng:\(\((\d+),\s*(\d+)\),\s*("(?:[^"\\]|\\.)*")\)\s*~exp:("(?:[^"\\]|\\.)*")'
    matches = list(re.finditer(pattern, source, re.S))
    if len(matches) != source.count('>:: assert_range_formats_to'):
        raise RuntimeError('Could not discover every upstream range unit test')
    for m in matches:
        name, src, start, end, substring, expected = m.groups()
        text = ocaml_string(sum_source if src.strip() == 'sum' else src).encode()
        assert text[int(start):int(end)] == ocaml_string(substring).encode(), name
        result.append(Case('hackfmt/range_unit/' + name, base,
            ['--test-raw-range', start, end], ocaml_string(expected).encode(), text,
            exact=True))
    return result


def run(case: Case, command: list[str], timeout: int) -> str | None:
    def invoke(args: list[str], stdin: bytes | None):
        return subprocess.run(command + args, input=stdin, cwd=case.cwd,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=timeout)
    try:
        p = invoke(case.args, case.stdin)
        actual = p.stdout
        if case.json_lines:
            equal = [json.loads(x) for x in actual.splitlines()] == [json.loads(x) for x in case.expected.splitlines()]
        elif case.exact:
            equal = actual == case.expected
        else:
            # verify.py's strip_lines and filter_ocaml_stacktrace each split
            # and rejoin the output. Preserve indentation but match its handling
            # of terminal line endings, including the second split/join.
            expected = b'\n'.join(case.expected.splitlines())
            output = b'\n'.join(actual.splitlines())
            equal = expected == output or expected == b'\n'.join(output.splitlines())
        if not equal or p.returncode != case.status:
            diff = ''.join(difflib.unified_diff(case.expected.decode(errors='replace').splitlines(True),
                actual.decode(errors='replace').splitlines(True), fromfile='upstream', tofile='hakana'))
            return f'{case.name}: exit {p.returncode} (expected {case.status})\n{diff}'
        if case.idempotence:
            args = [x for x in case.args if x != case.args[1]]
            again = invoke(args, p.stdout)
            if again.returncode or again.stdout.splitlines() != p.stdout.splitlines():
                return f'{case.name}: formatting is not idempotent\n' + ''.join(
                    difflib.unified_diff(p.stdout.decode().splitlines(True), again.stdout.decode().splitlines(True)))
    except (OSError, subprocess.TimeoutExpired, ValueError) as e:
        return f'{case.name}: {e}'
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--program', type=Path, default=ROOT / 'target/release/hakana')
    parser.add_argument('--standalone', action='store_true')
    parser.add_argument('--filter', default='')
    parser.add_argument('--list', action='store_true')
    parser.add_argument('--jobs', type=int, default=8)
    parser.add_argument('--timeout', type=int, default=30)
    parser.add_argument('--failures', type=Path, help='Write complete failure details here')
    args = parser.parse_args()
    selected = [c for c in cases() if args.filter in c.name]
    if not selected:
        parser.error('No matching upstream tests')
    if args.list:
        print('\n'.join(c.name for c in selected))
        print(f'{len(selected)} upstream cases')
        return 0
    command = [str(args.program.resolve())] + ([] if args.standalone else ['format'])
    with ThreadPoolExecutor(max_workers=args.jobs) as executor:
        results = list(executor.map(lambda c: run(c, command, args.timeout), selected))
    failures = [r for r in results if r is not None]
    report = '\n\n'.join(failures)
    if args.failures:
        args.failures.write_text(report)
        print('\n'.join(f.splitlines()[0] for f in failures))
    elif report:
        print(report)
    print(f'{len(selected) - len(failures)}/{len(selected)} upstream cases passed')
    return bool(failures)

if __name__ == '__main__':
    sys.exit(main())
