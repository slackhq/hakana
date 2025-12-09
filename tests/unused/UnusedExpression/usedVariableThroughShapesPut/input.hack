function foo(
	shape('x' => int) $s,
	int $value,
): shape('x' => int, 'y' => int) {
	$result = Shapes::put($s, 'y', $value);
	return $result;
}

function baz(
	string $str,
	int $value,
): void {
	$s = shape('a' => $str);
	$result = Shapes::put($s, 'y', $value);
	echo $result['a'];
	echo $result['y'];
}
