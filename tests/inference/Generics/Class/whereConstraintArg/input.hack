final class Foo<T> {
	public function __construct(private T $var) {}

	public function has(string $s): void where T as vec<string> {
		$value = $this->var;

		$matchingResponses = Vec\filter($value, ($response) ==> $response == $s);

		if (C\is_empty($matchingResponses)) {}
	}
}