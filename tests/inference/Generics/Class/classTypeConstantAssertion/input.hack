abstract class A {
	<<__Enforceable>>
	abstract const type TClass as B;
    
    private static function getBillingItem(mixed $foo): ?this::TClass {
        if ($foo is this::TClass) {
            return $foo;
        }
		return null;
	}
}

abstract class B {}