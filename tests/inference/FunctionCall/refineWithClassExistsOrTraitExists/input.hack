function foo(string $s) : void {
    if (trait_exists($s) || class_exists($s)) {
        new ReflectionClass($s);
    }
}

function bar(string $s) : void {
    if (class_exists($s) || trait_exists($s)) {
        new ReflectionClass($s);
    }
}

function baz(string $s) : void {
    if (class_exists($s) || interface_exists($s) || trait_exists($s)) {
        new ReflectionClass($s);
    }
}