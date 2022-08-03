function foo(string $s) : void {
    if (class_exists($s)) {
        new $s();
    }
}