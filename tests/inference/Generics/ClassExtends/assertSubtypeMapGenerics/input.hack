<<__Sealed(ResultSuccess::class, ResultError::class)>>
abstract class Result<+T, +TErr> {}

final class ResultSuccess<+T> extends Result<T, nothing> {
	public function __construct(private T $t) {}
	public function get(): T {
		return $this->t;
	}
}

final class ResultError<+T> extends Result<nothing, T> {
	public function __construct(private T $message) {}
    public function get(): nothing {
		throw new \Exception('bad');
	}
    public function getError(): T {
		return $this->message;
	}
}

function foo<T as arraykey>(Result<T, int> $a): void {
    if ($a is ResultError<_>) {
        echo $a->getError();
    } else {
		echo $a->get();
	}
}