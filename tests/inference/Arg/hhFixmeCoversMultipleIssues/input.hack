function takes_two_bools(bool $a, bool $b): bool {
	return $a && $b;
}

function f(): void {
	// like Hack's fixme_provider, one fixme suppresses every matching
	// error on the line, not just the first
	/* HH_FIXME[4110] */
	if (!takes_two_bools(0, 1)) {
		echo "x";
	}
}
