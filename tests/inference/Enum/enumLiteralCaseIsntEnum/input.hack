function bar(string $s): string {
	if ($s === Foo::A) {
	  return $s;
	}
	return 'a';
}

enum Foo: string {
	A = 'a';
	B = 'b';
}