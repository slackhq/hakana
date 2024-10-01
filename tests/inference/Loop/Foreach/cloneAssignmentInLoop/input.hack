final class A {
    public function __construct(public int $num) {}
}

function foo(A $a, vec<int> $nums): void {
    foreach ($nums as $num) {
    	$a = clone $a;
        $a->num += $num;
        echo $a->num;
    }
}