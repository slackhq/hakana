function foo(string $s) {
	do {
		$result = (bool) rand(0,1);
	} while (!$result);

	if ($result) {}
}