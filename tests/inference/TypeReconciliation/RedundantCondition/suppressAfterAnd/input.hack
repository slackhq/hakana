function foo(string $a): void {
    if (
        rand(0, 1) &&
        /* HAKANA_FIXME[RedundantNonnullTypeComparison] */
        $a is nonnull
    ) {
        echo $a;
    }
}