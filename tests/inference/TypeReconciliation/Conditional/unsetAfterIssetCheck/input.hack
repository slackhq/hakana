function checkbox(dict<string, mixed> $options = dict[]) : void {
    /* HAKANA_FIXME[PossiblyUndefinedStringArrayOffset] */
    if ($options["a"]) {}

    unset($options["a"], $options["b"]);
}