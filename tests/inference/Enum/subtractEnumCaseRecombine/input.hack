enum SomeEnum: int {
	A = 0;
	B = 1;
	C = 2;
}

function bar(SomeEnum $step): void {
    if ($step == SomeEnum::C) { }
    foo($step);
	echo($step as int);
}

function foo(string $e): void {}