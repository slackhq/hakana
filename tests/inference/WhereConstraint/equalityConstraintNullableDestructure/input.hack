final class ExpectObj<T> {
	public function __construct(private T $var) {}

	public function assertNotNull(mixed $_v): void {}

	public function toNotBeNull<Tv>(): Tv where T = ?Tv {
		$val = $this->var;
		$this->assertNotNull($val);
		return $val as nonnull;
	}
}

function expect_obj<T>(T $x): ExpectObj<T> {
	return new ExpectObj($x);
}

function takes_nullable(?string $s): string {
	return expect_obj($s)->toNotBeNull();
}
