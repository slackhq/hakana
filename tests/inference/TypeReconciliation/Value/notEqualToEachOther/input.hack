class A {}

function example(A $a, A $b): bool {
    /* HAKANA_IGNORE[StrictObjectEquality] */
    if ($a !== $b) {
        return true;
    }

    if (\get_class($a) === \get_class($b)) {
        return true;
    }

    return false;
}