abstract class Result<+T> {
	public function getOrNull(): ?T {
		if ($this is ResultSuccess<_>) {
			return $this->get();
		}

		return null;
	}
	abstract public function get(): T;
}

final class ResultSuccess<+T> extends Result<T> {
	public function __construct(private T $t) {}

	public function get(): T {
		return $this->t;
	}
}

function handleUnsafe(): void {
    $res = new ResultSuccess($_GET['a']);
    $res->getOrNull();
}

function safe(string $a): void {
    $res = new ResultSuccess($a);
    echo $res->getOrNull();
}