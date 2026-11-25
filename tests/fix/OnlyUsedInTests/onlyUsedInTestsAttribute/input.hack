final class Foo {
	public function isCalled(): void {
		echo "cool";
	}

	public function bar(): void {
	}

	<<\Hakana\OnlyUsedInTests>>
	public function baz(): void {
		echo "got here";
	}
}

<<__EntryPoint>>
function main(): void {
	$foo = new Foo();
	$foo->isCalled();
}

<<\Hakana\OnlyUsedInTests>>
function only_used_in_tests(): void {
	$foo = new Foo();
	$foo->bar();
	$foo->baz();
}
