function foo(inout dict<string, mixed> $arr, int $_b): void {
	$arr['a'] = 5;
}

<<__EntryPoint>>
function bar(): void {
	$b = 5;
	$barr = dict[];
	foo(
		inout $barr,
		$b,
	);
}

function baz(): void {
	$b = 5;
	$barr = dict[];
	foo(inout $barr, $b);
}