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