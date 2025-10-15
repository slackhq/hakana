function blah(shape(?'val' => int) $x): shape(?'val' => string) {
	if (Shapes::keyExists($x, 'val')) {
		$x['val'] = "{$x['val']}";
	}
	return $x;
}
