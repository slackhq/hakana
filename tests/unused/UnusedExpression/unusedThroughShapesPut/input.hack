function bar(
	string $str,
	int $value,
): void {
	$s = shape('a' => $str);
	$result = Shapes::put($s, 'y', $value);
	echo $result['y'];
}