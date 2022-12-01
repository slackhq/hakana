function foo(dict<string, string> $d): void {
    if ($d is KeyedContainer<_, _>) {}
}