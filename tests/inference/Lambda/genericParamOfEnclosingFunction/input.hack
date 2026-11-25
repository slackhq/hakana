final class Box<T> {
	public function __construct(public T $value) {}
}

function make_box<T>(T $value): Box<T> {
	$f = (Box<T> $b): Box<T> ==> new Box<T>($b->value);
	return $f(new Box($value));
}

function takes_fn<T>((function(Awaitable<T>): T) $f): void {}

function stub<T>(): void {
	takes_fn((Awaitable<T> $_): T ==> {
		throw new \Exception('no');
	});
}

function plain<T>(T $value): T {
	$f = (T $b): T ==> $b;
	return $f($value);
}

final class Maker<Tc> {
	public function make<Tm>(Tc $c, Tm $m): (Tc, Tm) {
		$f = (Tc $a, Tm $b): (Tc, Tm) ==> tuple($a, $b);
		return $f($c, $m);
	}

	public static function nested<Tm>(Tm $m): Tm {
		$f = (Tm $a): Tm ==> {
			$g = (Tm $b): Tm ==> $b;
			return $g($a);
		};
		return $f($m);
	}
}

function nested<T>(T $value): T {
	$f = (T $a): T ==> {
		$g = function(T $b): T {
			return $b;
		};
		return $g($a);
	};
	return $f($value);
}

function wrong<T>(T $value): int {
	$f = (T $b): T ==> $b;
	return $f($value);
}
