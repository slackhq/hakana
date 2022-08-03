function foo(string $s1, string $s2, ?int $i) : string {
    if ($s1 !== $s2) {
        return $s1;
    }

    return $s2;
}