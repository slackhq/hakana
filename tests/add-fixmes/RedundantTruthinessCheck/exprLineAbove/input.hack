function foo(string $s) {
	do {
		$result = rand(0,1) === 1;
	} while (!$result);

	if (
		$result
	) {}
}