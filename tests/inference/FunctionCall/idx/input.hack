function foo(dict<string, mixed> $args): void {
	$cookies_arg = idx($args, 'cookies', null);
    if ($cookies_arg === null) {}
}