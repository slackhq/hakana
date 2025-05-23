abstract class Result<+T, +TErr> {
	public function __construct(?string $trace) {}
	abstract public function get(): T;
}

final class ResultOk<+T> extends Result<T, nothing> {
	public function __construct(private T $t) {
		parent::__construct(null);
	}
	public function get(): T {
		return $this->t;
	}
}

final class ResultError extends Result<nothing, string> {
	public function __construct(private string $message) {
		parent::__construct(null);
	}
    public function get(): T {
		throw new \Exception('bad');
	}
}

function bar(): void {
    foo(HH\global_get('_GET')['arr']);
}

function foo(shape('a' => string) $args): void {
    $a = get_a_result($args);
    if ($a is ResultOk<_>) {
        echo $a->get();
    }
}

function get_a_result(shape('a' => string) $args): Result<string, nothing> {
    return new ResultOk($args['a']);
}