abstract class A {
	public function getValue(): int {
		return 0;
	}
}

final class AChild extends A {
	public function getChildValue(): string {
		return "child";
	}
}

interface Foo {
	public function getItemAsync(): Awaitable<A>;
}

interface Bar {
	public function getItemAsync(): Awaitable<AChild>;
}

async function test(Foo $obj): Awaitable<void> {
	if ($obj is Bar) {
		// $obj is now Foo & Bar
		// getItemAsync() should return Awaitable<AChild>
		// (intersection of Awaitable<A> and Awaitable<AChild>)
		$item = await $obj->getItemAsync();
		// This should work since $item should be AChild
		$item->getChildValue();
	}
}
