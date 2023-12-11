function foo(
	dict<string, mixed> $args,
): vec<int> {
	$args = HH\Lib\Vec\map($args, $arg ==> bat($arg));
	return $args;
}

function bat(mixed $i): int {
    return (int)$i + 1;
}