function foo(string $s): mixed {
    return @file_get_contents($s);
}