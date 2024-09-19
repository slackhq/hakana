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

function foo(): Result<string, string> {
    $a = get_a_result();
    if ($a is ResultError<_>) {
        return $a;
    }
    return new ResultSuccess("cool");
}

function bar(): void {
    $b = foo();
    if ($b is ResultError<_>) {
        echo $b->getError();
    }
}

function get_a_result(): Result<string, string> {
    if (rand(0, 1)) {
        return new ResultError(HH\global_get('_GET')['bad']);
    }
    return new ResultSuccess("good");
}