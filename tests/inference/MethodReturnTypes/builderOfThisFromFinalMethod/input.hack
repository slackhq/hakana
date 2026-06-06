final class MyBuilder<T as MyDateTime> {
	public function __construct(private T $instance) {}
}

class MyDateTime {
	final public function withYear(int $_year): MyBuilder<this> {
		return new MyBuilder($this);
	}

	final public function plusYears(int $years): MyBuilder<this> {
		return $this->withYear($years);
	}
}
