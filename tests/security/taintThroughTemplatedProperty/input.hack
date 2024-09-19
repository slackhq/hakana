abstract class Result<+T, +TErr> {
	public function orNull(): ?T {
		return $this is ResultSuccess<_> ? $this->get() : null;
	}
	abstract public function get(): T;
}

final class ResultSuccess<+T> extends Result<T, nothing> {
	public function __construct(private T $t) {}

	public function get(): T {
		return $this->t;
	}
}

function returnGetResult(): Result<string> {
    return new ResultSuccess(HH\global_get('_GET')['a']);
}

function doTheDangerousThing(): void {
    $res = returnGetResult();
    echo $res->orNull();
}