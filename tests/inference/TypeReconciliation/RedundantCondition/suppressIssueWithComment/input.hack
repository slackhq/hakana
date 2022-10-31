function foo(string $a): void {
	echo(/* HAKANA_IGNORE[RedundantNonnullTypeComparison] */ $a as nonnull);
}