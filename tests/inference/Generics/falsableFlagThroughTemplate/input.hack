final class MyResultOk<+T> {
	public function __construct(private T $value) {}
}

function my_result_ok<T>(T $value): MyResultOk<T> {
	return new MyResultOk($value);
}

function direct_return(): string {
	return \json_encode(dict['a' => 1]);
}

function through_template(): MyResultOk<string> {
	return my_result_ok(\json_encode(dict['a' => 1]));
}
