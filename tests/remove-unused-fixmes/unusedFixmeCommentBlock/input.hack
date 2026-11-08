function foo(string $s) {
	/* HAKANA_FIXME[RedundantTypeComparison] Type true is always truthy a */
	/* HAKANA_FIXME[ImpossibleNullTypeComparison] Type true is always truthy a */
	do {
		$result = rand(0,1) === 1;
	} while (!$result);

	if (/* HAKANA_FIXME[RedundantTruthinessCheck] Type true is always truthy a */$result) {}

	if (
		/* HAKANA_FIXME[RedundantTruthinessCheck] Type true is always truthy a */
		$result
	) {}
}