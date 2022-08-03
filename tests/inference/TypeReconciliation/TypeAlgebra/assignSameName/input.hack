function foo(string $value): string {
    $value = "yes" === $value;
    return !$value ? "foo" : "bar";
}