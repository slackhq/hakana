function foo(): void {
	$a = 0;
	if (rand(0, 1) !== 0) {
		$a = 1;
	} else {
		$a = 2;
	}
	echo $a;
}
