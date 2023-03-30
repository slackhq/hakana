function foo(): void {
	$b = 0;
	/* HHAST_FIXME[UnusedVariable] */ $a = 5;
	echo $a;
	echo $b;
}
