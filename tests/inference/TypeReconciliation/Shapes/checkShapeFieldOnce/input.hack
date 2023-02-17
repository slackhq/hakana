function foo(shape('bar' => string, ?'baz' => int) $s) {
    /* HAKANA_FIXME[RedundantNonnullEntryCheck] */
    if (isset($s['bar'])) {
        if (isset($s['baz'])) {}
    }
}