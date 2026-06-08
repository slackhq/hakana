<<__Sealed(Child::class)>>
class UnusedMethodClass {
    <<Hakana\MustUse>>
	public function getEncodedId(): string {
		return '';
	}

	public function doWork(): string {
		return '';
	}

	<<Hakana\MustUse>>
	public static function staticMustUse(): string {
		return '';
	}

	public function doesNotUse(): void {
		self::staticMustUse();

		static::staticMustUse();
	}
}

final class Child extends UnusedMethodClass {
	public function childDoesNotUse(): void {
		parent::getEncodedId();
	}
}

function foo(): void {
    $c = new UnusedMethodClass();
    $c->getEncodedId();
    $c->doWork();

	UnusedMethodClass::staticMustUse();
}
