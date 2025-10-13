<<__Sealed(ResultOk::class, ResultError::class)>>
abstract class Result<+T, +TErr> {
	abstract public function get(): T;
}

final class ResultOk<+T> extends Result<T, nothing> {
	public function __construct(private T $t) {}
	<<__Override>>
	public function get(): T {
		return $this->t;
	}
}

final class ResultError extends Result<nothing, string> {
	public function __construct(private string $message) {}
    <<__Override>>
    public function get(): nothing {
		throw new \Exception('bad');
	}
}

function foo(vec<Result<string>> $arr, ResultOk<int> $b): void {
    $arr[] = $b;

    $i = 0;
   
    foreach ($arr as $a) {
        if ($a is ResultError<_>) {
            return;
        }
        echo $a->get();
        $i += 1;
    }
}