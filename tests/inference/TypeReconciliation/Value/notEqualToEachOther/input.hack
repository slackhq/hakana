class A {}

function example(A $a, A $b): bool {
    if ($a !== $b && \get_class($a) === \get_class($b)) {
        return true;
    }

    return false;
}