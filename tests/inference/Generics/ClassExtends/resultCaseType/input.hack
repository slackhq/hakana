abstract class ResultBase<+T, +TErr> {}

final class ResultOk<+T> extends ResultBase<T, nothing> {
	public function __construct(private T $t) {}
	public function get(): T {
		return $this->t;
	}
}

final class ResultError<+T> extends ResultBase<nothing, T> {
	public function __construct(private T $message) {}
    public function getError(): T {
		return $this->message;
	}
}

<<file: __EnableUnstableFeatures('case_types')>>
case type Result<+T, +TErr> = ResultOk<T> | ResultError<TErr>;

function foo<T as arraykey>(Result<T, int> $a): void {
    if ($a is ResultError<_>) {
        echo $a->getError();
    } else {
		echo $a->get();
	}
}