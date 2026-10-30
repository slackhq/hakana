# Profiling retained memory

Build the optional object-graph profiler and select an output directory:

```sh
cargo build --release --bin hakana --features hakana-cli/memory-profile
HAKANA_MEMORY_PROFILE=/tmp/hakana-memory \
  target/release/hakana analyze --root /path/to/project --no-cache --threads 8
```

Custom binaries that depend on `hakana-cli` can enable the same dependency
feature in their own build. The environment variable also works when such a
build runs as a language server.

Each scan writes a matching pair named
`<pid>-<scan-number>-before-populate.json` and
`<pid>-<scan-number>-after-populate.json`. The checkpoints surround codebase
population and its read-only `after_populate` hooks. Snapshots cover:

- Codebase metadata, broken down into classes, functions, aliases, files,
  inheritance maps, and their fields.
- Interned strings and resolved names.
- File hashes/timestamps and symbol references.
- Shared `Arc`/`Rc` allocations, counted once across all roots.

`inline_type_bytes` reports the sizes of class, function, and parameter records.
`counts` also reports how many parameters have defaults, functions have nonempty
generic contexts, and classes have enum types, to help identify rarely used
fields that inflate every record.

Compare the snapshots:

```sh
python3 scripts/compare_memory_profiles.py /tmp/hakana-memory/123-0-before-populate.json /tmp/hakana-memory/123-0-after-populate.json
python3 scripts/compare_memory_profiles.py /tmp/hakana-memory/123-0-before-populate.json /tmp/hakana-memory/123-0-after-populate.json --type ClassLikeInfo
python3 scripts/compare_memory_profiles.py /tmp/hakana-memory/123-0-before-populate.json /tmp/hakana-memory/123-0-after-populate.json --type TUnion
```

The sibling `.folded` files contain byte-weighted object paths compatible with
FlameGraph or `inferno-flamegraph`. These show ownership paths, not allocation
call stacks. The JSON `allocations` entries are disjoint byte contributions;
sum paths sharing a prefix to inspect a subtree. Shared allocations live at
separate roots, so they are not arbitrarily charged to the first referencing
field. A field's displayed subtotal therefore excludes shared pointees.

## What the estimates include

The profiler traverses the actual objects without cloning or serializing the
codebase. It counts inline structure sizes, vector and string capacities,
reachable allocations, and estimated hash-table/index storage, including spare
capacity. Hash-table layout is inferred from public capacity APIs; internal
layout, tombstones, and IndexMap's private capacities can introduce error.
B-tree maps include live entry payload and descendants, but not node headers
or unused slots, because their capacity is not exposed.

These are retained-size estimates, not exact malloc accounting. They exclude
allocator rounding/metadata, freed memory retained by the allocator, stacks,
unreachable temporaries, and state outside the listed roots. In particular,
they do not measure the temporary peak *inside* population. Current process RSS
is sampled before each traversal for context; an earlier traversal can affect
allocator retention and therefore the later RSS.

The comparison script rejects snapshots with accounting warnings or inconsistent
totals. Profiling does not run unless both the compile-time feature and the
environment variable are enabled. Profiling builds still continue through normal
analysis, so their diagnostics can be compared against an ordinary build.
