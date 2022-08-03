function example(object $a, object $b): bool {
    if ($a !== $b && \get_class($a) === \get_class($b)) {
        return true;
    }

    return false;
}