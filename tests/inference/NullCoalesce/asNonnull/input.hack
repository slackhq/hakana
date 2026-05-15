function foo(?string $a): string {
    /* HAKANA_FIXME[RedundantIssetCheck] */
    $b = ($a ?? null) as nonnull;
    return $b;
}
