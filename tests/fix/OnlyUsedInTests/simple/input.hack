final class Foo {
	public function isCalled(): void {
		echo "cool";
	}

	/** cool doc */
	public function bar(): void {
	}

	public function baz(): void {
		echo "got here";
	}
}

<<__EntryPoint>>
function main(): void {
	$foo = new Foo();
	$foo->isCalled();
}

<<\Hakana\TestOnly>>
function test_only(): void {
	$foo = new Foo();
	$foo->bar();
	$foo->baz();
}