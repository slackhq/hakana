function foo(dict<string, int> $arr, int $l): bool {
	$a = $arr['a'] ?? null;
	$b = $arr['b'] ?? null;
	$c = $arr['c'] ?? null;
	$d = $arr['d'] ?? null;
	$e = $arr['e'] ?? null;

	$s = false;

	if (
		($a !== null && $a > $l) ||
		($b !== null && $b > $l) ||
		($c !== null && $c > $l) ||
		($d !== null && $d > $l) ||
		($e !== null && $e > $l)
	) {
		$s = true;
	}

	return $s;
}