function foo(dict<string, mixed> $args): void {
	$a = idx($args, 'a', null);
    $b = idx($args, 'b');
    if ($a === null) {}
    if ($b === null) {}
}