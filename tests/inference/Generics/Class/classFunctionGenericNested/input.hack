abstract class A {}

final class B {
	public static function coerce<reify T as A>(
		vec<A> $items,
	): vec<T> {
		return Vec\map(
			$items,
			$item ==> {
				if ($item is T) {
					return $item;
				}
				throw new \Exception('bad');
			}
		);
	}
}