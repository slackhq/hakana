class A {
	const type TResult = string;

	public static function foo(): this::TResult {
		return "hello";
	}
}

class B extends A {}

function bar(): string {
	return B::foo();
}
