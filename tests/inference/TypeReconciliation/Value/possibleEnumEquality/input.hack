enum Foo: string {
	C = 'C';
	D = 'D';
	G = 'G';
}

function a(Foo $foo): void {
  if ($foo === 'C') {}
}

function b(Foo $foo): void {
  if ($foo !== 'C') {}
}

function c(Foo $foo, string $s): void {
  if ($foo === $s) {}
}

function d(Foo $foo, string $s): void {
  if ($foo !== $s) {}
}

function e(Foo $foo, string $s): void {
  if ($s !== $foo) {}
}
