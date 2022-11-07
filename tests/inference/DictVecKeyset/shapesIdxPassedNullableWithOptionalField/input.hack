function baz(?shape(?'maybe_defined' => string) $s) {
	$a = Shapes::idx($s, 'maybe_defined');
    if ($a is nonnull) {}
}