type foo_t = shape(
	?'a' => num,
	?'b' => num,
);

function bar(): foo_t {
	return shape(
		'a' => 0,
        'b' => 0,
	);
}

function foo(): void {
	$output = bar();
	$output['a'] = \rand(0, 1) ? 5 : null;
	
	if ($output is foo_t) {}
}