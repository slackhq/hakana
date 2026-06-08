final class LookupResult<T> {
	public function __construct(public T $value) {}
}

abstract class ConnectorHandler<T> {
	abstract protected function buildResult(): Awaitable<T>;
}

abstract class BaseHandler<T> extends ConnectorHandler<LookupResult<T>> {}

abstract class MultiAuthHandler<T> extends BaseHandler<T> {
	protected async function buildResultWrapped(): Awaitable<LookupResult<T>> {
		return await $this->buildResult();
	}
}
