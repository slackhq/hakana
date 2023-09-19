enum Foo: int {
	C = 0;
	D = 1;
	G = 2;
}

function a(Foo $foo): void {
  if ($foo == 'C') {}
}

function b(Foo $foo): void {
  if ($foo != 'C') {}
}

function c(Foo $foo, string $s): void {
  if ($foo == $s) {}
}

function d(Foo $foo, string $s): void {
  if ($foo != $s) {}
}