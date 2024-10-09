enum M: int as int {
	A = 0;
	B = 1;
	C = 2;
	D = 3;
	E = 4;
}

const keyset<M> FOO = Bar::FOO;

final class Bar {
    const keyset<M> FOO = keyset[M::A, M::C, M::E];
}

function baz(M $i): void {
    if (C\contains(FOO, $i)) {}
}