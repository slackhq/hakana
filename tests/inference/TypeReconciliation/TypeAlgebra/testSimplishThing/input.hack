function foo(
    bool $a,
    bool $b,
    bool $c,
    bool $d,
    bool $e,
    bool $f,
    bool $g,
    bool $h,
    bool $i,
    bool $j
): bool {
    return ($a && $b)
        || ($c && $d)
        || ($e && $f)
        || ($g && $h)
        || ($i && $j);
}