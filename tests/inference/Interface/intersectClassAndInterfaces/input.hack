abstract class A {}

interface I1 {}

interface I2 {}

function foo(A $a): void {
	if ($a is I1 && $a is I2) {}
}

function bar(A $a): void {
	if ($a is I1 && !($a is I2)) {
		$a = get_class($a);
		echo $a;
	}
}
