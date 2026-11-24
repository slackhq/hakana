function dead_disjunct(bool $a, bool $b, bool $c): string {
    if ($a && $b) {
        return 'both';
    }

    if (($a && $b) || $c) {
        return 'c';
    }

    return 'neither';
}

function dead_condition(bool $a, bool $b): string {
    if ($a && $b) {
        return 'both';
    }

    if ($a && $b) {
        return 'again';
    }

    return 'neither';
}
