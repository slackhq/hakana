function x(?string $foo): void {
    if (!in_array($foo, vec["foo", "bar", null], true)) {
        throw new Exception();
    }

    if ($foo) {}
}