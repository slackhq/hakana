function foo(vec<string> $strs): vec<string> {
    /* HAKANA_FIXME[RedundantTypeComparison] */
    $a = $strs ?? null;
    
    if ($a is nonnull) {}
}