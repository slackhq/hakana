final class C {
	public static function static_string_method(int $ignored, string $s, mixed $foo): void {}
	public function string_method(string $s, int $x): void {}
}

function string_function(string $s): void {}

function classname_function(classname<C> $cls): void {}

function caller(): void {
	$c = new C();
	$int = 5;
	C::static_string_method($int, C::class, 5.6);
	string_function(C::class);
	$c->string_method(C::class, 4);

	classname_function(C::class);
}
