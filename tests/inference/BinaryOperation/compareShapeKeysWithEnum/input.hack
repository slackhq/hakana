enum FeeNum: string as string {
	A = 'a';
	B = 'b';
}

function foo(
	shape('a' => FeeNum, ...) $a,
	?shape('a' => string, 'b' => string) $b,
): void {
	if ($a === $b) {
	}
}