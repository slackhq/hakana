final class A {
	private keyset<string> $scopes;

	public function __construct(keyset<string> $scopes) {
		$this->scopes = $scopes;
	}

	public function hasScope(): bool {
		$k = readonly $this->scopes;
		return HH\Lib\C\contains($k, 'foo');
	}

	public static function passReadonlyParam(readonly int $retry, int $count): int {
		if ($count > $retry) {
			return $count;
		}
		return static::passReadonlyParam($retry, $count + 1);
	}
}
