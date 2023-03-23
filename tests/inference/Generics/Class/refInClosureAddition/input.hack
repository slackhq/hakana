function foo(): int {
	$ref = new HH\Lib\Ref(0);

	$a = () ==> {
        if (rand(0, 1)) {
            $ref->value++;
        }
	};

	$a();
    $a();
    $a();

	if ($ref->value === 0) {}
    if ($ref->value === 1) {}
    if ($ref->value !== 2) {}

	return $ref->value;
}