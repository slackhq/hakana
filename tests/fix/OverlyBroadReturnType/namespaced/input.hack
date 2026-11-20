namespace Foo;

function always_ok(): \Result<int> {
	return new \ResultOk(1);
}

final class Bar {
	public function alwaysError(): \Result<int> {
		return new \ResultError('nope');
	}
}
