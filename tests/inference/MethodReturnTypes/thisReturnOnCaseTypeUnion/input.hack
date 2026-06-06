abstract class MyResultBase<+T> {
	abstract public function map<Tm>((function(T): Tm) $map): MyResultWithError<Tm>;
}

final class MyResultOk<+T> extends MyResultBase<T> {
	public function __construct(private T $value) {}

	public function get(): T {
		return $this->value;
	}

	<<__Override>>
	public function map<Tm>((function(T): Tm) $map): MyResultOk<Tm> {
		return new MyResultOk($map($this->get()));
	}
}

final class MyResultError extends MyResultBase<nothing> {
	<<__Override>>
	public function map<Tm>(mixed $_map): this {
		return $this;
	}
}

case type MyResultWithError<+T> as MyResultBase<T> = MyResultOk<T> | MyResultError;

function get_result(): MyResultWithError<int> {
	return new MyResultOk(5);
}

final class BookmarksLikeHandler {
	public function getResult(): MyResultWithError<shape('a' => int)> {
		$res = get_result();
		$ret = $res->map($x ==> shape('a' => $x));
		return $ret;
	}
}
