type a_t = shape(
	'a' => string,
	'b' => string,
	'c' => int,
	'd' => string,
	'e' => string,
	...
);

type b_t = shape(
	?'a' => string,
	?'b' => string,
	?'c' => int,
	?'d' => string,
	?'e' => string,
	?'f' => int,
);

type c_t = shape(
    'a' => string,
	'b' => string,
	'c' => int,
	'd' => string,
	'e' => string,
	?'f' => int,
);

function foo(a_t $a): c_t {
    return $a as b_t;
}