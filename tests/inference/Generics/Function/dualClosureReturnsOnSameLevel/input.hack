<<__Sealed(ResultSuccess::class, ResultError::class)>>
abstract class Result<+T, +TErr> {}

final class ResultSuccess<+T> extends Result<T, nothing> {
	public function __construct(private T $t) {}
	public function get(): T {
		return $this->t;
	}
}

final class ResultError<+T> extends Result<nothing, T> {
	public function __construct(private T $message) {}
    public function get(): nothing {
		throw new \Exception('bad');
	}
    public function getError(): T {
		return $this->message;
	}
}

async function foo<T>((function():Awaitable<Result<T, mixed>>) $one, (function():Awaitable<Result<T, mixed>>) $two): Awaitable<Result<?T, mixed>> {
    if (rand(0, 1)) {
        return await $one();
    }
    return await $two();
}

async function get_int_result(): Awaitable<Result<int, nothing>> {
    return new ResultSuccess(5);
}

async function bar(): Awaitable<void> {
    $a = await foo(async () ==> await get_int_result(), async () ==> new ResultSuccess(null));

    if ($a is ResultSuccess<_>) {
        $b = $a->get();

        if ($b is int) {
            // do something
        }

        if ($b is null) {}
    }
}