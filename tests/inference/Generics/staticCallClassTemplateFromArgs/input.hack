enum MyType: string {
	A = 'a';
	B = 'b';
}

final class Filter<T> {
	public function __construct(public vec<T> $vals) {}

	public static function exclude(T ...$values): Filter<T> {
		return new Filter(vec($values));
	}
}

function get_types(): vec<MyType> {
	return vec[MyType::A];
}

function f(): Filter<MyType> {
	return Filter::exclude(...get_types());
}

function g(): Filter<MyType> {
	return Filter::exclude(MyType::A, MyType::B);
}

// literal-precise arg types are widened when solving class templates from a
// static factory call — the object is mutable, so later writes with other
// literals must be allowed
final class Deque<T> {
	public function __construct(private vec<T> $items) {}

	public static function fromVec(vec<T> $items): Deque<T> {
		return new Deque($items);
	}

	public function pushFront(T $item): void {
		$this->items = \HH\Lib\Vec\concat(vec[$item], $this->items);
	}

	public function peekFront(): ?T {
		return $this->items[0] ?? null;
	}
}

function deque_patterns(): void {
	$deque = Deque::fromVec(vec[
		shape('a' => vec['a', 'b', 'c'], 'b' => 40),
		shape('a' => vec['d', 'e'], 'b' => 1000),
	]);
	$deque->pushFront(shape('a' => vec['y', 'z'], 'b' => -20));
	if ($deque->peekFront() === shape('a' => vec['y', 'z'], 'b' => -20)) {
		echo "front matches";
	}
}
