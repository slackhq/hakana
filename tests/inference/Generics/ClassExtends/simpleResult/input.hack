abstract class Result<+T, +TErr> {
	public function orNull(): ?T {
		return $this is ResultOk<_> ? $this->get() : null;
	}
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
}
