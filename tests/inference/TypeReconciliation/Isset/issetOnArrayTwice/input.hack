function foo(dict<arraykey, mixed> $options): void {
    if (!isset($options["a"])) {
        $options["a"] = "hello";
    }

    if (!isset($options["b"])) {
        $options["b"] = 1;
    }

    if ($options["b"] === 2) {}
}