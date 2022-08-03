function foo(dict<string, int> $arr, int $l): bool {
	$a = $arr['a'] ?? null;
	$b = $arr['b'] ?? null;
	$c = $arr['c'] ?? null;
	$d = $arr['d'] ?? null;
	$e = $arr['e'] ?? null;

	$s = false;

	if (
		($a && $a > $l) ||
		($b && $b > $l) ||
		($c && $c > $l) ||
		($d && $d > $l) ||
		($e && $e > $l)
	) {
		$s = true;
	}

	return $s;
}