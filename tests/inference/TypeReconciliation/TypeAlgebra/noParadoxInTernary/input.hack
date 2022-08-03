function foo(?bool $b) : string {
    return $b ? "a" : ($b === null ? "foo" : "b");
}