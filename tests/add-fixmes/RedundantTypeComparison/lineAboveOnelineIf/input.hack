function foo(string $s): void {
	$a = null;
	if (rand(0, 1)) $a = $s is string ? 'a' : 'b';
	$b = (mixed $a) ==> $s is string ? $a : null;
	$c = (mixed $a) ==> {
		if ($s is string) {
			return $a;
		}
		return null;
	};
}