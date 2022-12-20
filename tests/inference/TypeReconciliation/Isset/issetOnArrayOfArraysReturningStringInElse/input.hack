function foo(int $i, dict<int, dict<string, string>> $tokens) : string {
    if (isset($tokens[$i]["a"])) {
        return "hello";
    } else {
        /* HAKANA_FIXME[PossiblyUndefinedStringArrayOffset] */
        return $tokens[$i]["b"];
    }
}