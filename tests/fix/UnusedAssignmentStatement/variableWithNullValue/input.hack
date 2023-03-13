function foo(): void {
	/* HAKANA_FIXME[UnusedAssignment] */
	$a = null;
	$b = 0;
	echo $b;
	/* HAKANA_FIXME[UnusedAssignmentStatement] */
	$c = Str\replace("a", "b", "c");
}
