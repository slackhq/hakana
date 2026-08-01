// Builtin types referenced through the HH namespace are the same types.
function f(HH\keyset<string> $k, HH\vec<int> $v, HH\dict<string, int> $d): HH\keyset<string> {
	$out = keyset[];
	foreach ($k as $s) {
		$out[] = $s;
	}
	$_ = $v;
	$_ = $d;
	return $out;
}

function g(keyset<string> $k): HH\keyset<string> {
	return $k;
}

function h(int $i, HH\string $s, bool $b, num $n): arraykey {
	$_ = $b;
	$_ = $n;
	if ($i > 0) {
		return $i;
	}
	return $s;
}
