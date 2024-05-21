final class Reesource<T> {
	public function __construct(
		private typename<T> $t,
	) {}
}

final class A {}

function expectsResourceA(Reesource<A> $_res): void {}

function bar(): void {
	$a = new Reesource(A::class);
	expectsResourceA($a);
}