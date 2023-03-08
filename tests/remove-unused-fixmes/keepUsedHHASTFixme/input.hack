function foo(
    /* HHAST_FIXME[UnusedParameter] */
    string $s
) {
    /* HHAST_FIXME[UnusedVariable] */
    $a = "";
    /* HHAST_FIXME[UnusedVariable] */
    $b = "";
    echo $b;
}