function foo(
	dict<string, mixed> $args,
): mixed {
	$limit = idx($args, 'limit', 10);
	return $limit;
}