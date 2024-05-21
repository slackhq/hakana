abstract class A {
	const type TResult = string;

	public static function foo(): this::TResult {
		return "hello";
	}
}

final class B extends A {}

function bar(): string {
	return B::foo();
}
