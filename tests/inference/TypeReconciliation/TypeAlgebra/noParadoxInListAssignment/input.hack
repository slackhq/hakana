function foo(string $a): void {
    if (!$a) {
        list($a) = explode(":", "a:b");

        if ($a) { }
    }
}