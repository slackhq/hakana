<<Hakana\SecurityAnalysis\SpecializeInstance()>>
abstract class Result<+T, +TErr> {
	public function orNull(): ?T {
		return $this is ResultSuccess<_> ? $this->get() : null;
	}
	abstract public function get(): T;
}

<<Hakana\SecurityAnalysis\SpecializeInstance()>>
final class ResultSuccess<+T> extends Result<T, nothing> {
	public function __construct(private T $t) {}

	public function get(): T {
		return $this->t;
	}
}

function returnGetResult(): Result<string> {
    return new ResultSuccess($_GET['a']);
}

function doTheDangerousThing(): void {
    $res = returnGetResult();
    echo $res->orNull();
}