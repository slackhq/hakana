abstract class A {
	abstract const type T;
	
	abstract public function getValueInner(): this::T;
	
	public function getValue(): this::T {
		return $this->getValueInner();
	}
}

final class B extends A {
	const type T = vec<string>;
	
	public function getValueInner(): this::T {
		return vec["a"];
	}
	
	public function getValue(): this::T {
		$value = parent::getValue();
		$first = C\first($value);
		if ($first is nonnull) {}
		return $value;
	}
}

