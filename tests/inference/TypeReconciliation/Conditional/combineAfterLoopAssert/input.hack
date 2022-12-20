function foo(dict<string, string> $array) : void {
    $c = 0;

    /* HAKANA_FIXME[PossiblyUndefinedStringArrayOffset] */
    if ($array["a"] === "a") {
        foreach (vec[rand(0, 1), rand(0, 1)] as $i) {
            /* HAKANA_FIXME[PossiblyUndefinedStringArrayOffset] */
            if ($array["b"] === "c") {}
            $c++;
        }
    }
}