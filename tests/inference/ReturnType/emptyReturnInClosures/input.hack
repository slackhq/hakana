function trace_sync<T>(string $name, (function(): T) $fn): T {
	return $fn();
}

function run_void_cb((function(): void) $fn): void {
	$fn();
}

function generic_callback_case(bool $b): void {
	trace_sync('x', () ==> {
		if ($b) {
			return;
		}
		echo 'hi';
	});
}

function explicit_void_case(bool $b): int {
	run_void_cb(() ==> {
		if ($b) {
			return;
		}
		echo 'hi';
	});
	return 5;
}

function local_lambda_case(bool $b): vec<string> {
	$noop = (int $_input) ==> {
		if ($b) {
			return;
		}
		echo 'hi';
	};
	$noop(1);
	return vec[];
}

function genuinely_bad(bool $b): int {
	if ($b) {
		return;
	}
	return 5;
}
