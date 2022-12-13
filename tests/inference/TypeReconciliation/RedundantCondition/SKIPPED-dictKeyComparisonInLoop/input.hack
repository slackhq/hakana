function foo(dict<string, mixed> $dict) {
	while (rand(0, 1)) {
		/* HAKANA_FIXME[PossiblyUndefinedStringArrayOffset] */
		$v = $dict['foo'] as Container<_>;
	}
}