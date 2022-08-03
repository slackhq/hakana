abstract class Result<T> {
	abstract public function get(): T;
	abstract public function is_ok(): bool;

	// public function or<T2in, T2 as T|T2in>(T2in $default): T2 {
	//	return $this->is_ok() ? $this->get() : $default;
	// }

	public function or<T2 super T>(T2 $default): T2 {
		if ($this->is_ok()) {
			return $this->get();
		}
		
		return $default;
	}
}

function use_result_again(Result<string> $res): string {
    return $res->or("foo");
}