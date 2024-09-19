function foo(string $s) : void {
    /* HAKANA_FIXME[PossiblyUndefinedStringArrayOffset] */
    $a = (HH\global_get('_GET') as dict<_,_>)["s"] ?: vec[];
    if (count($a)) {}
    if (!count($a)) {}
}