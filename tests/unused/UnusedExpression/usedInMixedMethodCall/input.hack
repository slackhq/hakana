function foo(mixed $m): void {
    $a = 1;
    /* HAKANA_FIXME[MixedMethodCall] */
    $m->bar($a);
}