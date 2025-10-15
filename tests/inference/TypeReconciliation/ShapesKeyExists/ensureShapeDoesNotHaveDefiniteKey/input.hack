function blah(shape(?'val' => int) $x): void {
	if (Shapes::keyExists($x, 'val')) {
		$x['val'] = "{$x['val']}";
	}

    echo $x['val'];
}
