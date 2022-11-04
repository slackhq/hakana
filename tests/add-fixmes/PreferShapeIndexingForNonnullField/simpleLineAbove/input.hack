type shape_t = shape(
	'a' => ?string,
	'always' => string,
	?'nullable' => string
);
function foo(shape_t $s) {
	$type = Shapes::idx($s, 'always');
	$type = Shapes::idx($s, 'a');
	$type = Shapes::idx($s, 'b');
	$type = Shapes::idx($s, 'nullable');

	$type = Shapes::idx($s, 'always', "useless default");
	$type = Shapes::idx($s, 'nullable', "default");
}