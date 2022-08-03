function foo(string $s1, string $s2) : string {
    if ($s1 !== "hello") {
        if ($s1 !== "goodbye") {
            return $s1;
        }
    }

    return $s2;
}