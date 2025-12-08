abstract class A {}

interface I1 {
	public function doSomething(string $s): void;
}

function foo(A $a, string $s): void {
	if ($a is I1) {
        $a::doSomething($s);
    }
}
