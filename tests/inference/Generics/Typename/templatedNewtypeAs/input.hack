function foo(stringkey_dict<string> $dict) {
    /* HAKANA_FIXME[PossiblyUndefinedStringArrayOffset] */
    echo $dict["a"];
    $b = $dict["b"] ?? null;
    if ($b is nonnull) {
        echo $b;
    }

    foreach ($dict as $k => $v) {
        echo $k;
        echo $v;
    }
}