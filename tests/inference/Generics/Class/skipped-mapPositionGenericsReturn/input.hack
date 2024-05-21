final class A<T> {
	public function __construct(private (function(T): string) $fn) {}
}

function foo(): A<string> {
    return new A(($t) ==> $t);
}