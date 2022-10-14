class ExpectObj<T> extends Assert {
  public function __construct(private T $var) {}

  public function toThrow<TException as Throwable>(
    classname<TException> $exception_class
  ): TException where T = (function(): mixed) {
  }
}

final class CustomExpectObj<T> extends ExpectObj<T> {
}

function takesCustomExpectObj(CustomExpectObj<(function(): void)> $expect): void {
	$expect->toThrow(Exception::class);
}