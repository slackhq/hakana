function x(?string $foo): void {
    if (!C\contains(vec["foo", "bar", null], $foo)) {
        throw new Exception();
    }

    if ($foo) {}
}