function foo(string $a): void {
    if ($a === "foo") {
        // do something
    } else if ($a === "bar") {
        // can never get here
    }
}