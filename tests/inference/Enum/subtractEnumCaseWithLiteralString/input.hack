enum Bar: string {
	A = 'normal';
	B = 'abnormal';
	C = 'weird';
}

function foo(Bar $t): void {
	if ($t !== 'normal') {}
}