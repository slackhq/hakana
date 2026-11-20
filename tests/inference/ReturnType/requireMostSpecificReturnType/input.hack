abstract class ResultBase<+T, +TErr> {}

final class ResultOk<+T> extends ResultBase<T, nothing> {
	public function __construct(private T $t) {}
}

final class ResultError<+TErr> extends ResultBase<nothing, TErr> {
	public function __construct(private string $message) {}
}

<<file: __EnableUnstableFeatures('case_types')>>
<<Hakana\RequireMostSpecificReturnType>>
case type ResultWithError<+T, +TErr> as ResultBase<T, TErr> = ResultOk<T> | ResultError<TErr>;

type Result<+T> = ResultWithError<T, mixed>;

// Always ok, through the alias of the alias.
function always_ok(int $i): Result<int> {
	if ($i > 0) {
		return new ResultOk($i);
	}
	return new ResultOk(0);
}

// Always an error.
function always_error(): Result<int> {
	return new ResultError('nope');
}

// Naming the case type itself.
function always_ok_direct(): ResultWithError<int, string> {
	return new ResultOk(1);
}

// Async, and nullable.
async function always_ok_async(Awaitable<int> $gen): Awaitable<?Result<int>> {
	$i = await $gen;
	if ($i > 0) {
		return null;
	}
	return new ResultOk($i);
}

// Either: fine.
function either(int $i): Result<int> {
	if ($i > 0) {
		return new ResultOk($i);
	}
	return new ResultError('negative');
}

// Passes a Result on: fine.
function passes_on(int $i): Result<int> {
	return either($i);
}

// Already as specific as it gets.
function specific(): ResultOk<int> {
	return new ResultOk(1);
}

// Only throws: nothing to say.
function only_throws(): Result<int> {
	throw new Exception('no');
}

abstract class Base {
	// Overridden below with a method that can fail: fine.
	public function overridden(): Result<int> {
		return new ResultOk(1);
	}

	// Not overridden.
	public function not_overridden(): Result<int> {
		return new ResultOk(1);
	}

	abstract public function abstract_method(): Result<int>;
}

final class Child extends Base {
	// Overrides follow the return type of the method they override.
	<<__Override>>
	public function overridden(): Result<int> {
		return new ResultError('child');
	}

	<<__Override>>
	public function abstract_method(): Result<int> {
		return new ResultOk(1);
	}
}

trait T {
	// Trait methods are left alone.
	public function in_trait(): Result<int> {
		return new ResultOk(1);
	}
}

function closures(): void {
	// Closures are left alone.
	$f = (): Result<int> ==> new ResultOk(1);
}

// Suppressed above the function, and between a method's attributes and its
// modifiers.
/* HAKANA_IGNORE[OverlyBroadReturnType] tests stub this with an error */
function suppressed(): Result<int> {
	return new ResultOk(1);
}

final class Suppressed {
	<<__Memoize>>
	/* HAKANA_IGNORE[OverlyBroadReturnType] tests stub this with an error */
	public static function suppressed(): Result<int> {
		return new ResultOk(1);
	}
}

function exits(): noreturn {
	exit(0);
}

// The last return is not reached by the analysis, and returns the other
// class.
function returns_after_exit(int $i): Result<int> {
	if ($i < 0) {
		return new ResultError('negative');
	}
	exits();

	/* HAKANA_IGNORE[UnevaluatedCode] only happens in tests */
	return new ResultOk($i);
}

final class FinalWithMock {
	// A mock class extends this final class and overrides the method.
	public function mocked(): Result<int> {
		return new ResultOk(1);
	}
}

/* HH_FIXME[4035] */
final class MockOfFinal extends FinalWithMock {
	<<__Override>>
	public function mocked(): Result<int> {
		return new ResultError('mocked');
	}
}

// Methods of generated classes are left to their generator.
<<Codegen>>
abstract class GeneratedBase {
	final protected function generated(): Result<int> {
		return new ResultOk(1);
	}
}
