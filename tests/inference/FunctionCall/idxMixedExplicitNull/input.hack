function foo(dict<string, mixed> $args): void {
	$a = idx($args, 'a', null);
    if ($a === null) {}
}