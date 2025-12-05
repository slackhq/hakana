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
	public function getItems(): vec<A>;
}

interface Bar {
	public function getItems(): vec<AChild>;
}

function test(Foo $obj): void {
	if ($obj is Bar) {
		// $obj is now Foo & Bar
		// getItems() should return vec<AChild> (intersection of vec<A> and vec<AChild>)
		$items = $obj->getItems();
		foreach ($items as $item) {
			// This should work since $item should be AChild
			$item->getChildValue();
		}
	}
}
