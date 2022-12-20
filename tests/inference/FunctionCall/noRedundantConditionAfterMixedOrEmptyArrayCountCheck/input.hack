function foo(string $s) : void {
    /* HAKANA_FIXME[PossiblyUndefinedStringArrayOffset] */
    $a = $_GET["s"] ?: vec[];
    if (count($a)) {}
    if (!count($a)) {}
}