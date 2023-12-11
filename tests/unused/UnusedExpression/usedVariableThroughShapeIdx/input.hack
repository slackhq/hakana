function foo(
	shape(?'limit' => int) $args,
): int {
	$limit = Shapes::idx($args, 'limit', 10);
	return $limit;
}