function foo(mixed $a, bool $b): void {
    if (!($a is string && $b)) {
    	if ($a is null) {}
    }
}