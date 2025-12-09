function foo(
	shape('x' => int) $s,
	int $value,
): shape('x' => int, 'y' => int) {
	$result = Shapes::put($s, 'y', $value);
	return $result;
}

function bar_without_put(
	string $str,
	int $value,
): void {
	$s = shape('a' => $str);
    $result = $s;
	$result['y'] = $value;
	echo $result['y'];
}

function bar(
	string $str,
	int $value,
): void {
	$s = shape('a' => $str);
	$result = Shapes::put($s, 'y', $value);
	echo $result['y'];
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
