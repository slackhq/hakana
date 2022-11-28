enum Foo: int {
	A = 0;
    B = 1;
    C = 2;
}

function bar(): int {
	return Foo::C as int;
}