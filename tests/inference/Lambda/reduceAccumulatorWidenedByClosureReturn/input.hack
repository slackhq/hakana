<<__Sealed(HasGet::class, NoGet::class)>>
abstract class Base {}

final class HasGet extends Base {
	public function __construct(private string $s) {}
	public function get(): string {
		return $this->s;
	}
}

final class NoGet extends Base {
	public function reason(): int {
		return 0;
	}
}

function reduce<Tv, Ta>(
	Traversable<Tv> $_,
	(function(Ta, Tv): Ta) $_,
	Ta $initial,
): Ta {
	return $initial;
}

function test(vec<int> $items): Base {
	return reduce(
		$items,
		($accum, $item) ==> {
			if ($item > 0) {
				return new NoGet();
			}
			// A prior iteration can return NoGet, so $accum is not provably
			// HasGet here: its type is the union HasGet|NoGet, and get() exists
			// only on HasGet.
			return new HasGet($accum->get());
		},
		new HasGet("init"),
	);
}
