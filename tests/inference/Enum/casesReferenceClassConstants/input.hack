final class SomeClass {
  const string A = 'a';
  const string B = 'b';
  const string C = 'c';
}

enum SomeEnum: string as string {
	A = SomeClass::A;
    B = SomeClass::B;
    C = SomeClass::C;
}

function foo(SomeEnum $e): void {
	switch ($e) {
		case SomeEnum::A:
			break;
		case SomeEnum::B:
			break;
		default:
			break;
	}

    echo SomeEnum::C;
}