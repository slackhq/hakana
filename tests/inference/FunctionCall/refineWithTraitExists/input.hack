function foo(string $s) : void {
    if (trait_exists($s)) {
        new ReflectionClass($s);
    }
}