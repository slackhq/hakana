interface A {}
final class B {}

function bar(A $a): void {
	if ($a is B) {}
}