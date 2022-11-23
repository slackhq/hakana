function foo(vec<string> $strs): vec<string> {
    /* HAKANA_FIXME[RedundantIssetCheck] */
    $a = $strs ?? null;
    
    if ($a is nonnull) {}
}