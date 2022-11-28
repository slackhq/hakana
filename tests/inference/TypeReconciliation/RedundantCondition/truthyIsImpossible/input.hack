function foo(bool $b, bool $c): void {
    if (!$b && $c) {
    	if ($b) {}
    }
}