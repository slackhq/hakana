function foo(string $s): void {
	$a = 0;

	while (rand(0, 1)) {
		$a++;

		if ($s is string) {}
	}
}
