function foo(dict<string, dict<int, string>> $arr, string $k) : void {
    if (!isset($arr[$k])) {
        return;
    }

    /* HAKANA_FIXME[PossiblyUndefinedIntArrayOffset] */
    if ($arr[$k][0]) {}
}