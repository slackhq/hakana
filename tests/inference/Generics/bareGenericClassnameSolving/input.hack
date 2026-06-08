interface MyMap<Tk as arraykey, +Tv> {}

abstract class TypeSpec<+T> {}

final class MapSpec<Tk as arraykey, Tv, T as MyMap<Tk, Tv>> extends TypeSpec<T> {
	public function __construct(
		private classname<T> $what,
		private TypeSpec<Tk> $tsk,
		private TypeSpec<Tv> $tsv,
	) {}
}

function constmap<Tk as arraykey, Tv>(
	TypeSpec<Tk> $tsk,
	TypeSpec<Tv> $tsv,
): TypeSpec<MyMap<Tk, Tv>> {
	return new MapSpec(MyMap::class, $tsk, $tsv);
}
