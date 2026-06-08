function add_num_int(num $a, int $b): num {
	return $a + $b;
}

function add_int_num(int $a, num $b): num {
	return $a + $b;
}

function add_num_num(num $a, num $b): num {
	return $a + $b;
}

function add_num_float(num $a, float $b): float {
	return $a + $b;
}

function multiply_num_int(num $a, int $b): num {
	return $a * $b;
}

function divide_num_num(num $a, num $b): num {
	return $a / $b;
}

// exponential-backoff pattern: num-returning helpers combined with ints
function next_job_delay(int $num_attempts, num $base): int {
	$delay = \HH\Lib\Math\maxva(60, $base) + 5;
	return $delay is int ? $delay : 3600;
}
