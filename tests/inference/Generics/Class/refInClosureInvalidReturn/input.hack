function foo(): bool {
	$ref = new HH\Lib\Ref(false);

	$a = (int $b) ==> {
		$ref->value = $b;
	};

	$a(0);

	if ($ref->value === true) {}

	return $ref->value;
}