function foo(string $s) {
	/* HAKANA_FIXME[RedundantTypeComparison] Type true is always truthy a */
	/* HAKANA_FIXME[ImpossibleNullTypeComparison] Type true is always truthy a */
	do {
		$result = (bool) rand(0,1);
	} while (!$result);

	if (
		$result
	) {}
}