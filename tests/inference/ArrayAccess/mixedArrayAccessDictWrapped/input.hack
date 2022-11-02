function foo(mixed $m): void {
    $a = dict(
        /* HH_FIXME[4110] */ $m
    );

    echo $a["foo"];
}