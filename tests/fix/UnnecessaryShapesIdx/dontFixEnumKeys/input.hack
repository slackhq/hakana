function baz(shape(?'maybe_defined' => shape('conditionally_defined' => string)) $s) {
	$a = Shapes::idx($s, 'maybe_defined', shape());
	$b = Shapes::idx($a, 'conditionally_defined');

    $c = getNullableShape() ?? shape();
	$d = Shapes::idx($c, 'a');
}

enum E: string {
    A = 'a';
    B = 'b';
}

function getNullableShape(): ?shape(E::A => string, ...) {
    return shape(E::A => 'cool');
}