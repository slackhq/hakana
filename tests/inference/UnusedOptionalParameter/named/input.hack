<<file: __EnableUnstableFeatures('named_parameters', 'named_parameters_use')>>

function named_options(
	int $first,
	named int $omitted = 1,
	named int $supplied = 2,
): int { return $first + $omitted + $supplied; }

final class Example {
	public function __construct(public int $option = 1) {}
}

function production(): void {
	echo named_options(1, supplied=2);
	echo (new Example())->option;
}
