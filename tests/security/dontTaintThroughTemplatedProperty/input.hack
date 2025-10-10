abstract class Result<+T> {
	public function getOrNull(): ?T {
		if ($this is ResultOk<_>) {
			return $this->get();
		}

		return null;
	}
	abstract public function get(): T;
}

final class ResultOk<+T> extends Result<T> {
	public function __construct(private T $t) {}

	<<__Override>>
	public function get(): T {
		return $this->t;
	}
}

function handleUnsafe(): void {
    $res = new ResultOk(HH\global_get('_GET')['a']);
    $res->getOrNull();
}

function safe(string $a): void {
    $res = new ResultOk($a);
    echo $res->getOrNull();
}