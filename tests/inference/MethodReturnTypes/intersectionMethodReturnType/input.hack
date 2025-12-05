interface Foo {
	public function getItems(): Awaitable<vec<Foo>>;
	public function onlyAvailableOnFoo(): void;
}

interface Bar {
	public function getItems(): Awaitable<vec<Bar>>;
	public function onlyAvailableOnBar(): void;
}

async function test(Foo $obj): Awaitable<void> {
	if ($obj is Bar) {
		// $obj is now Foo & Bar
		// getItems() should return vec<AChild> (intersection of vec<A> and vec<AChild>)
		$items = await $obj->getItems();
		foreach ($items as $item) {
			// This should work since $item should be AChild
			$item->onlyAvailableOnFoo();
			$item->onlyAvailableOnBar();
		}
	}
}
