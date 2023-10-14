function foo($a, vec<string> $cs, string $d): void {
	$dict = dict[];
	$dict['foo'] = $a ?? false;

	foreach ($cs as $c) {
		$dict[$c] = 'bar';
	}

	if ($dict['c_'.$d] ?? null) {}
}