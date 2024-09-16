function foo(vec<mixed> $vecs): vec<nonnull> {
    return HH\Lib\Vec\filter_nulls($vecs);
}