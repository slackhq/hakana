class A<T> {
	public function __construct(private (function(T): string) $fn) {}
}

function takesAString(A<string> $a): void {}

function bar(): void {
    takesAString(new A(($t) ==> $t));
}