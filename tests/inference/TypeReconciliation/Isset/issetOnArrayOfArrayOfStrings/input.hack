function foo(int $i, dict<int, dict<string, string>> $tokens) : void {
    if (!isset($tokens[$i]["a"])) {
        /* HAKANA_FIXME[PossiblyUndefinedStringArrayOffset] */
        echo $tokens[$i]["b"];
    }
}