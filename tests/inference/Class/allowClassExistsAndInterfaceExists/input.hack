function foo(string $s) : void {
    if (class_exists($s) || interface_exists($s)) {}
}