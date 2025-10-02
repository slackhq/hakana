namespace {
	abstract class P {}

	final class C extends P {
		public static function static_string_method(int $ignored, string $s, mixed $foo): void {}
		public function string_method(string $s, int $x): void {}

		public function foo(): void {
			$this->string_method(self::class, 4);
			$this->string_method(static::class, 5);
			$this->string_method(parent::class, 5);
		}
	}

	function string_function(string $s): void {}

	function classname_function(classname<C> $cls): void {}

	function caller(): void {
		$c = new C();
		$int = 5;
		C::static_string_method($int, C::class, 5.6);
		string_function(C::class);
		$c->string_method(C::class, 4);

		string_function(\N\E::class);

		/* HAKANA_FIXME[ClassnameUsedAsString] Using C in this position will lead to an implicit runtime conversion to string, please use "nameof C" instead */
		$c->string_method(C::class, 4);

		classname_function(C::class);

		$cls_var = C::class;
		string_function($cls_var);
	}
}

namespace N {
	final class D {}

	final class E {
		public static function foo(): void {
			\string_function(D::class);
			\string_function(\C::class);
			\string_function(self::class);
		}
	}
}
